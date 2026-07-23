import { render, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { tick } from 'svelte';
import FileThumbnail from './FileThumbnail.svelte';
import type { File } from '$lib/api/types';

describe('FileThumbnail', () => {
	const mockFetch = vi.mocked(fetch);

	beforeEach(() => {
		vi.clearAllMocks();
		mockFetch.mockResolvedValue({
			ok: true,
			status: 200,
			blob: async () => new Blob(['mock'], { type: 'image/jpeg' })
		} as Response);
	});

	const mockThumbnailBlob = (body = 'mock', type = 'image/jpeg') =>
		mockFetch.mockResolvedValueOnce({
			ok: true,
			status: 200,
			blob: async () => new Blob([body], { type })
		} as Response);

	const createMockFile = (overrides?: Partial<File>): File => ({
		id: 'test-file-id',
		name: 'test-image.jpg',
		path: '/test-image.jpg',
		size: 1024000,
		mime_type: 'image/jpeg',
		storage_key: 'blobs/abc123',
		owner_id: 'user-id',
		parent_folder_id: null,
		current_version: 1,
		created_at: '2026-03-19T10:00:00Z',
		modified_at: '2026-03-19T10:00:00Z',
		...overrides
	});

	describe('Image File Thumbnails', () => {
		it('should show loading spinner initially for image files', () => {
			const file = createMockFile({ mime_type: 'image/jpeg' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			const spinner = container.querySelector('.loading-spinner');
			expect(spinner).toBeTruthy();
		});

		it('should fetch the thumbnail endpoint for image files', async () => {
			const file = createMockFile({ mime_type: 'image/png' });

			mockThumbnailBlob('mock', 'image/png');

			render(FileThumbnail, { props: { file, size: 'md' } });

			await waitFor(() => {
				expect(mockFetch).toHaveBeenCalledWith(
					'/api/v1/files/test-file-id/thumbnail?size=md',
					expect.objectContaining({
						credentials: 'include'
					})
				);
			});
		});

		it('should render a thumbnail image after loading', async () => {
			const file = createMockFile({ mime_type: 'image/jpeg' });

			mockThumbnailBlob();

			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			await waitFor(
				() => {
					const img = container.querySelector('img');
					expect(img).toBeTruthy();
					expect(img?.src).toMatch(/^blob:mock-object-url-\d+$/);
				},
				{ timeout: 2000 }
			);
		});

		it('should apply correct size class for medium size', () => {
			const file = createMockFile({ mime_type: 'image/jpeg' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			const wrapper = container.querySelector('.w-16.h-16');
			expect(wrapper).toBeTruthy();
		});

		it('should apply correct size class for small size', () => {
			const file = createMockFile({ mime_type: 'image/jpeg' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'sm' }
			});

			const wrapper = container.querySelector('.w-10.h-10');
			expect(wrapper).toBeTruthy();
		});

		it('should apply correct size class for large size', () => {
			const file = createMockFile({ mime_type: 'image/jpeg' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'lg' }
			});

			const wrapper = container.querySelector('.w-24.h-24');
			expect(wrapper).toBeTruthy();
		});

		it('should fall back to an icon when thumbnail loading fails', async () => {
			const file = createMockFile({ mime_type: 'image/jpeg' });

			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 500
			} as Response);

			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			await waitFor(() => {
				const icon = container.querySelector('svg');
				expect(icon).toBeTruthy();
			});
		});
	});

	describe('Non-Image File Icons', () => {
		it('should show PDF icon for PDF files', async () => {
			const file = createMockFile({
				mime_type: 'application/pdf',
				name: 'document.pdf'
			});
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 500
			} as Response);

			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			await waitFor(() => {
				const icon = container.querySelector('svg');
				expect(icon).toBeTruthy();
			});
		});

		it('should show video icon for video files', async () => {
			const file = createMockFile({
				mime_type: 'video/mp4',
				name: 'video.mp4'
			});
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 500
			} as Response);

			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			await waitFor(() => {
				const icon = container.querySelector('svg');
				expect(icon).toBeTruthy();
			});
		});

		it('should show text icon for text files', async () => {
			const file = createMockFile({
				mime_type: 'text/plain',
				name: 'document.txt'
			});
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});
			await tick();

			const icon = container.querySelector('svg');
			expect(icon).toBeTruthy();
		});

		it('should show audio icon for audio files', async () => {
			const file = createMockFile({
				mime_type: 'audio/mpeg',
				name: 'song.mp3'
			});
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});
			await tick();

			const icon = container.querySelector('svg');
			expect(icon).toBeTruthy();
		});

		it('should show archive icon for zip files', async () => {
			const file = createMockFile({
				mime_type: 'application/zip',
				name: 'archive.zip'
			});
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});
			await tick();

			const icon = container.querySelector('svg');
			expect(icon).toBeTruthy();
		});

		it('should show word icon for Word documents', async () => {
			const file = createMockFile({
				mime_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
				name: 'document.docx'
			});
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});
			await tick();

			const icon = container.querySelector('svg');
			expect(icon).toBeTruthy();
		});

		it('should show spreadsheet icon for Excel files', async () => {
			const file = createMockFile({
				mime_type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
				name: 'spreadsheet.xlsx'
			});
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});
			await tick();

			const icon = container.querySelector('svg');
			expect(icon).toBeTruthy();
		});

		it('should show presentation icon for PowerPoint files', async () => {
			const file = createMockFile({
				mime_type: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
				name: 'presentation.pptx'
			});
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});
			await tick();

			const icon = container.querySelector('svg');
			expect(icon).toBeTruthy();
		});

		it('should show generic document icon for unknown types', async () => {
			const file = createMockFile({
				mime_type: 'application/octet-stream',
				name: 'file.bin'
			});
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});
			await tick();

			const icon = container.querySelector('svg');
			expect(icon).toBeTruthy();
		});

		it('should not attempt thumbnail generation for non-image files', async () => {
			const file = createMockFile({ mime_type: 'application/octet-stream' });

			render(FileThumbnail, { props: { file, size: 'md' } });
			await tick();

			// Should not call fetch for thumbnail URL
			expect(mockFetch).not.toHaveBeenCalled();
		});
	});

	describe('Image Type Detection', () => {
		it('should detect JPEG images', () => {
			const file = createMockFile({ mime_type: 'image/jpeg' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			// Should show loading spinner (indicating thumbnail generation)
			const spinner = container.querySelector('.loading-spinner');
			expect(spinner).toBeTruthy();
		});

		it('should detect PNG images', () => {
			const file = createMockFile({ mime_type: 'image/png' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			const spinner = container.querySelector('.loading-spinner');
			expect(spinner).toBeTruthy();
		});

		it('should detect GIF images', () => {
			const file = createMockFile({ mime_type: 'image/gif' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			const spinner = container.querySelector('.loading-spinner');
			expect(spinner).toBeTruthy();
		});

		it('should detect SVG images', () => {
			const file = createMockFile({ mime_type: 'image/svg+xml' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			const spinner = container.querySelector('.loading-spinner');
			expect(spinner).toBeTruthy();
		});

		it('should detect WebP images', () => {
			const file = createMockFile({ mime_type: 'image/webp' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			const spinner = container.querySelector('.loading-spinner');
			expect(spinner).toBeTruthy();
		});
	});

	describe('Component Rendering', () => {
		it('should render wrapper div with correct classes', () => {
			const file = createMockFile({ mime_type: 'application/pdf' });
			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			const wrapper = container.querySelector('div');
			expect(wrapper).toBeTruthy();
			expect(wrapper?.classList.contains('flex')).toBe(true);
			expect(wrapper?.classList.contains('items-center')).toBe(true);
			expect(wrapper?.classList.contains('justify-center')).toBe(true);
			expect(wrapper?.classList.contains('bg-base-200')).toBe(true);
			expect(wrapper?.classList.contains('rounded')).toBe(true);
		});

		it('should use the file name as alt text for thumbnail images', async () => {
			const file = createMockFile({
				mime_type: 'image/jpeg',
				name: 'vacation-photo.jpg'
			});

			mockThumbnailBlob();

			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			await waitFor(
				() => {
					const img = container.querySelector('img');
					expect(img?.alt).toBe('vacation-photo.jpg');
				},
				{ timeout: 2000 }
			);
		});

		it('should apply object-cover class to thumbnail images', async () => {
			const file = createMockFile({ mime_type: 'image/jpeg' });

			mockThumbnailBlob();

			const { container } = render(FileThumbnail, {
				props: { file, size: 'md' }
			});

			await waitFor(
				() => {
					const img = container.querySelector('img');
					expect(img?.classList.contains('object-cover')).toBe(true);
				},
				{ timeout: 2000 }
			);
		});

		it('should not display a stale thumbnail when the file changes', async () => {
			const file1 = createMockFile({
				id: 'file-1',
				name: 'first.jpg',
				mime_type: 'image/jpeg',
				modified_at: '2026-03-19T10:00:00Z'
			});
			const file2 = createMockFile({
				id: 'file-2',
				name: 'second.jpg',
				mime_type: 'image/jpeg',
				modified_at: '2026-03-19T11:00:00Z'
			});

			mockThumbnailBlob('first');

			const { container, rerender } = render(FileThumbnail, {
				props: { file: file1, size: 'md' }
			});

			let firstSrc: string | null = null;
			await waitFor(() => {
				const img = container.querySelector('img');
				expect(img).toBeTruthy();
				firstSrc = img?.src ?? null;
			});

			mockThumbnailBlob('second');

			await rerender({ file: file2, size: 'md' });

			await waitFor(() => {
				const img = container.querySelector('img');
				expect(img?.src).toBeTruthy();
				expect(img?.src).not.toBe(firstSrc);
			});
		});

		it('should revoke the object URL when the component is destroyed', async () => {
			const file = createMockFile({ mime_type: 'image/jpeg' });

			mockThumbnailBlob();

			const { unmount } = render(FileThumbnail, { props: { file, size: 'md' } });

			await waitFor(() => {
				expect(URL.createObjectURL).toHaveBeenCalled();
			});

			const createdUrl = vi.mocked(URL.createObjectURL).mock.results[0].value as string;

			unmount();

			await waitFor(() => {
				expect(URL.revokeObjectURL).toHaveBeenCalledWith(createdUrl);
			});
		});
	});
});
