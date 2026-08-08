import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import {
	activityStore,
	serverActivityStore,
	getActivityDisplay,
	getRelativeTime,
	getActivityHref,
	type Activity as ActivityItem,
	type ActivityType
} from '$lib/stores/activity';
import {
	StickyNote,
	Pencil,
	CalendarDays,
	Activity as ActivityIcon,
	Columns,
	GitBranch,
	Lightbulb
} from 'lucide-svelte';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('Activity Store', () => {
	beforeEach(() => {
		activityStore.clearHistory();
		localStorage.clear();
		serverActivityStore.reset();
		vi.clearAllMocks();
	});

	describe('Initial State', () => {
		it('should start with empty history', () => {
			expect(get(activityStore)).toEqual([]);
		});

		it('should start with empty history on fresh import', async () => {
			// activityStore no longer persists to localStorage;
			// serverActivityStore is the canonical source.
			localStorage.setItem(
				'activity-history',
				JSON.stringify([
					{
						id: '1',
						type: 'file_uploaded',
						fileName: 'test.txt',
						timestamp: new Date().toISOString()
					}
				])
			);

			vi.resetModules();
			const { activityStore: freshStore } = await import('./activity');
			const activities = get(freshStore);

			expect(activities).toEqual([]);
		});

		it('should handle corrupted localStorage gracefully', async () => {
			localStorage.setItem('activity-history', 'invalid json');

			vi.resetModules();
			const { activityStore: freshStore } = await import('./activity');
			const activities = get(freshStore);

			expect(activities).toEqual([]);
		});
	});

	describe('addActivity', () => {
		it('should add new activity to the beginning', () => {
			activityStore.addActivity('file_uploaded', 'test.txt');

			const activities = get(activityStore);
			expect(activities).toHaveLength(1);
			expect(activities[0].fileName).toBe('test.txt');
			expect(activities[0].type).toBe('file_uploaded');
		});

		it('should add activity with details', () => {
			activityStore.addActivity('file_renamed', 'new.txt', { details: 'old.txt' });

			const activities = get(activityStore);
			expect(activities[0].details).toBe('old.txt');
		});

		it('should add activity with artifactId and applicationId', () => {
			activityStore.addActivity('note_created', 'My Note', {
				artifactId: 'abc',
				applicationId: 'notes'
			});

			const activities = get(activityStore);
			expect(activities[0].type).toBe('note_created');
			expect(activities[0].fileName).toBe('My Note');
			expect(activities[0].artifactId).toBe('abc');
			expect(activities[0].applicationId).toBe('notes');
		});

		it('should maintain chronological order (newest first)', () => {
			activityStore.addActivity('file_uploaded', 'first.txt');
			activityStore.addActivity('file_uploaded', 'second.txt');
			activityStore.addActivity('file_uploaded', 'third.txt');

			const activities = get(activityStore);
			expect(activities[0].fileName).toBe('third.txt');
			expect(activities[1].fileName).toBe('second.txt');
			expect(activities[2].fileName).toBe('first.txt');
		});

		it('should limit to 50 activities', () => {
			// Add 60 activities
			for (let i = 0; i < 60; i++) {
				activityStore.addActivity('file_uploaded', `file-${i}.txt`);
			}

			const activities = get(activityStore);
			expect(activities).toHaveLength(50);
			expect(activities[0].fileName).toBe('file-59.txt'); // Most recent
			expect(activities[49].fileName).toBe('file-10.txt'); // 50th activity
		});

		it('should generate unique IDs', () => {
			activityStore.addActivity('file_uploaded', 'test.txt');
			activityStore.addActivity('file_uploaded', 'test.txt');

			const activities = get(activityStore);
			expect(activities[0].id).not.toBe(activities[1].id);
		});

		it('should add activity to store', () => {
			activityStore.addActivity('file_uploaded', 'test.txt');

			const activities = get(activityStore);
			expect(activities).toHaveLength(1);
			expect(activities[0].fileName).toBe('test.txt');
		});
	});

	describe('clearHistory', () => {
		it('should clear all activities', () => {
			activityStore.addActivity('file_uploaded', 'test1.txt');
			activityStore.addActivity('file_uploaded', 'test2.txt');

			activityStore.clearHistory();

			expect(get(activityStore)).toEqual([]);
		});

		it('should clear all activities', () => {
			activityStore.addActivity('file_uploaded', 'test.txt');
			activityStore.clearHistory();

			expect(get(activityStore)).toEqual([]);
		});
	});

	describe('removeActivity', () => {
		it('should remove specific activity by ID', () => {
			activityStore.addActivity('file_uploaded', 'test1.txt');
			activityStore.addActivity('file_uploaded', 'test2.txt');

			const activities = get(activityStore);
			const idToRemove = activities[0].id;

			activityStore.removeActivity(idToRemove);

			const updated = get(activityStore);
			expect(updated).toHaveLength(1);
			expect(updated[0].id).not.toBe(idToRemove);
		});

		it('should remove activity by ID', () => {
			activityStore.addActivity('file_uploaded', 'test1.txt');
			activityStore.addActivity('file_uploaded', 'test2.txt');

			const activities = get(activityStore);
			activityStore.removeActivity(activities[0].id);

			const updated = get(activityStore);
			expect(updated).toHaveLength(1);
		});

		it('should handle removing non-existent ID gracefully', () => {
			activityStore.addActivity('file_uploaded', 'test.txt');
			activityStore.removeActivity('non-existent-id');

			const activities = get(activityStore);
			expect(activities).toHaveLength(1);
		});
	});

	describe('serverActivityStore', () => {
		it('should fetch items from API and map to Activity type', async () => {
			const mockResponse = {
				items: [
					{
						id: 'evt-1',
						action: 'file_uploaded',
						resource_type: 'file',
						resource_id: 'file-1',
						resource_name: 'document.pdf',
						actor_id: 'user-1',
						timestamp: '2026-05-30T10:00:00Z'
					},
					{
						id: 'evt-2',
						action: 'note_modified',
						resource_type: 'module',
						resource_id: 'note-1',
						resource_name: 'My Note',
						actor_id: 'user-1',
						timestamp: '2026-05-30T09:00:00Z'
					}
				],
				next_cursor: null
			};

			vi.mocked(apiClient.get).mockResolvedValue(mockResponse);

			await serverActivityStore.fetch(10);

			const state = get(serverActivityStore);
			expect(state.items).toHaveLength(2);
			expect(state.items[0].type).toBe('file_uploaded');
			expect(state.items[0].fileName).toBe('document.pdf');
			expect(state.items[0].artifactId).toBe('file-1');
			expect(state.items[0].accessible).toBe(true);
			expect(state.items[1].type).toBe('note_modified');
			expect(state.items[1].applicationId).toBe('notes');
			expect(state.loading).toBe(false);
			expect(state.error).toBeNull();
			expect(state.hasMore).toBe(false);
		});

		it('should handle fetch errors', async () => {
			vi.mocked(apiClient.get).mockRejectedValue(new Error('Network error'));

			await serverActivityStore.fetch(10);

			const state = get(serverActivityStore);
			expect(state.items).toEqual([]);
			expect(state.loading).toBe(false);
			expect(state.error).toBe('Network error');
		});

		it('should support pagination with loadMore', async () => {
			const firstPage = {
				items: [
					{
						id: 'evt-1',
						action: 'file_uploaded',
						resource_type: 'file',
						resource_id: 'file-1',
						resource_name: 'first.pdf',
						actor_id: 'user-1',
						timestamp: '2026-05-30T10:00:00Z'
					}
				],
				next_cursor: {
					before_timestamp: '2026-05-30T10:00:00Z',
					before_id: 'evt-1'
				}
			};

			const secondPage = {
				items: [
					{
						id: 'evt-2',
						action: 'folder_created',
						resource_type: 'folder',
						resource_id: 'folder-1',
						resource_name: 'Documents',
						actor_id: 'user-1',
						timestamp: '2026-05-30T09:00:00Z'
					}
				],
				next_cursor: null
			};

			vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

			await serverActivityStore.fetch(10);
			expect(get(serverActivityStore).items).toHaveLength(1);
			expect(get(serverActivityStore).hasMore).toBe(true);

			await serverActivityStore.loadMore(10);
			const state = get(serverActivityStore);
			expect(state.items).toHaveLength(2);
			expect(state.items[1].type).toBe('folder_created');
			expect(state.hasMore).toBe(false);
		});

		it('should map unknown server actions to file_uploaded fallback', async () => {
			const mockResponse = {
				items: [
					{
						id: 'evt-1',
						action: 'unknown_event',
						resource_type: 'file',
						resource_id: 'file-1',
						resource_name: 'test.txt',
						actor_id: 'user-1',
						timestamp: '2026-05-30T10:00:00Z'
					}
				],
				next_cursor: null
			};

			vi.mocked(apiClient.get).mockResolvedValue(mockResponse);

			await serverActivityStore.fetch(10);
			expect(get(serverActivityStore).items[0].type).toBe('file_uploaded');
		});

		it('should infer module keys correctly', async () => {
			const mockResponse = {
				items: [
					{
						id: 'evt-1',
						action: 'brainstorm_board_modified',
						resource_type: 'module',
						resource_id: 'brd-1',
						resource_name: 'Ideas',
						actor_id: 'user-1',
						timestamp: '2026-05-30T10:00:00Z'
					},
					{
						id: 'evt-2',
						action: 'share_created',
						resource_type: 'share',
						resource_id: 'share-1',
						resource_name: 'shared.pdf',
						actor_id: 'user-1',
						timestamp: '2026-05-30T09:00:00Z'
					}
				],
				next_cursor: null
			};

			vi.mocked(apiClient.get).mockResolvedValue(mockResponse);

			await serverActivityStore.fetch(10);
			const items = get(serverActivityStore).items;
			expect(items[0].applicationId).toBe('brainstorming');
			expect(items[1].applicationId).toBe('shares');
		});

		it('should not call loadMore when already loading', async () => {
			vi.mocked(apiClient.get).mockImplementation(
				() => new Promise((resolve) => setTimeout(resolve, 100))
			);

			serverActivityStore.fetch(10);
			const stateBefore = get(serverActivityStore);
			expect(stateBefore.loading).toBe(true);

			await serverActivityStore.loadMore(10);
			// Should not trigger another fetch while loading
			expect(vi.mocked(apiClient.get)).toHaveBeenCalledTimes(1);
		});

		it('should reset state', async () => {
			const mockResponse = {
				items: [
					{
						id: 'evt-1',
						action: 'file_uploaded',
						resource_type: 'file',
						resource_id: 'f1',
						resource_name: 'a.txt',
						actor_id: 'u1',
						timestamp: '2026-05-30T10:00:00Z'
					}
				],
				next_cursor: null
			};
			vi.mocked(apiClient.get).mockResolvedValue(mockResponse);

			await serverActivityStore.fetch(10);
			expect(get(serverActivityStore).items).toHaveLength(1);

			serverActivityStore.reset();
			const state = get(serverActivityStore);
			expect(state.items).toEqual([]);
			expect(state.loading).toBe(false);
			expect(state.error).toBeNull();
			expect(state.hasMore).toBe(true);
		});
	});

	describe('getActivityDisplay', () => {
		const testCases: Array<{
			type: ActivityType;
			fileName: string;
			details?: string;
			expectedIcon: any;
			expectedTitle: string;
		}> = [
			{
				type: 'file_uploaded',
				fileName: 'test.txt',
				expectedIcon: '📤',
				expectedTitle: 'File Uploaded'
			},
			{
				type: 'file_downloaded',
				fileName: 'test.txt',
				expectedIcon: '📥',
				expectedTitle: 'File Downloaded'
			},
			{
				type: 'file_deleted',
				fileName: 'test.txt',
				expectedIcon: '🗑️',
				expectedTitle: 'File Deleted'
			},
			{
				type: 'file_renamed',
				fileName: 'new.txt',
				details: 'old.txt',
				expectedIcon: '✏️',
				expectedTitle: 'File Renamed'
			},
			{
				type: 'folder_created',
				fileName: 'Documents',
				expectedIcon: '📁',
				expectedTitle: 'Folder Created'
			},
			{
				type: 'share_created',
				fileName: 'test.txt',
				expectedIcon: '🔗',
				expectedTitle: 'Share Link Created'
			},
			{
				type: 'note_created',
				fileName: 'My Note',
				expectedIcon: StickyNote,
				expectedTitle: 'Note created'
			},
			{
				type: 'note_edited',
				fileName: 'My Note',
				expectedIcon: Pencil,
				expectedTitle: 'Note edited'
			},
			{
				type: 'meeting_created',
				fileName: 'Meeting',
				expectedIcon: CalendarDays,
				expectedTitle: 'Meeting note created'
			},
			{
				type: 'standup_created',
				fileName: 'Standup',
				expectedIcon: ActivityIcon,
				expectedTitle: 'Standup record created'
			},
			{
				type: 'kanban_created',
				fileName: 'Board',
				expectedIcon: Columns,
				expectedTitle: 'Kanban board created'
			},
			{
				type: 'decision_created',
				fileName: 'Decision',
				expectedIcon: GitBranch,
				expectedTitle: 'Decision recorded'
			},
			{
				type: 'brainstorm_created',
				fileName: 'Brainstorm',
				expectedIcon: Lightbulb,
				expectedTitle: 'Idea board created'
			}
		];

		testCases.forEach(({ type, fileName, details, expectedIcon, expectedTitle }) => {
			it(`should display correct info for ${type}`, () => {
				const activity: ActivityItem = {
					id: '1',
					type,
					fileName,
					details,
					timestamp: new Date().toISOString()
				};

				const display = getActivityDisplay(activity);

				expect(display.icon).toBe(expectedIcon);
				expect(display.title).toBe(expectedTitle);
				expect(display.description).toBeTruthy();
			});
		});

		it('should include old name in rename description', () => {
			const activity: ActivityItem = {
				id: '1',
				type: 'file_renamed',
				fileName: 'new.txt',
				details: 'old.txt',
				timestamp: new Date().toISOString()
			};

			const display = getActivityDisplay(activity);
			expect(display.description).toContain('old.txt');
			expect(display.description).toContain('new.txt');
		});

		it('should handle old-format activity without artifactId and applicationId', () => {
			const oldActivity = {
				id: 'old-1',
				type: 'file_uploaded',
				fileName: 'legacy.txt',
				timestamp: new Date().toISOString()
			};

			const display = getActivityDisplay(oldActivity as ActivityItem);
			expect(display.icon).toBe('📤');
			expect(display.title).toBe('File Uploaded');
			expect(display.description).toBeTruthy();
		});

		it('should display new server activity types', () => {
			const newTypes: Array<{ type: ActivityType; title: string }> = [
				{ type: 'file_restored', title: 'File Restored' },
				{ type: 'share_updated', title: 'Share Link Updated' },
				{ type: 'share_received', title: 'Share Received' },
				{ type: 'share_permission_changed', title: 'Share Permission Changed' },
				{ type: 'share_revoked_from_user', title: 'Share Revoked' },
				{ type: 'note_modified', title: 'Note modified' },
				{ type: 'meeting_note_modified', title: 'Meeting note modified' },
				{ type: 'standup_modified', title: 'Standup record modified' },
				{ type: 'kanban_modified', title: 'Kanban board modified' },
				{ type: 'decision_modified', title: 'Decision modified' },
				{ type: 'brainstorm_board_modified', title: 'Idea board modified' }
			];

			newTypes.forEach(({ type, title }) => {
				const activity: ActivityItem = {
					id: '1',
					type,
					fileName: 'test.txt',
					timestamp: new Date().toISOString()
				};
				const display = getActivityDisplay(activity);
				expect(display.title).toBe(title);
				expect(display.description).toBeTruthy();
			});
		});
	});

	describe('getActivityHref', () => {
		it('should return null when no artifactId', () => {
			const activity: ActivityItem = {
				id: '1',
				type: 'file_uploaded',
				fileName: 'test.txt',
				timestamp: new Date().toISOString()
			};
			expect(getActivityHref(activity)).toBeNull();
		});

		it('should return null when accessible is false', () => {
			const activity: ActivityItem = {
				id: '1',
				type: 'note_created',
				fileName: 'My Note',
				timestamp: new Date().toISOString(),
				artifactId: 'note-123',
				applicationId: 'notes',
				accessible: false
			};
			expect(getActivityHref(activity)).toBeNull();
		});

		it('should return correct href for notes module', () => {
			const activity: ActivityItem = {
				id: '1',
				type: 'note_created',
				fileName: 'My Note',
				timestamp: new Date().toISOString(),
				artifactId: 'note-123',
				applicationId: 'notes'
			};
			expect(getActivityHref(activity)).toBe('/apps/notes/note-123');
		});

		it('should return file preview fallback', () => {
			const activity: ActivityItem = {
				id: '1',
				type: 'file_uploaded',
				fileName: 'test.txt',
				timestamp: new Date().toISOString(),
				artifactId: 'file-123'
			};
			expect(getActivityHref(activity)).toBe('/files?preview=file-123');
		});
	});

	describe('getRelativeTime', () => {
		it('should show "Just now" for recent timestamps', () => {
			const now = new Date().toISOString();
			expect(getRelativeTime(now)).toBe('Just now');
		});

		it('should show minutes ago', () => {
			const fiveMinutesAgo = new Date(Date.now() - 5 * 60 * 1000).toISOString();
			expect(getRelativeTime(fiveMinutesAgo)).toBe('5m ago');
		});

		it('should show hours ago', () => {
			const threeHoursAgo = new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString();
			expect(getRelativeTime(threeHoursAgo)).toBe('3h ago');
		});

		it('should show days ago', () => {
			const twoDaysAgo = new Date(Date.now() - 2 * 24 * 60 * 60 * 1000).toISOString();
			expect(getRelativeTime(twoDaysAgo)).toBe('2d ago');
		});

		it('should show formatted date for old timestamps', () => {
			const tenDaysAgo = new Date(Date.now() - 10 * 24 * 60 * 60 * 1000).toISOString();
			const result = getRelativeTime(tenDaysAgo);

			expect(result).toMatch(/\w{3} \d{1,2}/); // e.g., "Mar 9"
		});

		it('should handle edge cases', () => {
			const thirtySecondsAgo = new Date(Date.now() - 30 * 1000).toISOString();
			expect(getRelativeTime(thirtySecondsAgo)).toBe('Just now');

			const sixtyMinutesAgo = new Date(Date.now() - 60 * 60 * 1000).toISOString();
			expect(getRelativeTime(sixtyMinutesAgo)).toBe('1h ago');
		});
	});

	describe('Activity Types Coverage', () => {
		it('should support all activity types', () => {
			const types: ActivityType[] = [
				'file_uploaded',
				'file_downloaded',
				'file_deleted',
				'file_renamed',
				'file_moved',
				'file_restored',
				'folder_created',
				'folder_deleted',
				'folder_renamed',
				'share_created',
				'share_revoked',
				'share_updated',
				'share_received',
				'share_permission_changed',
				'share_revoked_from_user',
				'note_created',
				'note_edited',
				'note_modified',
				'meeting_created',
				'meeting_note_modified',
				'standup_created',
				'standup_modified',
				'kanban_created',
				'kanban_modified',
				'decision_created',
				'decision_modified',
				'brainstorm_created',
				'brainstorm_board_modified'
			];

			types.forEach((type) => {
				activityStore.addActivity(type, 'test.txt');
			});

			const activities = get(activityStore);
			expect(activities).toHaveLength(types.length);
		});
	});
});
