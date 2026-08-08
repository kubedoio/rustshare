import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock dependencies before importing the module under test
vi.mock('$lib/query-client', () => ({
	queryClient: {
		invalidateQueries: vi.fn()
	}
}));

vi.mock('$lib/stores/toast', () => ({
	toastStore: {
		show: vi.fn()
	}
}));

vi.mock('$lib/stores/replication', () => ({
	replicationStore: {
		upsert: vi.fn(),
		remove: vi.fn()
	}
}));

vi.mock('./client', () => ({
	getWebSocketClient: vi.fn(),
	disconnectWebSocket: vi.fn()
}));

import { queryClient } from '$lib/query-client';
import { toastStore } from '$lib/stores/toast';
import { getWebSocketClient, disconnectWebSocket } from './client';
import { initializeWebSocket, cleanupWebSocket } from './manager';

describe('WebSocket Manager - Application Invalidations', () => {
	const handlers: Record<string, (event: unknown) => void> = {};

	beforeEach(() => {
		vi.clearAllMocks();
		cleanupWebSocket();
		handlers['KanbanModified'] = () => {};
		handlers['NoteModified'] = () => {};
		handlers['BrainstormBoardModified'] = () => {};
		handlers['MeetingNoteModified'] = () => {};
		handlers['DecisionModified'] = () => {};
		handlers['StandupModified'] = () => {};

		const mockClient = {
			on: (event: string, handler: (event: unknown) => void) => {
				handlers[event] = handler;
			},
			connect: vi.fn().mockResolvedValue(undefined)
		};
		vi.mocked(getWebSocketClient).mockReturnValue(
			mockClient as unknown as ReturnType<typeof getWebSocketClient>
		);
	});

	it('invalidates kanban queries on KanbanModified', async () => {
		await initializeWebSocket('token', 'user-1');
		const event = {
			payload: { board_id: 'board-1', card_id: 'card-1' },
			user_id: 'user-2'
		};
		handlers['KanbanModified'](event);

		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['kanban-board', 'board-1']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['kanban-cards', 'board-1']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['kanban-boards'] });
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['kanban-card', 'card-1']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['kanban-card-detail', 'card-1']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['workspace-module-summaries']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['enabled-modules'] });
	});

	it('invalidates note queries on NoteModified', async () => {
		await initializeWebSocket('token', 'user-1');
		const event = {
			payload: { note_id: 'note-1', title: 'Test Note' },
			user_id: 'user-2'
		};
		handlers['NoteModified'](event);

		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['note', 'note-1'] });
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['notes'] });
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['workspace-module-summaries']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['enabled-modules'] });
	});

	it('invalidates dashboard summaries on BrainstormBoardModified', async () => {
		await initializeWebSocket('token', 'user-1');
		const event = {
			payload: { board_id: 'board-1', title: 'Brainstorm' },
			user_id: 'user-2'
		};
		handlers['BrainstormBoardModified'](event);

		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['brainstorm-board', 'board-1']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['brainstorm-boards'] });
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['workspace-module-summaries']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['enabled-modules'] });
	});

	it('invalidates dashboard summaries on MeetingNoteModified', async () => {
		await initializeWebSocket('token', 'user-1');
		const event = {
			payload: { meeting_id: 'meet-1', title: 'Sprint' },
			user_id: 'user-2'
		};
		handlers['MeetingNoteModified'](event);

		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['meetings'] });
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['workspace-module-summaries']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['enabled-modules'] });
	});

	it('invalidates dashboard summaries on DecisionModified', async () => {
		await initializeWebSocket('token', 'user-1');
		const event = {
			payload: { decision_id: 'dec-1', title: 'ADR' },
			user_id: 'user-2'
		};
		handlers['DecisionModified'](event);

		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['decisions'] });
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['workspace-module-summaries']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['enabled-modules'] });
	});

	it('invalidates dashboard summaries on StandupModified', async () => {
		await initializeWebSocket('token', 'user-1');
		const event = {
			payload: { standup_id: 'stand-1', title: 'Daily' },
			user_id: 'user-2'
		};
		handlers['StandupModified'](event);

		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['standups'] });
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['workspace-module-summaries']
		});
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['enabled-modules'] });
	});
});
