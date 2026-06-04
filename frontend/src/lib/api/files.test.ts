import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getFileContent } from '$lib/api/files';

vi.mock('$lib/api/client', () => ({
	apiClient: {
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
});
