import { describe, it, expect, beforeEach, vi } from 'vitest';
import { createShare, listFileShares, revokeShare, listAllUserShares } from '$lib/api/shares';
import type { CreateShareRequest } from '$lib/api/shares';

// Mock the API client
vi.mock('$lib/api/client', () => ({
  apiClient: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn()
  }
}));

import { apiClient } from '$lib/api/client';

describe('shares API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Mock window.location
    Object.defineProperty(window, 'location', {
      value: {
        origin: 'http://localhost:3000'
      },
      writable: true
    });
  });

  describe('createShare', () => {
    it('should create a share with password and expiry', async () => {
      const mockResponse = {
        id: 'share-1',
        share_token: 'abc123',
        permissions: 'View' as const,
        password_protected: true,
        expires_at: '2024-12-31T23:59:59Z'
      };

      vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

      const request: CreateShareRequest = {
        permissions: 'View',
        password: 'secret123',
        expires_at: '2024-12-31T23:59:59Z'
      };

      const result = await createShare('file-1', request);

      expect(apiClient.post).toHaveBeenCalledWith('/files/file-1/shares', request);
      expect(result.share_url).toBe('http://localhost:3000/share/abc123');
      expect(result.share_token).toBe('abc123');
      expect(result.password_protected).toBe(true);
    });

    it('should create a share without password', async () => {
      const mockResponse = {
        id: 'share-2',
        share_token: 'xyz789',
        permissions: 'ReadWrite' as const,
        password_protected: false,
        expires_at: null
      };

      vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

      const request: CreateShareRequest = {
        permissions: 'ReadWrite'
      };

      const result = await createShare('file-2', request);

      expect(result.password_protected).toBe(false);
      expect(result.expires_at).toBeNull();
    });

    it('should generate correct share URL', async () => {
      const mockResponse = {
        id: 'share-3',
        share_token: 'token123',
        permissions: 'View' as const,
        password_protected: false,
        expires_at: null
      };

      vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

      const result = await createShare('file-3', { permissions: 'View' });

      expect(result.share_url).toBe('http://localhost:3000/share/token123');
    });
  });

  describe('listFileShares', () => {
    it('should fetch all shares for a file', async () => {
      const mockShares = [
        {
          id: 'share-1',
          file_id: 'file-1',
          share_token: 'abc123',
          permissions: 'View' as const,
          password_protected: true,
          expires_at: '2024-12-31T23:59:59Z',
          created_at: '2024-01-01T00:00:00Z',
          created_by: 'user-1'
        },
        {
          id: 'share-2',
          file_id: 'file-1',
          share_token: 'xyz789',
          permissions: 'ReadWrite' as const,
          password_protected: false,
          expires_at: null,
          created_at: '2024-01-02T00:00:00Z',
          created_by: 'user-1'
        }
      ];

      vi.mocked(apiClient.get).mockResolvedValue(mockShares);

      const result = await listFileShares('file-1');

      expect(apiClient.get).toHaveBeenCalledWith('/files/file-1/shares');
      expect(result).toHaveLength(2);
      expect(result[0].share_token).toBe('abc123');
      expect(result[1].share_token).toBe('xyz789');
    });

    it('should return empty array when no shares exist', async () => {
      vi.mocked(apiClient.get).mockResolvedValue([]);

      const result = await listFileShares('file-2');

      expect(result).toHaveLength(0);
    });
  });

  describe('revokeShare', () => {
    it('should delete a share', async () => {
      vi.mocked(apiClient.delete).mockResolvedValue(undefined);

      await revokeShare('share-1');

      expect(apiClient.delete).toHaveBeenCalledWith('/shares/share-1');
    });

    it('should handle deletion errors', async () => {
      vi.mocked(apiClient.delete).mockRejectedValue(new Error('Not found'));

      await expect(revokeShare('invalid-share')).rejects.toThrow('Not found');
    });
  });

  describe('listAllUserShares', () => {
    it('should return empty array (placeholder)', async () => {
      const result = await listAllUserShares();

      expect(result).toEqual([]);
    });

    // TODO: Update when backend implements endpoint
    it.todo('should fetch all user shares when backend endpoint is available');
  });

  describe('error handling', () => {
    it('should propagate API errors', async () => {
      vi.mocked(apiClient.post).mockRejectedValue(new Error('Network error'));

      await expect(
        createShare('file-1', { permissions: 'View' })
      ).rejects.toThrow('Network error');
    });

    it('should handle validation errors', async () => {
      vi.mocked(apiClient.post).mockRejectedValue(
        new Error('Invalid expiry date')
      );

      await expect(
        createShare('file-1', {
          permissions: 'View',
          expires_at: 'invalid-date'
        })
      ).rejects.toThrow('Invalid expiry date');
    });
  });

  describe('share permissions', () => {
    it('should support View permission', async () => {
      const mockResponse = {
        id: 'share-1',
        share_token: 'abc123',
        permissions: 'View' as const,
        password_protected: false,
        expires_at: null
      };

      vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

      const result = await createShare('file-1', { permissions: 'View' });

      expect(result.permissions).toBe('View');
    });

    it('should support ReadWrite permission', async () => {
      const mockResponse = {
        id: 'share-2',
        share_token: 'xyz789',
        permissions: 'ReadWrite' as const,
        password_protected: false,
        expires_at: null
      };

      vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

      const result = await createShare('file-2', { permissions: 'ReadWrite' });

      expect(result.permissions).toBe('ReadWrite');
    });
  });
});
