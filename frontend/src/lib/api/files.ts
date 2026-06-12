import { apiClient } from './client';
import type { File, FileVersion } from './types';
import type { FolderContents } from './folders';

const AGGREGATE_PAGE_SIZE = 100;

export async function listAllFiles(): Promise<File[]> {
	return fetchAllPages<File>('/files');
}

export async function getStarredContents(): Promise<FolderContents> {
	return fetchAllFolderContentsPages('/files/starred');
}

export async function getDeletedContents(): Promise<FolderContents> {
	return fetchAllFolderContentsPages('/files/deleted');
}

export type UploadProgressCallback = (progress: number) => void;

async function fetchAllPages<T>(endpoint: string): Promise<T[]> {
	const items: T[] = [];
	let page = 1;

	while (true) {
		const pageItems = await apiClient.get<T[]>(
			`${endpoint}?page=${page}&per_page=${AGGREGATE_PAGE_SIZE}`
		);
		items.push(...pageItems);

		if (pageItems.length < AGGREGATE_PAGE_SIZE) {
			return items;
		}

		page += 1;
	}
}

async function fetchAllFolderContentsPages(endpoint: string): Promise<FolderContents> {
	const contents: FolderContents = {
		folders: [],
		files: []
	};
	let page = 1;

	while (true) {
		const pageContents = await apiClient.get<FolderContents>(
			`${endpoint}?page=${page}&per_page=${AGGREGATE_PAGE_SIZE}`
		);
		contents.folders.push(...pageContents.folders);
		contents.files.push(...pageContents.files);

		if (
			pageContents.folders.length < AGGREGATE_PAGE_SIZE &&
			pageContents.files.length < AGGREGATE_PAGE_SIZE
		) {
			return contents;
		}

		page += 1;
	}
}

/**
 * Uploads a file to the server.
 * Uses resumable chunked upload for files larger than 5MB to bypass proxy limits
 * and improve reliability.
 */
export async function uploadFile(
	folderId: string | null,
	file: globalThis.File,
	onProgress?: UploadProgressCallback,
	name?: string
): Promise<File> {
	// Use chunked upload for files > 10MB to bypass most proxy/Cloudflare limits (usually 100MB, but let's be safe)
	// Actually, let's use it for > 5MB to ensure we test it properly.
	if (file.size > 5 * 1024 * 1024) {
		return uploadFileChunked(folderId, file, onProgress, name);
	}

	const formData = new FormData();
	formData.append('file', file);
	formData.append('name', name || file.name);

	// Only append parent_folder_id if it's not null
	if (folderId) {
		formData.append('parent_folder_id', folderId);
	}

	// standard upload
	if (onProgress) onProgress(10);
	const result = await apiClient.post<File>('/files/upload', formData);
	if (onProgress) onProgress(100);
	return result;
}

/**
 * Internal helper for resumable chunked uploads
 */
async function uploadFileChunked(
	folderId: string | null,
	file: globalThis.File,
	onProgress?: UploadProgressCallback,
	name?: string
): Promise<File> {
	const CHUNK_SIZE = 5 * 1024 * 1024; // 5MB chunks
	const totalChunks = Math.ceil(file.size / CHUNK_SIZE);

	// 1. Create upload session
	const session = await apiClient.post<{
		session_id: string;
		total_chunks: number;
		chunk_size: number;
	}>('/uploads/sessions', {
		folder_id: folderId,
		file_name: name || file.name,
		mime_type: file.type || 'application/octet-stream',
		total_size: file.size,
		chunk_size: CHUNK_SIZE
	});

	const sessionId = session.session_id;

	// 2. Upload chunks
	for (let i = 0; i < totalChunks; i++) {
		const start = i * CHUNK_SIZE;
		const end = Math.min(start + CHUNK_SIZE, file.size);
		const chunk = file.slice(start, end);

		await apiClient.requestVoid(`/uploads/sessions/${sessionId}/chunks/${i}`, {
			method: 'PUT',
			body: chunk
		});

		if (onProgress) {
			const progress = Math.round(((i + 1) / totalChunks) * 90); // 0-90% for chunks, last 10% for complete
			onProgress(progress);
		}
	}

	// 3. Complete upload
	if (onProgress) onProgress(95);
	const result = await apiClient.post<{ file_id: string; file_name: string }>(
		`/uploads/sessions/${sessionId}/complete`,
		{}
	);

	// 4. Return the created file (we need to fetch it to match the expected return type)
	if (onProgress) onProgress(100);
	return getFile(result.file_id);
}

