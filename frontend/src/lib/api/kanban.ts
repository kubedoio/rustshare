import { apiClient } from './client';
import type {
	KanbanAssignee,
	KanbanBoard,
	KanbanBoardSummary,
	KanbanCard,
	KanbanLabel,
	KanbanCardAttachment,
	KanbanChecklistGroup,
	KanbanChecklistItem,
	KanbanCardDetail
} from './types';

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
		labels?: string[];
		assignees?: string[];
		due_date?: string | null;
	}
): Promise<KanbanCard> {
	return apiClient.post<KanbanCard>(`/modules/kanban/boards/${boardId}/cards`, input);
}

export async function getKanbanCard(cardId: string): Promise<KanbanCardDetail> {
	return apiClient.get<KanbanCardDetail>(`/modules/kanban/cards/${cardId}/detail`);
}

export async function updateKanbanCard(
	cardId: string,
	input: {
		title?: string;
		content?: string;
		priority?: string;
		labels?: string[];
		assignees?: string[];
		due_date?: string | null;
	}
): Promise<KanbanCard> {
	return apiClient.patch<KanbanCard>(`/modules/kanban/cards/${cardId}`, input);
}

export async function updateCardDescription(
	cardId: string,
	content: string
): Promise<KanbanCard> {
	return apiClient.put<KanbanCard>(`/modules/kanban/cards/${cardId}/description`, { content });
}

export async function moveKanbanCard(
	cardId: string,
	input: {
		boardId: string;
		targetColumnId: string;
		targetOrder?: number;
		beforeCardId?: string;
		afterCardId?: string;
	}
): Promise<KanbanBoard> {
	return apiClient.post<KanbanBoard>(`/modules/kanban/cards/${cardId}/move`, {
		board_id: input.boardId,
		target_column_id: input.targetColumnId,
		target_order: input.targetOrder,
		before_card_id: input.beforeCardId,
		after_card_id: input.afterCardId
	});
}

export async function archiveKanbanCard(cardId: string): Promise<KanbanCard> {
	return apiClient.post<KanbanCard>(`/modules/kanban/cards/${cardId}/archive`, {});
}

export async function deleteKanbanCard(cardId: string): Promise<void> {
	await apiClient.delete(`/modules/kanban/cards/${cardId}`);
}

// Labels
export async function createKanbanLabel(
	boardId: string,
	input: { name: string; color: string }
): Promise<KanbanLabel> {
	return apiClient.post<KanbanLabel>(`/modules/kanban/boards/${boardId}/labels`, input);
}

export async function updateKanbanLabel(
	boardId: string,
	labelId: string,
	input: { name?: string; color?: string }
): Promise<KanbanLabel> {
	return apiClient.patch<KanbanLabel>(`/modules/kanban/boards/${boardId}/labels/${labelId}`, input);
}

export async function deleteKanbanLabel(boardId: string, labelId: string): Promise<void> {
	await apiClient.delete(`/modules/kanban/boards/${boardId}/labels/${labelId}`);
}

export async function addCardLabel(cardId: string, labelId: string): Promise<void> {
	await apiClient.post(`/modules/kanban/cards/${cardId}/labels`, { labelId });
}

export async function removeCardLabel(cardId: string, labelId: string): Promise<void> {
	await apiClient.delete(`/modules/kanban/cards/${cardId}/labels/${labelId}`);
}

// Assignees
export async function getKanbanAssignableUsers(): Promise<KanbanAssignee[]> {
	return apiClient.get<KanbanAssignee[]>('/modules/kanban/assignable-users');
}

export async function assignCardMember(cardId: string, assigneeId: string): Promise<void> {
	await apiClient.post(`/modules/kanban/cards/${cardId}/assignees`, { assigneeId });
}

export async function unassignCardMember(cardId: string, assigneeId: string): Promise<void> {
	await apiClient.delete(`/modules/kanban/cards/${cardId}/assignees/${assigneeId}`);
}

// Attachments
export async function addCardAttachment(cardId: string, file: File): Promise<KanbanCardAttachment> {
	const formData = new FormData();
	formData.append('file', file);
	return apiClient.post<KanbanCardAttachment>(`/modules/kanban/cards/${cardId}/attachments`, formData);
}

export async function deleteCardAttachment(cardId: string, attachmentId: string): Promise<void> {
	await apiClient.delete(`/modules/kanban/cards/${cardId}/attachments/${attachmentId}`);
}

// Checklists
export async function createChecklist(cardId: string, title: string): Promise<KanbanChecklistGroup> {
	return apiClient.post<KanbanChecklistGroup>(`/modules/kanban/cards/${cardId}/checklists`, { title });
}

export async function createChecklistItem(
	cardId: string,
	checklistId: string,
	text: string
): Promise<KanbanChecklistItem> {
	return apiClient.post<KanbanChecklistItem>(
		`/modules/kanban/cards/${cardId}/checklists/${checklistId}/items`,
		{ text }
	);
}

export async function toggleChecklistItem(
	cardId: string,
	checklistId: string,
	itemId: string,
	done: boolean
): Promise<void> {
	await apiClient.patch(`/modules/kanban/cards/${cardId}/checklists/${checklistId}/items/${itemId}`, {
		done
	});
}

export async function deleteChecklistItem(
	cardId: string,
	checklistId: string,
	itemId: string
): Promise<void> {
	await apiClient.delete(`/modules/kanban/cards/${cardId}/checklists/${checklistId}/items/${itemId}`);
}

export async function deleteChecklist(cardId: string, checklistId: string): Promise<void> {
	await apiClient.delete(`/modules/kanban/cards/${cardId}/checklists/${checklistId}`);
}
