import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import MeetingResourceLinks from './MeetingResourceLinks.svelte';

const mocks = vi.hoisted(() => ({
	listResources: vi.fn(),
	linkResource: vi.fn(),
	unlinkResource: vi.fn(),
	searchResources: vi.fn(),
	toast: vi.fn()
}));

vi.mock('$lib/api/meetings', () => ({ meetingsApi: mocks }));
vi.mock('$lib/stores/toast', () => ({ toastStore: { show: mocks.toast } }));

describe('MeetingResourceLinks', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.listResources.mockResolvedValue({
			items: [
				{
					id: 'visible-link',
					available: true,
					created_at: '2026-10-20T10:00:00Z',
					display_name: 'agenda.pdf',
					resource_kind: 'file',
					open_url: '/files?preview=file-1'
				},
				{
					id: 'hidden-link',
					available: false,
					created_at: '2026-10-20T10:01:00Z'
				}
			],
			total: 2,
			page: 1,
			per_page: 50
		});
		mocks.searchResources.mockResolvedValue([
			{
				resource_ref: 'elembra://io.elembra.files/file/file-2',
				title: 'Accessible note',
				location: '/Workspace/Notes/Accessible note/note.md',
				source_application: 'io.elembra.files',
				source_type: 'file'
			}
		]);
		mocks.linkResource.mockResolvedValue({ id: 'new-link', available: true });
		mocks.unlinkResource.mockResolvedValue(undefined);
	});

	it('shows inaccessible links without resource metadata and removes only the link', async () => {
		render(MeetingResourceLinks, { props: { meetingId: 'meeting-1' } });

		expect((await screen.findByRole('link', { name: /agenda\.pdf/ })).getAttribute('href')).toBe(
			'/files?preview=file-1'
		);
		expect(screen.getByText('Linked resource unavailable')).toBeTruthy();
		expect(screen.queryByText('private-budget.xlsx')).toBeNull();

		const removeButtons = screen.getAllByRole('button', { name: 'Remove linked resource' });
		await fireEvent.click(removeButtons[1]);
		expect(mocks.unlinkResource).toHaveBeenCalledWith('meeting-1', 'hidden-link');
	});

	it('searches authorized resources and links them by canonical reference', async () => {
		render(MeetingResourceLinks, { props: { meetingId: 'meeting-2' } });
		await screen.findByText('Linked resource unavailable');

		await fireEvent.click(screen.getByRole('button', { name: 'Link a resource' }));
		await fireEvent.input(
			screen.getByRole('textbox', { name: 'Search accessible Files and Notes' }),
			{
				target: { value: 'Accessible note' }
			}
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Search' }));
		await screen.findByText('Accessible note');
		await fireEvent.click(screen.getByRole('button', { name: 'Link' }));

		await waitFor(() =>
			expect(mocks.linkResource).toHaveBeenCalledWith(
				'meeting-2',
				'elembra://io.elembra.files/file/file-2'
			)
		);
		expect(mocks.listResources).toHaveBeenCalledWith('meeting-2', 1);
	});
});
