import { describe, it, expect } from 'vitest';
import type { File, Folder } from '$lib/api/types';

// Test data
const mockFiles: File[] = [
  {
    id: 'file-1',
    name: 'zebra.pdf',
    path: '/zebra.pdf',
    content_hash: 'abc123',
    size: 5000,
    mime_type: 'application/pdf',
    parent_folder_id: null,
    owner_id: 'user-1',
    current_version: 1,
    created_at: '2024-01-01T00:00:00Z',
    modified_at: '2024-01-03T00:00:00Z'
  },
  {
    id: 'file-2',
    name: 'apple.txt',
    path: '/apple.txt',
    content_hash: 'def456',
    size: 1000,
    mime_type: 'text/plain',
    parent_folder_id: null,
    owner_id: 'user-1',
    current_version: 1,
    created_at: '2024-01-02T00:00:00Z',
    modified_at: '2024-01-01T00:00:00Z'
  },
  {
    id: 'file-3',
    name: 'mango.png',
    path: '/mango.png',
    content_hash: 'ghi789',
    size: 3000,
    mime_type: 'image/png',
    parent_folder_id: null,
    owner_id: 'user-1',
    current_version: 1,
    created_at: '2024-01-03T00:00:00Z',
    modified_at: '2024-01-02T00:00:00Z'
  }
];

const mockFolders: Folder[] = [
  {
    id: 'folder-1',
    name: 'Zoo',
    path: '/Zoo',
    parent_folder_id: null,
    owner_id: 'user-1',
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-03T00:00:00Z'
  },
  {
    id: 'folder-2',
    name: 'Archive',
    path: '/Archive',
    parent_folder_id: null,
    owner_id: 'user-1',
    created_at: '2024-01-02T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z'
  },
  {
    id: 'folder-3',
    name: 'Music',
    path: '/Music',
    parent_folder_id: null,
    owner_id: 'user-1',
    created_at: '2024-01-03T00:00:00Z',
    updated_at: '2024-01-02T00:00:00Z'
  }
];

