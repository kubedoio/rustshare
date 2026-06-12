import { apiClient } from './client';
import type {
	ReceivedShare,
	Share,
	ShareAccessLogEntry,
	SharedFolderContents,
	ShareRecipient
} from './types';

const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:8080/api/v1';

// Request/Response Types

export interface CreateShareRequest {
	permissions: 'View' | 'Edit' | 'Admin';
	password?: string;
	expires_at?: string; // ISO 8601
	upload_only?: boolean;
}

export interface CreateShareResponse {
	id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	share_token: string;
	permissions: 'View' | 'Edit' | 'Admin';
	upload_only: boolean;
	password_protected: boolean;
	expires_at: string | null;
	share_url: string;
}

export interface ShareInfo {
	resource_id: string;
	resource_type: 'file' | 'folder';
	name: string;
	permissions: 'View' | 'Edit' | 'Admin';
	upload_only: boolean;
	file_size: number | null;
	mime_type: string | null;
	password_protected: boolean;
	expires_at: string | null;
}

export interface ShareSessionRequest {
	password?: string;
}

export interface ShareSessionResponse {
	session_token: string;
	expires_at: string;
	permissions: 'View' | 'Edit' | 'Admin';
	upload_only: boolean;
}

export interface PublicShareUploadResponse {
	id: string;
	name: string;
	size: number;
	mime_type: string;
	current_version: number;
	created_at: string;
}

export interface PublicShareUploadOptions {
	parentFolderId?: string;
	uploaderName?: string;
	onProgress?: (progress: number) => void;
}

export interface CreateUserShareRequest {
	recipient_email: string;
	permission: 'View' | 'Edit' | 'Admin';
}

export interface UpdateSharePermissionRequest {
	permission: 'View' | 'Edit' | 'Admin';
}

// Authenticated Share Operations

/**
 * Create a public share link for a file
 * POST /api/files/{file_id}/shares
 */
export async function createShare(
	resourceType: 'file' | 'folder',
	resourceId: string,
	request: CreateShareRequest
): Promise<CreateShareResponse> {
	const endpoint =
		resourceType === 'folder' ? `/folders/${resourceId}/shares` : `/files/${resourceId}/shares`;
	const response = await apiClient.post<Omit<CreateShareResponse, 'share_url'>>(endpoint, request);

	const baseUrl = window.location.origin;
	const share_url = `${baseUrl}/share/${response.share_token}`;

	return {
		...response,
		share_url
	};
}

/**
 * List all share links for a file
 * GET /api/files/{file_id}/shares
 */
export async function listFileShares(fileId: string): Promise<Share[]> {
	return apiClient.get<Share[]>(`/files/${fileId}/shares`);
}

/**
 * List all share links for a folder
 * GET /api/folders/{folder_id}/shares
 */
export async function listFolderShares(folderId: string): Promise<Share[]> {
	return apiClient.get<Share[]>(`/folders/${folderId}/shares`);
}

const MAX_PAGE_SIZE = 100;

/**
 * List all shares owned by the current user.
 * Pages through the backend until every owned share is returned, preserving
 * the previous unpaginated behaviour for the existing shares UI.
 */
export async function listAllUserShares(): Promise<Share[]> {
	const shares: Share[] = [];
	let page = 1;

	while (true) {
		const batch = await apiClient.get<Share[]>(`/shares?page=${page}&per_page=${MAX_PAGE_SIZE}`);
		shares.push(...batch);

		if (batch.length < MAX_PAGE_SIZE) {
			return shares;
		}

		page += 1;
	}
}

/**
 * List shares received by the current user.
 * GET /api/shares/received
 */
export async function listReceivedShares(): Promise<ReceivedShare[]> {
	return apiClient.get<ReceivedShare[]>('/shares/received');
}

/**
 * Share a file with another user.
 * POST /api/files/{file_id}/share
 */
