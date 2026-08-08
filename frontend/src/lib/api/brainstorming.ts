import { apiClient } from './client';

export interface BrainstormBoard {
	id: string;
	title: string;
	slug: string;
	path: string;
	template: string;
	source_file_id: string | null;
	preview_file_id: string | null;
	created_at: string;
	updated_at: string;
}

export interface ListBoardsResponse {
	boards: BrainstormBoard[];
}

export interface CreateBoardRequest {
	title: string;
	template_key: string;
}

export interface CreateBoardResponse {
	id: string;
	title: string;
	slug: string;
	path: string;
	template: string;
	created_at: string;
	updated_at: string;
}

export interface GetBoardSourceResponse {
	source: string;
}

export interface SaveBoardSourceRequest {
	source: string;
}

const MAX_PAGE_SIZE = 100;

export async function listBrainstormBoards(): Promise<BrainstormBoard[]> {
	const boards: BrainstormBoard[] = [];
	let page = 1;

	while (true) {
		const response = await apiClient.get<ListBoardsResponse>(
			`/applications/brainstorming/boards?page=${page}&per_page=${MAX_PAGE_SIZE}`
		);
		boards.push(...response.boards);

		if (response.boards.length < MAX_PAGE_SIZE) {
			return boards;
		}

		page += 1;
	}
}

export async function createBrainstormBoard(
	title: string,
	templateKey: string
): Promise<CreateBoardResponse> {
	return apiClient.post<CreateBoardResponse>('/applications/brainstorming/boards', {
		title,
		template_key: templateKey
	});
}

export async function getBrainstormBoard(boardId: string): Promise<BrainstormBoard> {
	return apiClient.get<BrainstormBoard>(`/applications/brainstorming/boards/${boardId}`);
}

export async function getBrainstormBoardSource(boardId: string): Promise<string> {
	const response = await apiClient.get<GetBoardSourceResponse>(
		`/applications/brainstorming/boards/${boardId}/source`
	);
	return response.source;
}

export async function saveBrainstormBoardSource(
	boardId: string,
	source: string
): Promise<BrainstormBoard> {
	return apiClient.put<BrainstormBoard>(`/applications/brainstorming/boards/${boardId}/source`, {
		source
	});
}

export async function updateBrainstormBoardPreview(
	boardId: string,
	pngBlob: Blob
): Promise<BrainstormBoard> {
	return apiClient.request<BrainstormBoard>(
		`/applications/brainstorming/boards/${boardId}/preview`,
		{
			method: 'PUT',
			body: pngBlob,
			headers: { 'Content-Type': 'image/png' }
		}
	);
}

export async function deleteBrainstormBoard(boardId: string): Promise<void> {
	return apiClient.delete(`/applications/brainstorming/boards/${boardId}`);
}