export async function getFile(fileId: string): Promise<File> {
	return apiClient.get<File>(`/files/${fileId}`);
}

export async function downloadFile(fileId: string): Promise<{ url: string }> {
	// Use the new /content endpoint which returns the file with proper Content-Disposition header
	// This ensures downloaded files have their original filename instead of the storage ID
	return { url: `/api/v1/files/${fileId}/content` };
}

export async function previewFile(fileId: string): Promise<{ url: string }> {
	// Use the /preview endpoint which returns the file with inline disposition
	// This allows the browser to display the file (images, PDFs, videos) instead of downloading
	return { url: `/api/v1/files/${fileId}/preview` };
}

export async function renameFile(fileId: string, newName: string): Promise<void> {
	return apiClient.postVoid(`/files/${fileId}/rename`, { new_name: newName });
}

export async function moveFile(fileId: string, targetFolderId: string | null): Promise<void> {
	return apiClient.postVoid(`/files/${fileId}/move`, {
		target_folder_id: targetFolderId
	});
}

export async function deleteFile(fileId: string): Promise<void> {
	return apiClient.delete(`/files/${fileId}`);
}

export async function permanentlyDeleteFile(fileId: string): Promise<void> {
	return apiClient.delete(`/files/${fileId}/permanent`);
}

export async function restoreFileFromTrash(fileId: string): Promise<void> {
	return apiClient.postVoid(`/files/${fileId}/restore-from-trash`, null);
}

export async function setFileStarred(fileId: string, starred: boolean): Promise<void> {
	return apiClient.patchVoid(`/files/${fileId}/star`, { starred });
}

export async function updateFile(
	fileId: string,
	file: globalThis.File,
	currentVersion: number
): Promise<File> {
	const formData = new FormData();
	formData.append('file', file);
	const headers: Record<string, string> = {
		'If-Match': currentVersion.toString()
	};

	return apiClient.put<File>(`/files/${fileId}`, formData);
}

export async function getFileVersions(fileId: string): Promise<FileVersion[]> {
	return apiClient.get<FileVersion[]>(`/files/${fileId}/versions`);
}

export async function restoreFileVersion(
	fileId: string,
	versionNumber: number,
	currentVersion: number
): Promise<File> {
	const headers: Record<string, string> = {
		'If-Match': currentVersion.toString(),
		'Content-Type': 'application/json'
	};

	const response = await apiClient.request<File>(`/files/${fileId}/restore`, {
		method: 'POST',
		headers,
		body: JSON.stringify({ version: versionNumber })
	});

	return response;
}

export interface EditFileResponse {
	id: string;
	current_version: number;
	size: number;
	modified_at: string;
	saved_as_new_version: boolean;
}

export async function editFile(
	fileId: string,
	content: string,
	saveMode: 'overwrite' | 'new_version',
	changeDescription?: string
): Promise<EditFileResponse> {
	// Convert content to base64
	const base64Content = btoa(unescape(encodeURIComponent(content)));

	return apiClient.post<EditFileResponse>(`/files/${fileId}/edit`, {
		content: base64Content,
		save_mode: saveMode,
		change_description: changeDescription
	});
}

export async function getFileContent(fileId: string): Promise<string> {
	return apiClient.requestText(`/files/${fileId}/content`);
}

// Trash operations

export interface TrashSummary {
	file_count: number;
	folder_count: number;
	total_size: number;
}

export async function getTrashSummary(): Promise<TrashSummary> {
	return apiClient.get<TrashSummary>('/trash/summary');
}

export async function emptyTrash(): Promise<void> {
	return apiClient.delete('/trash/empty');
}