export async function createFileUserShare(
	fileId: string,
	request: CreateUserShareRequest
): Promise<void> {
	return apiClient.postVoid(`/files/${fileId}/share`, request);
}

/**
 * Share a folder with another user.
 * POST /api/folders/{folder_id}/share
 */
export async function createFolderUserShare(
	folderId: string,
	request: CreateUserShareRequest
): Promise<void> {
	return apiClient.postVoid(`/folders/${folderId}/share`, request);
}

/**
 * List recipients for a shared file.
 * GET /api/files/{file_id}/recipients
 */
export async function listFileRecipients(fileId: string): Promise<ShareRecipient[]> {
	return apiClient.get<ShareRecipient[]>(`/files/${fileId}/recipients`);
}

/**
 * List recipients for a shared folder.
 * GET /api/folders/{folder_id}/recipients
 */
export async function listFolderRecipients(folderId: string): Promise<ShareRecipient[]> {
	return apiClient.get<ShareRecipient[]>(`/folders/${folderId}/recipients`);
}

/**
 * Update an internal share permission.
 * PUT /api/shares/{share_id}/permission
 */
export async function updateSharePermission(
	shareId: string,
	request: UpdateSharePermissionRequest
): Promise<void> {
	return apiClient.put<void>(`/shares/${shareId}/permission`, request);
}

/**
 * Remove a recipient from an internal share.
 * DELETE /api/shares/{share_id}/recipient
 */
export async function removeShareRecipient(shareId: string): Promise<void> {
	return apiClient.delete(`/shares/${shareId}/recipient`);
}

/**
 * Revoke a share link
 * DELETE /api/shares/{id}
 * Note: This endpoint may not be implemented in the backend yet
 */
export async function revokeShare(shareId: string): Promise<void> {
	return apiClient.delete(`/shares/${shareId}`);
}

/**
 * Get public-share access-log entries for an owned share.
 * GET /api/shares/{id}/access-log?limit=50
 */
export async function getShareAccessLog(
	shareId: string,
	limit = 50
): Promise<ShareAccessLogEntry[]> {
	return apiClient.get<ShareAccessLogEntry[]>(`/shares/${shareId}/access-log?limit=${limit}`);
}

// Public Share Access (No Authentication)

/**
 * Get information about a public share
 * GET /api/public/share/{token}/info
 * This endpoint does not require authentication
 */
export async function getPublicShareInfo(token: string): Promise<ShareInfo> {
	const response = await fetch(`${API_BASE_URL}/public/share/${token}/info`);

	if (!response.ok) {
		let errorMessage = 'Failed to get share info';
		try {
			const errorData = await response.json();
			errorMessage = errorData.error || errorData.message || errorMessage;
		} catch {
			// If parsing fails, use status-based messages
			if (response.status === 404) {
				errorMessage = 'Share not found';
			} else if (response.status === 410) {
				errorMessage = 'Share has expired';
			} else {
				errorMessage = response.statusText || errorMessage;
			}
		}
		const error = new Error(errorMessage) as Error & { status?: number };
		error.status = response.status;
		throw error;
	}

	return response.json();
}

/**
 * Create a session for accessing a password-protected share
 * POST /api/public/share/{token}/session
 * This endpoint does not require authentication
 */
export async function createShareSession(
	token: string,
	request: ShareSessionRequest
): Promise<ShareSessionResponse> {
	const response = await fetch(`${API_BASE_URL}/public/share/${token}/session`, {
		method: 'POST',
		headers: {
			'Content-Type': 'application/json'
		},
		body: JSON.stringify(request)
	});

	if (!response.ok) {
		let errorMessage = 'Failed to create share session';
		try {
			const errorData = await response.json();
			errorMessage = errorData.error || errorData.message || errorMessage;
		} catch {
			errorMessage = response.statusText || errorMessage;
		}
		throw new Error(errorMessage);
	}

	return response.json();
}

/**
 * Download a file from a public share
 * GET /api/public/share/{token}/file
 * Requires a valid session token (from createShareSession)
 */
