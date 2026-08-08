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
	KanbanCardDetail,
	KanbanEvent
} from './types';
import {
	cardDetailToMarkdown,
	markdownToCardDetail,
	serializeCardMarkdown,
	parseCardMarkdown
} from '$lib/kanban/cardMarkdown';

const MAX_PAGE_SIZE = 100;

export async function listKanbanBoards(limit?: number): Promise<KanbanBoardSummary[]> {
	// When no limit is requested, preserve the original unbounded behaviour by
	// walking all pages. A requested limit returns a single slice.
	if (limit === undefined) {
		const boards: KanbanBoardSummary[] = [];
		let page = 1;

		while (true) {
			const batch = await apiClient.get<KanbanBoardSummary[]>(
				`/applications/kanban/boards?page=${page}&per_page=${MAX_PAGE_SIZE}`
			);
			boards.push(...batch);

			if (batch.length < MAX_PAGE_SIZE) {
				return boards;
			}

			page += 1;
		}
	}

	return apiClient.get<KanbanBoardSummary[]>(`/applications/kanban/boards?per_page=${limit}`);
}

export async function createKanbanBoard(title: string): Promise<KanbanBoard> {
	return apiClient.post<KanbanBoard>('/applications/kanban/boards', { title });
}

export async function getKanbanBoard(boardId: string): Promise<KanbanBoard> {
	return apiClient.get<KanbanBoard>(`/applications/kanban/boards/${boardId}`);
}

export async function updateKanbanBoard(
	boardId: string,
	input: { title?: string }
): Promise<KanbanBoard> {
	return apiClient.patch<KanbanBoard>(`/applications/kanban/boards/${boardId}`, input);
}

export async function archiveKanbanBoard(boardId: string): Promise<void> {
	await apiClient.post(`/applications/kanban/boards/${boardId}/archive`, {});
}

export async function listKanbanCards(boardId: string, limit?: number): Promise<KanbanCard[]> {
	const params = `?per_page=${limit ?? 100}`;
	return apiClient.get<KanbanCard[]>(`/applications/kanban/boards/${boardId}/cards${params}`);
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
	return apiClient.post<KanbanCard>(`/applications/kanban/boards/${boardId}/cards`, input);
}

export async function getKanbanCard(cardId: string): Promise<KanbanCardDetail> {
	return apiClient.get<KanbanCardDetail>(`/applications/kanban/cards/${cardId}/detail`);
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
		checklists?: KanbanChecklistGroup[];
		activity?: KanbanEvent[];
	}
): Promise<KanbanCard> {
	return apiClient.patch<KanbanCard>(`/applications/kanban/cards/${cardId}`, input);
}

export async function updateCardDescription(cardId: string, content: string): Promise<KanbanCard> {
	return apiClient.put<KanbanCard>(`/applications/kanban/cards/${cardId}/description`, { content });
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
	return apiClient.post<KanbanBoard>(`/applications/kanban/cards/${cardId}/move`, {
		board_id: input.boardId,
		target_column_id: input.targetColumnId,
		target_order: input.targetOrder,
		before_card_id: input.beforeCardId,
		after_card_id: input.afterCardId
	});
}

export async function archiveKanbanCard(cardId: string): Promise<KanbanCard> {
	return apiClient.post<KanbanCard>(`/applications/kanban/cards/${cardId}/archive`, {});
}

export async function deleteKanbanCard(cardId: string): Promise<void> {
	await apiClient.delete(`/applications/kanban/cards/${cardId}`);
}

// Labels
export async function createKanbanLabel(
	boardId: string,
	input: { name: string; color: string }
): Promise<KanbanLabel> {
	return apiClient.post<KanbanLabel>(`/applications/kanban/boards/${boardId}/labels`, input);
}

export async function updateKanbanLabel(
	boardId: string,
	labelId: string,
	input: { name?: string; color?: string }
): Promise<KanbanLabel> {
	return apiClient.patch<KanbanLabel>(
		`/applications/kanban/boards/${boardId}/labels/${labelId}`,
		input
	);
}

