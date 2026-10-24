import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import UploadProgress from './UploadProgress.svelte';

describe('UploadProgress', () => {
	it('allows the whole active queue to be cancelled', async () => {
		const onCancelAll = vi.fn();
		render(UploadProgress, {
			props: {
				tasks: [
					{
						id: 'uploading-1',
						fileName: 'large.zip',
						size: 10,
						status: 'uploading',
						progress: 20
					}
				],
				onCancelAll
			}
		});

		await fireEvent.click(screen.getByRole('button', { name: 'Cancel all' }));

		expect(onCancelAll).toHaveBeenCalledOnce();
	});

	it('allows active and queued files to be cancelled', async () => {
		const onCancel = vi.fn();
		render(UploadProgress, {
			props: {
				tasks: [
					{
						id: 'uploading-1',
						fileName: 'large.zip',
						size: 10,
						status: 'uploading',
						progress: 20
					},
					{
						id: 'pending-1',
						fileName: 'next.zip',
						size: 5,
						status: 'pending',
						progress: 0
					}
				],
				onCancel
			}
		});

		await fireEvent.click(screen.getByRole('button', { name: 'Cancel upload of large.zip' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Cancel upload of next.zip' }));

		expect(onCancel).toHaveBeenNthCalledWith(1, 'uploading-1');
		expect(onCancel).toHaveBeenNthCalledWith(2, 'pending-1');
	});
});