export async function downloadPublicShareFile(token: string, sessionToken: string): Promise<Blob> {
	const response = await fetch(`${API_BASE_URL}/public/share/${token}/file`, {
		headers: {
			Authorization: `Bearer ${sessionToken}`
		}
	});

	if (!response.ok) {
		let errorMessage = 'Failed to download file';
		try {
			const errorData = await response.json();
			errorMessage = errorData.error || errorData.message || errorMessage;
		} catch {
			errorMessage = response.statusText || errorMessage;
		}
		throw new Error(errorMessage);
	}

	return response.blob();
}

/**
 * Helper function to trigger file download in the browser
 */
export function triggerFileDownload(blob: Blob, fileName: string): void {
	const url = window.URL.createObjectURL(blob);
	const a = document.createElement('a');
	a.href = url;
	a.download = fileName;
	document.body.appendChild(a);
	a.click();
	window.URL.revokeObjectURL(url);
	document.body.removeChild(a);
}

export async function getPublicFolderContents(
	token: string,
	sessionToken: string,
	folderId?: string
): Promise<SharedFolderContents> {
	const url = new URL(`${API_BASE_URL}/public/share/${token}/folder/contents`);
	if (folderId) {
		url.searchParams.set('folder_id', folderId);
	}

	const response = await fetch(url, {
		headers: {
			Authorization: `Bearer ${sessionToken}`
		}
	});

	if (!response.ok) {
		let errorMessage = 'Failed to load shared folder';
		try {
			const errorData = await response.json();
			errorMessage = errorData.error || errorData.message || errorMessage;
		} catch {
			errorMessage = response.statusText || errorMessage;
		}
		throw new Error(errorMessage);
	}

	return response.json();
}

export async function downloadPublicFolderFile(
	token: string,
	fileId: string,
	sessionToken: string
): Promise<Blob> {
	const response = await fetch(`${API_BASE_URL}/public/share/${token}/folder/files/${fileId}`, {
		headers: {
			Authorization: `Bearer ${sessionToken}`
		}
	});

	if (!response.ok) {
		let errorMessage = 'Failed to download file';
		try {
			const errorData = await response.json();
			errorMessage = errorData.error || errorData.message || errorMessage;
		} catch {
			errorMessage = response.statusText || errorMessage;
		}
		throw new Error(errorMessage);
	}

	return response.blob();
}

export async function uploadToPublicFolder(
	token: string,
	sessionToken: string,
	file: globalThis.File,
	options: PublicShareUploadOptions = {}
): Promise<PublicShareUploadResponse> {
	const formData = new FormData();
	formData.append('file', file);
	formData.append('name', file.name);
	if (options.parentFolderId) {
		formData.append('parent_folder_id', options.parentFolderId);
	}
	if (options.uploaderName?.trim()) {
		formData.append('uploader_name', options.uploaderName.trim());
	}

	return new Promise((resolve, reject) => {
		const xhr = new XMLHttpRequest();
		xhr.open('POST', `${API_BASE_URL}/public/share/${token}/folder/upload`);
		xhr.setRequestHeader('Authorization', `Bearer ${sessionToken}`);
		xhr.responseType = 'json';

		xhr.upload.onprogress = (event) => {
			if (!event.lengthComputable) {
				return;
			}
			const progress = Math.round((event.loaded / event.total) * 100);
			options.onProgress?.(progress);
		};

		xhr.onerror = () => {
			reject(new Error('Network error during upload'));
		};

		xhr.onload = () => {
			if (xhr.status >= 200 && xhr.status < 300) {
				options.onProgress?.(100);
				resolve(xhr.response as PublicShareUploadResponse);
				return;
			}

			const response = xhr.response as { error?: string; message?: string } | null;
			reject(
				new Error(response?.error || response?.message || xhr.statusText || 'Failed to upload file')
			);
		};

		xhr.send(formData);
	});
}
