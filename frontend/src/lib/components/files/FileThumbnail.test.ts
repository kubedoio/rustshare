import { render, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import FileThumbnail from './FileThumbnail.svelte';
import type { File } from '$lib/api/types';

describe('FileThumbnail', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.setItem('token', 'mock-token');
  });

  const createMockFile = (overrides?: Partial<File>): File => ({
    id: 'test-file-id',
    name: 'test-image.jpg',
    path: '/test-image.jpg',
    size: 1024000,
    mime_type: 'image/jpeg',
    content_hash: 'abc123',
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
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const spinner = container.querySelector('.loading-spinner');
      expect(spinner).toBeTruthy();
    });

    it('should fetch download URL for image files', async () => {
      const file = createMockFile({ mime_type: 'image/png' });
      const mockFetch = vi.mocked(fetch);

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ url: 'http://example.com/image.png' })
      } as Response);

      render(FileThumbnail, { props: { file, size: 'md' } });

      await waitFor(() => {
        expect(mockFetch).toHaveBeenCalledWith(
          '/api/files/test-file-id/download',
          expect.objectContaining({
            headers: expect.objectContaining({
              'Authorization': 'Bearer mock-token'
            })
          })
        );
      });
    });

    it('should generate thumbnail and display image', async () => {
      const file = createMockFile({ mime_type: 'image/jpeg' });
      const mockFetch = vi.mocked(fetch);

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ url: 'http://example.com/image.jpg' })
      } as Response);

      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      await waitFor(
        () => {
          const img = container.querySelector('img');
          expect(img).toBeTruthy();
          expect(img?.src).toContain('data:image/jpeg');
        },
        { timeout: 2000 }
      );
    });

    it('should apply correct size class for medium size', () => {
      const file = createMockFile({ mime_type: 'image/jpeg' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const wrapper = container.querySelector('.w-16.h-16');
      expect(wrapper).toBeTruthy();
    });

    it('should apply correct size class for small size', () => {
      const file = createMockFile({ mime_type: 'image/jpeg' });
      const { container } = render(FileThumbnail, { props: { file, size: 'sm' } });

      const wrapper = container.querySelector('.w-10.h-10');
      expect(wrapper).toBeTruthy();
    });

    it('should apply correct size class for large size', () => {
      const file = createMockFile({ mime_type: 'image/jpeg' });
      const { container } = render(FileThumbnail, { props: { file, size: 'lg' } });

      const wrapper = container.querySelector('.w-24.h-24');
      expect(wrapper).toBeTruthy();
    });

    it('should handle thumbnail generation failure gracefully', async () => {
      const file = createMockFile({ mime_type: 'image/jpeg' });
      const mockFetch = vi.mocked(fetch);

      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 500
      } as Response);

      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      await waitFor(() => {
        const icon = container.querySelector('span');
        expect(icon?.textContent).toBe('🖼️');
      });
    });
  });

  describe('Non-Image File Icons', () => {
    it('should show PDF icon for PDF files', () => {
      const file = createMockFile({ mime_type: 'application/pdf', name: 'document.pdf' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const icon = container.querySelector('span');
      expect(icon?.textContent).toBe('📄');
    });

    it('should show video icon for video files', () => {
      const file = createMockFile({ mime_type: 'video/mp4', name: 'video.mp4' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const icon = container.querySelector('span');
      expect(icon?.textContent).toBe('🎬');
    });

    it('should show text icon for text files', () => {
      const file = createMockFile({ mime_type: 'text/plain', name: 'document.txt' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const icon = container.querySelector('span');
      expect(icon?.textContent).toBe('📝');
    });

    it('should show audio icon for audio files', () => {
      const file = createMockFile({ mime_type: 'audio/mpeg', name: 'song.mp3' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const icon = container.querySelector('span');
      expect(icon?.textContent).toBe('🎵');
    });

    it('should show archive icon for zip files', () => {
      const file = createMockFile({ mime_type: 'application/zip', name: 'archive.zip' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const icon = container.querySelector('span');
      expect(icon?.textContent).toBe('📦');
    });

    it('should show word icon for Word documents', () => {
      const file = createMockFile({
        mime_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
        name: 'document.docx'
      });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const icon = container.querySelector('span');
      expect(icon?.textContent).toBe('📘');
    });

    it('should show spreadsheet icon for Excel files', () => {
      const file = createMockFile({
        mime_type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
        name: 'spreadsheet.xlsx'
      });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const icon = container.querySelector('span');
      expect(icon?.textContent).toBe('📊');
    });

    it('should show presentation icon for PowerPoint files', () => {
      const file = createMockFile({
        mime_type: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
        name: 'presentation.pptx'
      });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const icon = container.querySelector('span');
      expect(icon?.textContent).toBe('📽️');
    });

    it('should show generic document icon for unknown types', () => {
      const file = createMockFile({ mime_type: 'application/octet-stream', name: 'file.bin' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const icon = container.querySelector('span');
      expect(icon?.textContent).toBe('📄');
    });

    it('should not attempt thumbnail generation for non-image files', () => {
      const file = createMockFile({ mime_type: 'application/pdf' });
      const mockFetch = vi.mocked(fetch);

      render(FileThumbnail, { props: { file, size: 'md' } });

      // Should not call fetch for download URL
      expect(mockFetch).not.toHaveBeenCalled();
    });
  });

  describe('Image Type Detection', () => {
    it('should detect JPEG images', () => {
      const file = createMockFile({ mime_type: 'image/jpeg' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      // Should show loading spinner (indicating thumbnail generation)
      const spinner = container.querySelector('.loading-spinner');
      expect(spinner).toBeTruthy();
    });

    it('should detect PNG images', () => {
      const file = createMockFile({ mime_type: 'image/png' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const spinner = container.querySelector('.loading-spinner');
      expect(spinner).toBeTruthy();
    });

    it('should detect GIF images', () => {
      const file = createMockFile({ mime_type: 'image/gif' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const spinner = container.querySelector('.loading-spinner');
      expect(spinner).toBeTruthy();
    });

    it('should detect SVG images', () => {
      const file = createMockFile({ mime_type: 'image/svg+xml' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const spinner = container.querySelector('.loading-spinner');
      expect(spinner).toBeTruthy();
    });

    it('should detect WebP images', () => {
      const file = createMockFile({ mime_type: 'image/webp' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const spinner = container.querySelector('.loading-spinner');
      expect(spinner).toBeTruthy();
    });
  });

  describe('Component Rendering', () => {
    it('should render wrapper div with correct classes', () => {
      const file = createMockFile({ mime_type: 'application/pdf' });
      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      const wrapper = container.querySelector('div');
      expect(wrapper).toBeTruthy();
      expect(wrapper?.classList.contains('flex')).toBe(true);
      expect(wrapper?.classList.contains('items-center')).toBe(true);
      expect(wrapper?.classList.contains('justify-center')).toBe(true);
      expect(wrapper?.classList.contains('bg-base-200')).toBe(true);
      expect(wrapper?.classList.contains('rounded')).toBe(true);
    });

    it('should have alt text on thumbnail images', async () => {
      const file = createMockFile({
        mime_type: 'image/jpeg',
        name: 'vacation-photo.jpg'
      });
      const mockFetch = vi.mocked(fetch);

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ url: 'http://example.com/image.jpg' })
      } as Response);

      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

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
      const mockFetch = vi.mocked(fetch);

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ url: 'http://example.com/image.jpg' })
      } as Response);

      const { container } = render(FileThumbnail, { props: { file, size: 'md' } });

      await waitFor(
        () => {
          const img = container.querySelector('img');
          expect(img?.classList.contains('object-cover')).toBe(true);
        },
        { timeout: 2000 }
      );
    });
  });
});