describe('File and Folder Sorting', () => {
  describe('Sort by name', () => {
    it('should sort files by name ascending', () => {
      const sorted = [...mockFiles].sort((a, b) => a.name.localeCompare(b.name));

      expect(sorted[0].name).toBe('apple.txt');
      expect(sorted[1].name).toBe('mango.png');
      expect(sorted[2].name).toBe('zebra.pdf');
    });

    it('should sort files by name descending', () => {
      const sorted = [...mockFiles].sort((a, b) => b.name.localeCompare(a.name));

      expect(sorted[0].name).toBe('zebra.pdf');
      expect(sorted[1].name).toBe('mango.png');
      expect(sorted[2].name).toBe('apple.txt');
    });

    it('should sort folders by name ascending', () => {
      const sorted = [...mockFolders].sort((a, b) => a.name.localeCompare(b.name));

      expect(sorted[0].name).toBe('Archive');
      expect(sorted[1].name).toBe('Music');
      expect(sorted[2].name).toBe('Zoo');
    });

    it('should sort folders by name descending', () => {
      const sorted = [...mockFolders].sort((a, b) => b.name.localeCompare(a.name));

      expect(sorted[0].name).toBe('Zoo');
      expect(sorted[1].name).toBe('Music');
      expect(sorted[2].name).toBe('Archive');
    });

    it('should be case-insensitive', () => {
      const filesWithCase = [
        { ...mockFiles[0], name: 'Zebra.pdf' },
        { ...mockFiles[1], name: 'apple.txt' },
        { ...mockFiles[2], name: 'MANGO.png' }
      ];

      const sorted = [...filesWithCase].sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
      );

      expect(sorted[0].name).toBe('apple.txt');
      expect(sorted[1].name).toBe('MANGO.png');
      expect(sorted[2].name).toBe('Zebra.pdf');
    });
  });

  describe('Sort by modified date', () => {
    it('should sort files by date ascending (oldest first)', () => {
      const sorted = [...mockFiles].sort((a, b) => {
        const aTime = new Date(a.modified_at).getTime();
        const bTime = new Date(b.modified_at).getTime();
        return aTime - bTime;
      });

      expect(sorted[0].name).toBe('apple.txt'); // 2024-01-01
      expect(sorted[1].name).toBe('mango.png'); // 2024-01-02
      expect(sorted[2].name).toBe('zebra.pdf'); // 2024-01-03
    });

    it('should sort files by date descending (newest first)', () => {
      const sorted = [...mockFiles].sort((a, b) => {
        const aTime = new Date(a.modified_at).getTime();
        const bTime = new Date(b.modified_at).getTime();
        return bTime - aTime;
      });

      expect(sorted[0].name).toBe('zebra.pdf'); // 2024-01-03
      expect(sorted[1].name).toBe('mango.png'); // 2024-01-02
      expect(sorted[2].name).toBe('apple.txt'); // 2024-01-01
    });

    it('should sort folders by updated date', () => {
      const sorted = [...mockFolders].sort((a, b) => {
        const aTime = new Date(a.updated_at).getTime();
        const bTime = new Date(b.updated_at).getTime();
        return aTime - bTime;
      });

      expect(sorted[0].name).toBe('Archive'); // 2024-01-01
      expect(sorted[1].name).toBe('Music'); // 2024-01-02
      expect(sorted[2].name).toBe('Zoo'); // 2024-01-03
    });
  });

  describe('Sort by size', () => {
    it('should sort files by size ascending', () => {
      const sorted = [...mockFiles].sort((a, b) => a.size - b.size);

      expect(sorted[0].size).toBe(1000); // apple.txt
      expect(sorted[1].size).toBe(3000); // mango.png
      expect(sorted[2].size).toBe(5000); // zebra.pdf
    });

    it('should sort files by size descending', () => {
      const sorted = [...mockFiles].sort((a, b) => b.size - a.size);

      expect(sorted[0].size).toBe(5000); // zebra.pdf
      expect(sorted[1].size).toBe(3000); // mango.png
      expect(sorted[2].size).toBe(1000); // apple.txt
    });

    it('should handle zero-size files', () => {
      const filesWithEmpty = [
        ...mockFiles,
        {
          ...mockFiles[0],
          id: 'file-4',
          name: 'empty.txt',
          size: 0
        }
      ];

      const sorted = [...filesWithEmpty].sort((a, b) => a.size - b.size);

      expect(sorted[0].size).toBe(0);
    });
  });

  describe('Sort by MIME type', () => {
    it('should sort files by MIME type ascending', () => {
      const sorted = [...mockFiles].sort((a, b) => a.mime_type.localeCompare(b.mime_type));

      expect(sorted[0].mime_type).toBe('application/pdf');
      expect(sorted[1].mime_type).toBe('image/png');
      expect(sorted[2].mime_type).toBe('text/plain');
    });

    it('should sort files by MIME type descending', () => {
      const sorted = [...mockFiles].sort((a, b) => b.mime_type.localeCompare(a.mime_type));

      expect(sorted[0].mime_type).toBe('text/plain');
      expect(sorted[1].mime_type).toBe('image/png');
      expect(sorted[2].mime_type).toBe('application/pdf');
    });

    it('should group files by main type', () => {
      const sorted = [...mockFiles].sort((a, b) => {
        const aType = a.mime_type.split('/')[0];
        const bType = b.mime_type.split('/')[0];
        return aType.localeCompare(bType);
      });

      // application, image, text
      expect(sorted[0].mime_type).toContain('application/');
      expect(sorted[1].mime_type).toContain('image/');
      expect(sorted[2].mime_type).toContain('text/');
    });
  });

  describe('Mixed sorting scenarios', () => {
    it('should maintain folder-file separation when needed', () => {
      // In file browser, typically folders are shown first regardless of sort
      const allItems = [...mockFolders, ...mockFiles];

      // Sort folders and files separately, then combine
      const sortedFolders = [...mockFolders].sort((a, b) => a.name.localeCompare(b.name));
      const sortedFiles = [...mockFiles].sort((a, b) => a.name.localeCompare(b.name));
      const sorted = [...sortedFolders, ...sortedFiles];

      // First 3 should be folders
      expect(sorted[0].name).toBe('Archive');
      expect(sorted[1].name).toBe('Music');
      expect(sorted[2].name).toBe('Zoo');

      // Next 3 should be files
      expect(sorted[3].name).toBe('apple.txt');
      expect(sorted[4].name).toBe('mango.png');
      expect(sorted[5].name).toBe('zebra.pdf');
    });

    it('should handle empty arrays', () => {
      const sorted = [].sort();
      expect(sorted).toHaveLength(0);
    });

    it('should handle single item', () => {
      const singleFile = [mockFiles[0]];
      const sorted = [...singleFile].sort((a, b) => a.name.localeCompare(b.name));
      expect(sorted).toHaveLength(1);
      expect(sorted[0]).toEqual(mockFiles[0]);
    });
  });

  describe('Edge cases', () => {
    it('should handle items with same name', () => {
      const duplicates = [
        { ...mockFiles[0], id: 'file-1', name: 'test.txt' },
        { ...mockFiles[1], id: 'file-2', name: 'test.txt' }
      ];

      const sorted = [...duplicates].sort((a, b) => a.name.localeCompare(b.name));

      // Both should remain, order maintained
      expect(sorted).toHaveLength(2);
      expect(sorted[0].id).toBe('file-1');
      expect(sorted[1].id).toBe('file-2');
    });

    it('should handle items with same date', () => {
      const sameDates = [
        { ...mockFiles[0], modified_at: '2024-01-01T12:00:00Z', name: 'first.txt' },
        { ...mockFiles[1], modified_at: '2024-01-01T12:00:00Z', name: 'second.txt' }
      ];

      const sorted = [...sameDates].sort((a, b) => {
        const aTime = new Date(a.modified_at).getTime();
        const bTime = new Date(b.modified_at).getTime();
        return aTime - bTime;
      });

      // Order preserved when dates are equal
      expect(sorted).toHaveLength(2);
    });

    it('should handle special characters in names', () => {
      const specialNames = [
        { ...mockFiles[0], name: '!important.txt' },
        { ...mockFiles[1], name: '@readme.txt' },
        { ...mockFiles[2], name: 'normal.txt' }
      ];

      const sorted = [...specialNames].sort((a, b) => a.name.localeCompare(b.name));

      // Special characters sort before letters
      expect(sorted[0].name).toBe('!important.txt');
      expect(sorted[1].name).toBe('@readme.txt');
      expect(sorted[2].name).toBe('normal.txt');
    });
  });
});