export async function deleteKanbanLabel(boardId: string, labelId: string): Promise<void> {
	await apiClient.delete(`/applications/kanban/boards/${boardId}/labels/${labelId}`);
}

export async function addCardLabel(cardId: string, labelId: string): Promise<void> {
	await apiClient.post(`/applications/kanban/cards/${cardId}/labels`, { labelId });
}

export async function removeCardLabel(cardId: string, labelId: string): Promise<void> {
	await apiClient.delete(`/applications/kanban/cards/${cardId}/labels/${labelId}`);
}

// Assignees
export async function getKanbanAssignableUsers(): Promise<KanbanAssignee[]> {
	return apiClient.get<KanbanAssignee[]>('/applications/kanban/assignable-users');
}

export async function assignCardMember(cardId: string, assigneeId: string): Promise<void> {
	await apiClient.post(`/applications/kanban/cards/${cardId}/assignees`, { assigneeId });
}

export async function unassignCardMember(cardId: string, assigneeId: string): Promise<void> {
	await apiClient.delete(`/applications/kanban/cards/${cardId}/assignees/${assigneeId}`);
}

// Attachments
export async function addCardAttachment(cardId: string, file: File): Promise<KanbanCardAttachment> {
	const formData = new FormData();
	formData.append('file', file);
	return apiClient.post<KanbanCardAttachment>(
		`/applications/kanban/cards/${cardId}/attachments`,
		formData
	);
}

export async function deleteCardAttachment(cardId: string, attachmentId: string): Promise<void> {
	await apiClient.delete(`/applications/kanban/cards/${cardId}/attachments/${attachmentId}`);
}

// Checklists
export async function createChecklist(
	cardId: string,
	title: string
): Promise<KanbanChecklistGroup> {
	return apiClient.post<KanbanChecklistGroup>(`/applications/kanban/cards/${cardId}/checklists`, {
		title
	});
}

export async function createChecklistItem(
	cardId: string,
	checklistId: string,
	text: string
): Promise<KanbanChecklistItem> {
	return apiClient.post<KanbanChecklistItem>(
		`/applications/kanban/cards/${cardId}/checklists/${checklistId}/items`,
		{ text }
	);
}

export async function toggleChecklistItem(
	cardId: string,
	checklistId: string,
	itemId: string,
	done: boolean
): Promise<void> {
	await apiClient.patch(
		`/applications/kanban/cards/${cardId}/checklists/${checklistId}/items/${itemId}`,
		{
			done
		}
	);
}

export async function deleteChecklistItem(
	cardId: string,
	checklistId: string,
	itemId: string
): Promise<void> {
	await apiClient.delete(
		`/applications/kanban/cards/${cardId}/checklists/${checklistId}/items/${itemId}`
	);
}

export async function deleteChecklist(cardId: string, checklistId: string): Promise<void> {
	await apiClient.delete(`/applications/kanban/cards/${cardId}/checklists/${checklistId}`);
}

/**
 * Save a complete card by sending the full card data.
 * The backend will serialize to Markdown.
 */
export async function saveKanbanCardDetail(card: KanbanCardDetail): Promise<KanbanCardDetail> {
	const updated = await updateKanbanCard(card.id, {
		title: card.title,
		content: card.content,
		priority: card.priority,
		labels: card.labels.map((l) => l.id),
		assignees: card.assignees.map((a) => a.id),
		due_date: card.due_date,
		checklists: card.checklists,
		activity: card.activity
	});
	return {
		...card,
		...updated,
		content: card.content,
		checklists: card.checklists,
		activity: card.activity
	};
}

export async function addKanbanCardComment(cardId: string, text: string): Promise<void> {
	// For now, this is a no-op on the backend side
	// The comment will be saved as part of the card Markdown on full save
	// In the future, we may have a dedicated endpoint
	console.log('Comment added:', text);
}
