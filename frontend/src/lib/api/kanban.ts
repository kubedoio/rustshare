import { apiClient } from './client';
import type { KanbanBoard, KanbanBoardSummary, KanbanCard } from './types';

export async function listKanbanBoards(limit?: number): Promise<KanbanBoardSummary[]> {
	const params = limit !== undefined ? `?limit=${limit}` : '';
	return apiClient.get<KanbanBoardSummary[]>(`/modules/kanban/boards${params}`);
}

export async function createKanbanBoard(title: string): Promise<KanbanBoard> {
	return apiClient.post<KanbanBoard>('/modules/kanban/boards', { title });
}

export async function getKanbanBoard(boardId: string): Promise<KanbanBoard> {
	return apiClient.get<KanbanBoard>(`/modules/kanban/boards/${boardId}`);
}

export async function updateKanbanBoard(
	boardId: string,
	input: { title?: string }
): Promise<KanbanBoard> {
	return apiClient.patch<KanbanBoard>(`/modules/kanban/boards/${boardId}`, input);
}

export async function archiveKanbanBoard(boardId: string): Promise<void> {
	await apiClient.post(`/modules/kanban/boards/${boardId}/archive`, {});
}

export async function listKanbanCards(boardId: string): Promise<KanbanCard[]> {
	return apiClient.get<KanbanCard[]>(`/modules/kanban/boards/${boardId}/cards`);
}

export async function createKanbanCard(
	boardId: string,
	input: {
		title: string;
		column_id: string;
		content?: string;
		priority?: string;
		tags?: string[];
	}
): Promise<KanbanCard> {
	return apiClient.post<KanbanCard>(`/modules/kanban/boards/${boardId}/cards`, input);
}

export async function getKanbanCard(cardId: string): Promise<KanbanCard> {
	return apiClient.get<KanbanCard>(`/modules/kanban/cards/${cardId}`);
}

export async function updateKanbanCard(
	cardId: string,
	input: {
		title?: string;
		content?: string;
		priority?: string;
		tags?: string[];
		assignees?: string[];
	}
): Promise<KanbanCard> {
	return apiClient.patch<KanbanCard>(`/modules/kanban/cards/${cardId}`, input);
}

export async function moveKanbanCard(
	cardId: string,
	targetColumnId: string,
	targetOrder: number
): Promise<KanbanBoard> {
	return apiClient.post<KanbanBoard>(`/modules/kanban/cards/${cardId}/move`, {
		target_column_id: targetColumnId,
		target_order: targetOrder
	});
}

export async function archiveKanbanCard(cardId: string): Promise<KanbanCard> {
	return apiClient.post<KanbanCard>(`/modules/kanban/cards/${cardId}/archive`, {});
}

export async function deleteKanbanCard(cardId: string): Promise<void> {
	await apiClient.delete(`/modules/kanban/cards/${cardId}`);
}
