import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	listBrainstormBoards,
	createBrainstormBoard,
	getBrainstormBoard,
	getBrainstormBoardSource,
	saveBrainstormBoardSource,
	updateBrainstormBoardPreview,
	deleteBrainstormBoard
} from '$lib/api/brainstorming';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn(),
		post: vi.fn(),
		put: vi.fn(),
		delete: vi.fn(),
		request: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('brainstorming API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	const mockBoard = {
		id: 'board-1',
		title: 'Test Board',
		slug: 'test-board',
		path: '/Brainstorming/test-board',
		template: 'template_blank_brainstorm',
		source_file_id: 'file-1',
		preview_file_id: 'file-2',
		created_at: '2026-04-30T00:00:00Z',
		updated_at: '2026-04-30T12:00:00Z'
	};

	it('lists brainstorming boards', async () => {
		vi.mocked(apiClient.get).mockResolvedValue({ boards: [mockBoard] });

		const result = await listBrainstormBoards();
		expect(result).toEqual([mockBoard]);
		expect(apiClient.get).toHaveBeenCalledWith('/modules/brainstorming/boards');
	});

	it('creates a brainstorming board', async () => {
		const response = {
			id: 'board-1',
			title: 'New Board',
			slug: 'new-board',
			path: '/Brainstorming/new-board',
			template: 'template_blank_brainstorm',
			created_at: '2026-04-30T00:00:00Z',
			updated_at: '2026-04-30T00:00:00Z'
		};
		vi.mocked(apiClient.post).mockResolvedValue(response);

		const result = await createBrainstormBoard('New Board', 'template_blank_brainstorm');
		expect(result).toEqual(response);
		expect(apiClient.post).toHaveBeenCalledWith('/modules/brainstorming/boards', {
			title: 'New Board',
			template_key: 'template_blank_brainstorm'
		});
	});

	it('gets a brainstorming board', async () => {
		vi.mocked(apiClient.get).mockResolvedValue(mockBoard);

		const result = await getBrainstormBoard('board-1');
		expect(result).toEqual(mockBoard);
		expect(apiClient.get).toHaveBeenCalledWith('/modules/brainstorming/boards/board-1');
	});

	it('gets board source', async () => {
		vi.mocked(apiClient.get).mockResolvedValue({ source: '{"type":"excalidraw"}' });

		const result = await getBrainstormBoardSource('board-1');
		expect(result).toEqual('{"type":"excalidraw"}');
		expect(apiClient.get).toHaveBeenCalledWith('/modules/brainstorming/boards/board-1/source');
	});

	it('saves board source', async () => {
		vi.mocked(apiClient.put).mockResolvedValue(mockBoard);

		const result = await saveBrainstormBoardSource('board-1', '{"type":"excalidraw"}');
		expect(result).toEqual(mockBoard);
		expect(apiClient.put).toHaveBeenCalledWith('/modules/brainstorming/boards/board-1/source', {
			source: '{"type":"excalidraw"}'
		});
	});

	it('updates board preview', async () => {
		vi.mocked(apiClient.request).mockResolvedValue(mockBoard);

		const blob = new Blob(['pngdata'], { type: 'image/png' });
		const result = await updateBrainstormBoardPreview('board-1', blob);
		expect(result).toEqual(mockBoard);
		expect(apiClient.request).toHaveBeenCalledWith(
			'/modules/brainstorming/boards/board-1/preview',
			{
				method: 'PUT',
				body: blob,
				headers: { 'Content-Type': 'image/png' }
			}
		);
	});

	it('deletes a brainstorming board', async () => {
		vi.mocked(apiClient.delete).mockResolvedValue(null);

		await deleteBrainstormBoard('board-1');
		expect(apiClient.delete).toHaveBeenCalledWith('/modules/brainstorming/boards/board-1');
	});
});
