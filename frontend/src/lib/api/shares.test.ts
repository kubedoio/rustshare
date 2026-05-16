import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
	createFileUserShare,
	createFolderUserShare,
	createShare,
	getShareAccessLog,
	listAllUserShares,
	listFolderShares,
	listFolderRecipients,
	listFileRecipients,
	listFileShares,
	listReceivedShares,
	removeShareRecipient,
	revokeShare,
	updateSharePermission
} from '$lib/api/shares';
import type { CreateShareRequest } from '$lib/api/shares';

// Mock the API client
vi.mock('$lib/api/client', () => ({
	apiClient: {
			postVoid: vi.fn(),
			patchVoid: vi.fn(),
			requestText: vi.fn(),
			requestVoid: vi.fn(),
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
				resource_id: 'file-1',
				resource_type: 'file' as const,
				share_token: 'abc123',
				permissions: 'View' as const,
				upload_only: false,
				password_protected: true,
				expires_at: '2024-12-31T23:59:59Z'
			};

			vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

			const request: CreateShareRequest = {
				permissions: 'View',
				password: 'secret123',
				expires_at: '2024-12-31T23:59:59Z'
			};

			const result = await createShare('file', 'file-1', request);

			expect(apiClient.post).toHaveBeenCalledWith('/files/file-1/shares', request);
			expect(result.share_url).toBe('http://localhost:3000/share/abc123');
			expect(result.share_token).toBe('abc123');
			expect(result.password_protected).toBe(true);
		});

		it('should create a share without password', async () => {
			const mockResponse = {
				id: 'share-2',
				resource_id: 'file-2',
				resource_type: 'file' as const,
				share_token: 'xyz789',
				permissions: 'Edit' as const,
				upload_only: false,
				password_protected: false,
				expires_at: null
			};

			vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

			const request: CreateShareRequest = {
				permissions: 'Edit'
			};

			const result = await createShare('file', 'file-2', request);

			expect(result.password_protected).toBe(false);
			expect(result.expires_at).toBeNull();
		});

		it('should generate correct share URL', async () => {
			const mockResponse = {
				id: 'share-3',
				resource_id: 'file-3',
				resource_type: 'file' as const,
				share_token: 'token123',
				permissions: 'View' as const,
				upload_only: false,
				password_protected: false,
				expires_at: null
			};

			vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

			const result = await createShare('file', 'file-3', { permissions: 'View' });

			expect(result.share_url).toBe('http://localhost:3000/share/token123');
		});

		it('should create a folder share', async () => {
			const mockResponse = {
				id: 'share-folder-1',
				resource_id: 'folder-1',
				resource_type: 'folder' as const,
				share_token: 'folder123',
				permissions: 'View' as const,
				upload_only: false,
				password_protected: false,
				expires_at: null
			};

			vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

			await createShare('folder', 'folder-1', { permissions: 'View' });

			expect(apiClient.post).toHaveBeenCalledWith('/folders/folder-1/shares', {
				permissions: 'View'
			});
		});
	});

	describe('listFileShares', () => {
		it('should fetch all shares for a file', async () => {
			const mockShares = [
				{
					id: 'share-1',
					resource_id: 'file-1',
					resource_type: 'file' as const,
					share_token: 'abc123',
					permissions: 'View' as const,
					upload_only: false,
					password_protected: true,
					access_count: 0,
					expires_at: '2024-12-31T23:59:59Z',
					created_at: '2024-01-01T00:00:00Z',
					created_by: 'user-1'
				},
				{
					id: 'share-2',
					resource_id: 'file-1',
					resource_type: 'file' as const,
					share_token: 'xyz789',
					permissions: 'Edit' as const,
					upload_only: false,
					password_protected: false,
					access_count: 0,
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

	describe('listFolderShares', () => {
		it('should fetch all shares for a folder', async () => {
			const mockShares = [
				{
					id: 'share-folder-1',
					resource_id: 'folder-1',
					resource_type: 'folder' as const,
					share_token: 'folder123',
					permissions: 'View' as const,
					upload_only: true,
					password_protected: false,
					access_count: 0,
					expires_at: null,
					created_at: '2024-01-02T00:00:00Z'
				}
			];

			vi.mocked(apiClient.get).mockResolvedValue(mockShares);

			const result = await listFolderShares('folder-1');

			expect(apiClient.get).toHaveBeenCalledWith('/folders/folder-1/shares');
			expect(result).toEqual(mockShares);
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
		it('should fetch all shares for the current user', async () => {
			const mockShares = [
				{
					id: 'share-1',
					resource_id: 'file-1',
					resource_type: 'file' as const,
					resource_name: 'Quarterly Plan.pdf',
					share_token: 'abc123',
					permissions: 'View' as const,
					upload_only: false,
					password_protected: false,
					access_count: 3,
					expires_at: null,
					created_at: '2024-01-01T00:00:00Z',
					created_by: 'user-1'
				}
			];
			vi.mocked(apiClient.get).mockResolvedValue(mockShares);

			const result = await listAllUserShares();

			expect(apiClient.get).toHaveBeenCalledWith('/shares');
			expect(result).toEqual(mockShares);
		});
	});

	describe('internal shares', () => {
		it('should create a file user share', async () => {
			vi.mocked(apiClient.post).mockResolvedValue(undefined);

			await createFileUserShare('file-1', {
				recipient_email: 'teammate@example.com',
				permission: 'Edit'
			});

			expect(apiClient.postVoid).toHaveBeenCalledWith('/files/file-1/share', {
				recipient_email: 'teammate@example.com',
				permission: 'Edit'
			});
		});

		it('should list received shares', async () => {
			const mockShares = [
				{
					share_id: 'share-1',
					resource_id: 'file-1',
					resource_type: 'file' as const,
					resource_name: 'Roadmap.pdf',
					resource_path: '/Roadmap.pdf',
					permission: 'View' as const,
					shared_by: 'user-1',
					shared_by_name: 'Alice',
					shared_by_email: 'alice@example.com',
					created_at: '2024-01-01T00:00:00Z'
				}
			];
			vi.mocked(apiClient.get).mockResolvedValue(mockShares);

			const result = await listReceivedShares();

			expect(apiClient.get).toHaveBeenCalledWith('/shares/received');
			expect(result).toEqual(mockShares);
		});

		it('should list file recipients', async () => {
			const mockRecipients = [
				{
					share_id: 'share-1',
					user_id: 'user-2',
					email: 'teammate@example.com',
					permission: 'Edit' as const,
					added_at: '2024-01-01T00:00:00Z',
					added_by: 'user-1'
				}
			];
			vi.mocked(apiClient.get).mockResolvedValue(mockRecipients);

			const result = await listFileRecipients('file-1');

			expect(apiClient.get).toHaveBeenCalledWith('/files/file-1/recipients');
			expect(result).toEqual(mockRecipients);
		});

		it('should create a folder user share', async () => {
			vi.mocked(apiClient.post).mockResolvedValue(undefined);

			await createFolderUserShare('folder-1', {
				recipient_email: 'teammate@example.com',
				permission: 'View'
			});

			expect(apiClient.postVoid).toHaveBeenCalledWith('/folders/folder-1/share', {
				recipient_email: 'teammate@example.com',
				permission: 'View'
			});
		});

		it('should list folder recipients', async () => {
			const mockRecipients = [
				{
					share_id: 'share-2',
					user_id: 'user-2',
					email: 'teammate@example.com',
					permission: 'View' as const,
					added_at: '2024-01-01T00:00:00Z',
					added_by: 'user-1'
				}
			];
			vi.mocked(apiClient.get).mockResolvedValue(mockRecipients);

			const result = await listFolderRecipients('folder-1');

			expect(apiClient.get).toHaveBeenCalledWith('/folders/folder-1/recipients');
			expect(result).toEqual(mockRecipients);
		});

		it('should update recipient permission', async () => {
			vi.mocked(apiClient.put).mockResolvedValue(undefined);

			await updateSharePermission('share-1', { permission: 'Admin' });

			expect(apiClient.put).toHaveBeenCalledWith('/shares/share-1/permission', {
				permission: 'Admin'
			});
		});

		it('should remove a share recipient', async () => {
			vi.mocked(apiClient.delete).mockResolvedValue(undefined);

			await removeShareRecipient('share-1');

			expect(apiClient.delete).toHaveBeenCalledWith('/shares/share-1/recipient');
		});
	});

	describe('share activity', () => {
		it('should fetch the access log for a share', async () => {
			const mockEntries = [
				{
					accessed_at: '2026-03-21T10:00:00Z',
					action: 'download',
					success: true,
					actor_type: 'public_share_session',
					actor_label: 'Uploader',
					ip_address: '127.0.0.1',
					user_agent: 'Mozilla/5.0',
					share_session_id: 'session-1',
					share_session_subject: 'share:session-1'
				}
			];

			vi.mocked(apiClient.get).mockResolvedValue(mockEntries);

			const result = await getShareAccessLog('share-1', 25);

			expect(apiClient.get).toHaveBeenCalledWith('/shares/share-1/access-log?limit=25');
			expect(result).toEqual(mockEntries);
		});
	});

	describe('error handling', () => {
		it('should propagate API errors', async () => {
			vi.mocked(apiClient.post).mockRejectedValue(new Error('Network error'));

			await expect(createShare('file', 'file-1', { permissions: 'View' })).rejects.toThrow(
				'Network error'
			);
		});

		it('should handle validation errors', async () => {
			vi.mocked(apiClient.post).mockRejectedValue(new Error('Invalid expiry date'));

			await expect(
				createShare('file', 'file-1', {
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
				resource_id: 'file-1',
				resource_type: 'file' as const,
				share_token: 'abc123',
				permissions: 'View' as const,
				upload_only: false,
				password_protected: false,
				expires_at: null
			};

			vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

			const result = await createShare('file', 'file-1', { permissions: 'View' });

			expect(result.permissions).toBe('View');
		});

		it('should support Edit permission', async () => {
			const mockResponse = {
				id: 'share-2',
				resource_id: 'file-2',
				resource_type: 'file' as const,
				share_token: 'xyz789',
				permissions: 'Edit' as const,
				upload_only: false,
				password_protected: false,
				expires_at: null
			};

			vi.mocked(apiClient.post).mockResolvedValue(mockResponse);

			const result = await createShare('file', 'file-2', { permissions: 'Edit' });

			expect(result.permissions).toBe('Edit');
		});
	});
});
