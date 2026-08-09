import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	getDeletedContents,
	getFileContent,
	getStarredContents,
	listAllFiles,
	downloadFile,
	previewFile
} from '$lib/api/files';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn(),
		requestText: vi.fn(),
		getBaseURL: vi.fn().mockReturnValue('https://api.example.com/api/v1')
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

	it('fetches all file pages before returning aggregate file lists', async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) => ({
			id: `file-${index}`,
			name: `file-${index}.txt`
		}));
		const secondPage = [{ id: 'file-100', name: 'file-100.txt' }];

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(listAllFiles()).resolves.toHaveLength(101);
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/files?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/files?page=2&per_page=100');
	});

	it('fetches all starred folder-content pages before returning aggregate contents', async () => {
		const firstPage = {
			folders: Array.from({ length: 100 }, (_, index) => ({
				id: `folder-${index}`,
				name: `folder-${index}`
			})),
			files: []
		};
		const secondPage = {
			folders: [{ id: 'folder-100', name: 'folder-100' }],
			files: [{ id: 'file-1', name: 'file-1.txt' }]
		};

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(getStarredContents()).resolves.toMatchObject({
			folders: expect.arrayContaining([{ id: 'folder-100', name: 'folder-100' }]),
			files: [{ id: 'file-1', name: 'file-1.txt' }]
		});
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/files/starred?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/files/starred?page=2&per_page=100');
	});

	it('fetches all deleted folder-content pages before returning aggregate contents', async () => {
		const firstPage = {
			folders: [],
			files: Array.from({ length: 100 }, (_, index) => ({
				id: `file-${index}`,
				name: `file-${index}.txt`
			}))
		};
		const secondPage = {
			folders: [],
			files: [{ id: 'file-100', name: 'file-100.txt' }]
		};

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(getDeletedContents()).resolves.toMatchObject({
			folders: [],
			files: expect.arrayContaining([{ id: 'file-100', name: 'file-100.txt' }])
		});
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/files/deleted?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/files/deleted?page=2&per_page=100');
	});
});

describe('file URL construction', () => {
	it('builds download URLs from the configured API base URL', async () => {
		await expect(downloadFile('file-123')).resolves.toEqual({
			url: 'https://api.example.com/api/v1/files/file-123/content'
		});
	});

	it('builds preview URLs from the configured API base URL', async () => {
		await expect(previewFile('file-123')).resolves.toEqual({
			url: 'https://api.example.com/api/v1/files/file-123/preview'
		});
	});
});
