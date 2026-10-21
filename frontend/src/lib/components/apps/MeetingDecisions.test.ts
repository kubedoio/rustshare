import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import MeetingDecisions from './MeetingDecisions.svelte';

const mocks = vi.hoisted(() => ({
	listDecisions: vi.fn(),
	createDecision: vi.fn(),
	linkDecision: vi.fn(),
	unlinkDecision: vi.fn(),
	searchResources: vi.fn(),
	toast: vi.fn(),
	authValue: { id: 'user-1', tenant_id: 'tenant-1' },
	authSubscribers: [] as Array<(user: { id: string; tenant_id: string } | null) => void>
}));
vi.mock('$lib/api/meetings', () => ({ meetingsApi: mocks }));
vi.mock('$lib/stores/toast', () => ({ toastStore: { show: mocks.toast } }));
vi.mock('$lib/stores/auth', () => ({
	currentUser: {
		subscribe(run: (user: { id: string; tenant_id: string } | null) => void) {
			run(mocks.authValue);
			mocks.authSubscribers.push(run);
			return () => {
				mocks.authSubscribers = mocks.authSubscribers.filter((subscriber) => subscriber !== run);
			};
		}
	}
}));

describe('MeetingDecisions', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		sessionStorage.clear();
		mocks.authValue = { id: 'user-1', tenant_id: 'tenant-1' };
		mocks.authSubscribers = [];
		mocks.listDecisions.mockResolvedValue({
			items: [
				{
					id: 'visible-link',
					available: true,
					created_at: '2026-10-20T10:00:00Z',
					decision: {
						id: 'decision-1',
						name: 'DEC-001.md',
						title: 'Approve budget',
						status: 'accepted',
						decision_date: '2026-10-20',
						owner_id: 'owner-1',
						owner_display_name: 'Alex',
						open_url: '/apps/decisions/decision-1'
					}
				},
				{ id: 'hidden-link', available: false, created_at: '2026-10-20T10:01:00Z' }
			],
			total: 2,
			page: 1,
			per_page: 50
		});
		mocks.createDecision.mockResolvedValue({ id: 'created-link', available: true });
		mocks.linkDecision.mockResolvedValue({ id: 'linked-link', available: true });
		mocks.unlinkDecision.mockResolvedValue(undefined);
		mocks.searchResources.mockResolvedValue([
			{
				resource_ref: 'elembra://io.elembra.files/file/a1111111-1111-4111-8111-111111111111',
				title: 'Existing decision',
				location:
					'/Workspace/Decisions/DEC-a1111111-1111-4111-8111-111111111111-Existing-decision.md',
				source_application: 'io.elembra.files',
				source_type: 'file'
			},
			{
				resource_ref: 'elembra://io.elembra.files/file/22222222-2222-4222-8222-222222222222',
				title: 'Ordinary note',
				location: '/Workspace/Notes/Ordinary/note.md',
				source_application: 'io.elembra.files',
				source_type: 'file'
			}
		]);
	});

	it('opens authorized decisions and removes unavailable links without deleting targets', async () => {
		render(MeetingDecisions, { props: { meetingId: 'meeting-1' } });
		expect((await screen.findByRole('link', { name: 'Approve budget' })).getAttribute('href')).toBe(
			'/apps/decisions/decision-1'
		);
		expect(screen.getByText(/accepted/)).toBeTruthy();
		expect(screen.getByText(/Owner: Alex/)).toBeTruthy();
		expect(screen.getByText('2026-10-20')).toBeTruthy();
		expect(screen.queryByText('owner-1')).toBeNull();
		expect(screen.getByText('Linked decision unavailable')).toBeTruthy();
		expect(screen.queryByText('Private acquisition')).toBeNull();
		await fireEvent.click(screen.getAllByRole('button', { name: 'Remove decision link' })[1]);
		await waitFor(() =>
			expect(mocks.unlinkDecision).toHaveBeenCalledWith('meeting-1', 'hidden-link')
		);
		expect(mocks.createDecision).not.toHaveBeenCalled();
	});

	it('recovers the request identity and draft after an ambiguous creation failure and reload', async () => {
		mocks.createDecision.mockRejectedValueOnce(new Error('response interrupted'));
		const firstView = render(MeetingDecisions, { props: { meetingId: 'meeting-1' } });
		await screen.findByText('Linked decision unavailable');
		await fireEvent.click(screen.getByRole('button', { name: 'Create Decision' }));
		await fireEvent.input(screen.getByLabelText('Decision title'), {
			target: { value: 'Adopt proposal' }
		});
		await fireEvent.input(screen.getByLabelText('Decision content'), {
			target: { value: 'Approved after review.' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Create decision' }));
		await screen.findByRole('button', { name: 'Retry creation' });
		const firstRequest = mocks.createDecision.mock.calls[0][1];
		expect(firstRequest.request_id).toMatch(/^[0-9a-f-]{36}$/i);
		expect(
			JSON.parse(sessionStorage.getItem('meeting-decision-request:tenant-1:user-1:meeting-1')!)
		).toEqual(firstRequest);
		firstView.unmount();
		render(MeetingDecisions, { props: { meetingId: 'meeting-1' } });
		await screen.findByText('Approve budget');
		await fireEvent.click(screen.getByRole('button', { name: 'Create Decision' }));
		await screen.findByRole('button', { name: 'Retry creation' });
		expect((screen.getByLabelText('Decision title') as HTMLInputElement).value).toBe(
			'Adopt proposal'
		);
		expect((screen.getByLabelText('Decision content') as HTMLTextAreaElement).value).toBe(
			'Approved after review.'
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Retry creation' }));
		await waitFor(() => expect(mocks.createDecision).toHaveBeenCalledTimes(2));
		expect(mocks.createDecision.mock.calls[1]).toEqual(['meeting-1', firstRequest]);
		await waitFor(() =>
			expect(
				sessionStorage.getItem('meeting-decision-request:tenant-1:user-1:meeting-1')
			).toBeNull()
		);
	});

	it('does not restore a pending draft for a different meeting viewer', async () => {
		mocks.createDecision.mockRejectedValueOnce(new Error('response interrupted'));
		const ownerView = render(MeetingDecisions, { props: { meetingId: 'meeting-private-draft' } });
		await screen.findByText('Linked decision unavailable');
		await fireEvent.click(screen.getByRole('button', { name: 'Create Decision' }));
		await fireEvent.input(screen.getByLabelText('Decision title'), {
			target: { value: 'Private draft title' }
		});
		await fireEvent.input(screen.getByLabelText('Decision content'), {
			target: { value: 'Private draft content' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Create decision' }));
		await screen.findByRole('button', { name: 'Retry creation' });
		ownerView.unmount();

		mocks.authValue = { id: 'viewer-2', tenant_id: 'tenant-1' };
		render(MeetingDecisions, { props: { meetingId: 'meeting-private-draft' } });
		await screen.findByText('Linked decision unavailable');
		await fireEvent.click(screen.getByRole('button', { name: 'Create Decision' }));
		expect((screen.getByLabelText('Decision title') as HTMLInputElement).value).toBe('');
		expect((screen.getByLabelText('Decision content') as HTMLTextAreaElement).value).toBe('');
		expect(screen.queryByRole('button', { name: 'Retry creation' })).toBeNull();
		expect(screen.queryByText('Private draft content')).toBeNull();
	});

	it('links an accessible Decision by ID without copying its contents', async () => {
		render(MeetingDecisions, { props: { meetingId: 'meeting-2' } });
		await screen.findByText('Linked decision unavailable');
		await fireEvent.click(screen.getByRole('button', { name: 'Link Decision' }));
		await fireEvent.input(screen.getByRole('textbox', { name: 'Search accessible Decisions' }), {
			target: { value: 'Existing' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Search' }));
		await screen.findByText('Existing decision');
		expect(screen.queryByText('Ordinary note')).toBeNull();
		await fireEvent.click(screen.getByRole('button', { name: 'Link' }));
		await waitFor(() =>
			expect(mocks.linkDecision).toHaveBeenCalledWith(
				'meeting-2',
				'a1111111-1111-4111-8111-111111111111'
			)
		);
		expect(mocks.createDecision).not.toHaveBeenCalled();
	});

	it('keeps the newest search result when responses arrive out of order', async () => {
		let resolveOld: (value: unknown) => void = () => {};
		mocks.searchResources.mockImplementationOnce(
			() => new Promise((resolve) => (resolveOld = resolve))
		);
		mocks.searchResources.mockResolvedValueOnce([
			{
				resource_ref: 'elembra://io.elembra.files/file/b2222222-2222-4222-8222-222222222222',
				title: 'Newest decision',
				location: '/Workspace/Decisions/DEC-newest.md'
			}
		]);
		render(MeetingDecisions, { props: { meetingId: 'meeting-search-race' } });
		await screen.findByText('Linked decision unavailable');
		await fireEvent.click(screen.getByRole('button', { name: 'Link Decision' }));
		const input = screen.getByRole('textbox', { name: 'Search accessible Decisions' });
		await fireEvent.input(input, { target: { value: 'older query' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Search' }));
		await fireEvent.input(input, { target: { value: 'newer query' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Search' }));
		await screen.findByText('Newest decision');
		resolveOld([
			{
				resource_ref: 'elembra://io.elembra.files/file/a1111111-1111-4111-8111-111111111111',
				title: 'Stale decision',
				location: '/Workspace/Decisions/DEC-stale.md'
			}
		]);
		await waitFor(() => expect(screen.queryByText('Stale decision')).toBeNull());
		expect(screen.getByText('Newest decision')).toBeTruthy();
	});

	it('does not reveal stale meeting decisions after navigation', async () => {
		let resolveOld: (value: unknown) => void = () => {};
		mocks.listDecisions.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveOld = resolve;
				})
		);
		const view = render(MeetingDecisions, { props: { meetingId: 'meeting-old' } });
		await waitFor(() => expect(mocks.listDecisions).toHaveBeenCalledWith('meeting-old', 1));
		mocks.listDecisions.mockResolvedValue({ items: [], total: 0, page: 1, per_page: 50 });
		await view.rerender({ meetingId: 'meeting-new' });
		await screen.findByText('No linked decisions yet.');
		resolveOld({
			items: [
				{
					id: 'old',
					available: true,
					decision: { title: 'Old private title', open_url: '/apps/decisions/old' }
				}
			],
			total: 1,
			page: 1,
			per_page: 50
		});
		await waitFor(() => expect(screen.queryByText('Old private title')).toBeNull());
	});
});
