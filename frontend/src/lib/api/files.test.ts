import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	getDeletedContents,
	getFileContent,
	getStarredContents,
	listAllFiles
} from '$lib/api/files';
import type { File, Folder } from '$lib/api/types';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn(),
		requestText: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('files API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('fetches file content with an endpoint relative to the API base URL', async () => {
		vi.mocked(apiClient.requestText).mockResolvedValue('{"type":"excalidraw"}');

		await expect(getFileContent('file-123')).resolves.toBe('{"type":"excalidraw"}');
		expect(apiClient.requestText).toHaveBeenCalledWith('/files/file-123/content');
	});

	it('fetches all files across pages for aggregate workspace filters', async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) => file(`file-${index}`));
		const secondPage = [file('file-100'), file('file-101')];
		vi.mocked(apiClient.get)
			.mockResolvedValueOnce(firstPage)
			.mockResolvedValueOnce(secondPage);

		await expect(listAllFiles()).resolves.toEqual([...firstPage, ...secondPage]);
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/files?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/files?page=2&per_page=100');
	});

	it('fetches all starred folders and files across pages', async () => {
		const firstPage = {
			folders: Array.from({ length: 100 }, (_, index) => folder(`folder-${index}`)),
			files: [file('file-0')]
		};
		const secondPage = {
			folders: [folder('folder-100')],
			files: []
		};
		vi.mocked(apiClient.get)
			.mockResolvedValueOnce(firstPage)
			.mockResolvedValueOnce(secondPage);

		await expect(getStarredContents()).resolves.toEqual({
			folders: [...firstPage.folders, ...secondPage.folders],
			files: firstPage.files
		});
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/files/starred?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/files/starred?page=2&per_page=100');
	});

	it('fetches all deleted folders and files across pages', async () => {
		const firstPage = {
			folders: [],
			files: Array.from({ length: 100 }, (_, index) => file(`file-${index}`))
		};
		const secondPage = {
			folders: [],
			files: [file('file-100')]
		};
		vi.mocked(apiClient.get)
			.mockResolvedValueOnce(firstPage)
			.mockResolvedValueOnce(secondPage);

		await expect(getDeletedContents()).resolves.toEqual({
			folders: [],
			files: [...firstPage.files, ...secondPage.files]
		});
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/files/deleted?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/files/deleted?page=2&per_page=100');
	});
});

function file(id: string): File {
	return {
		id,
		name: `${id}.txt`,
		path: `/${id}.txt`,
		size: 1,
		mime_type: 'text/plain',
		parent_folder_id: null,
		owner_id: 'user-1',
		current_version: 1,
		created_at: '2026-01-01T00:00:00Z',
		modified_at: '2026-01-01T00:00:00Z'
	};
}

function folder(id: string): Folder {
	return {
		id,
		name: id,
		path: `/${id}`,
		parent_folder_id: null,
		owner_id: 'user-1',
		created_at: '2026-01-01T00:00:00Z',
		updated_at: '2026-01-01T00:00:00Z'
	};
}
