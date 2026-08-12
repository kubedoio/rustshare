import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AttachmentPicker from './AttachmentPicker.svelte';
import { ApiError, type File } from '$lib/api/types';

const mocks = vi.hoisted(() => ({
	listAllFiles: vi.fn(),
	post: vi.fn()
}));

vi.mock('$lib/api/files', () => ({
	listAllFiles: mocks.listAllFiles
}));

vi.mock('$lib/api/client', () => ({
	apiClient: { post: mocks.post }
}));

const FILES: File[] = [
	{
		id: 'f-1',
		name: 'Plan.md',
		path: '/Plan.md',
		size: 10,
		mime_type: 'text/markdown',
		parent_folder_id: null,
		owner_id: 'u-1',
		current_version: 1,
		created_at: '2026-08-12T10:00:00Z',
		modified_at: '2026-08-12T10:00:00Z'
	},
	{
		id: 'f-2',
		name: 'Notes.md',
		path: '/Notes.md',
		size: 20,
		mime_type: 'text/markdown',
		parent_folder_id: null,
		owner_id: 'u-1',
		current_version: 1,
		created_at: '2026-08-12T10:00:00Z',
		modified_at: '2026-08-12T10:00:00Z'
	}
];

describe('AttachmentPicker', () => {
	beforeEach(() => {
		mocks.listAllFiles.mockReset();
		mocks.post.mockReset();
		mocks.listAllFiles.mockResolvedValue(FILES);
	});

	it('sends the exact prepare payload and emits the returned buzz tag', async () => {
		const onSelect = vi.fn();
		mocks.post.mockResolvedValue({
			buzz_tag: ['elembra-ref', 'elembra://io.elembra.files/file/f-1']
		});
		render(AttachmentPicker, { props: { onSelect } });
		await fireEvent.click(screen.getByRole('button', { name: 'Attach file' }));
		await waitFor(() => expect(screen.getByText('Plan.md')).toBeTruthy());
		await fireEvent.click(screen.getByRole('button', { name: 'Plan.md' }));
		await waitFor(() =>
			expect(mocks.post).toHaveBeenCalledWith('/applications/chat/attachments/prepare', {
				resource: {
					application: 'io.elembra.files',
					resourceType: 'file',
					resourceId: 'f-1'
				}
			})
		);
		expect(onSelect).toHaveBeenCalledWith(['elembra-ref', 'elembra://io.elembra.files/file/f-1']);
	});

	it('shows the error and does not emit when prepare fails with 404', async () => {
		const onSelect = vi.fn();
		mocks.post.mockRejectedValue(new ApiError(404, 'not found'));
		render(AttachmentPicker, { props: { onSelect } });
		await fireEvent.click(screen.getByRole('button', { name: 'Attach file' }));
		await waitFor(() => expect(screen.getByText('Plan.md')).toBeTruthy());
		await fireEvent.click(screen.getByRole('button', { name: 'Plan.md' }));
		await waitFor(() => expect(screen.getByText('not found')).toBeTruthy());
		expect(onSelect).not.toHaveBeenCalled();
	});
});
