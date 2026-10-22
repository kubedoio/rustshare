import { beforeEach, describe, expect, it, vi } from 'vitest';
import { meetingsApi } from '$lib/api/meetings';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn(),
		post: vi.fn(),
		put: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('meetings API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('returns a single page when a limit is provided', async () => {
		const page = Array.from({ length: 50 }, (_, index) => ({
			id: `meeting-${index}`,
			name: `meeting-${index}.md`
		}));
		vi.mocked(apiClient.get).mockResolvedValueOnce(page);

		await expect(meetingsApi.list(50)).resolves.toHaveLength(50);
		expect(apiClient.get).toHaveBeenCalledTimes(1);
		expect(apiClient.get).toHaveBeenCalledWith('/meetings?per_page=50');
	});

	it('uses bounded action and tenant assignee endpoints and preserves mutation identity', async () => {
		await meetingsApi.listActions('meeting-1', 2, 25);
		await meetingsApi.listActionAssignees('meeting-1', 3, 25);
		expect(apiClient.get).toHaveBeenCalledWith('/meetings/meeting-1/actions?page=2&per_page=25');
		expect(apiClient.get).toHaveBeenCalledWith(
			'/meetings/meeting-1/action-assignees?page=3&per_page=25'
		);
		const create = {
			request_id: 'request-1',
			title: 'Action',
			description: null,
			assignee_id: null,
			due_date: '2026-11-01',
			related_decision_id: null
		};
		await meetingsApi.createAction('meeting-1', create);
		expect(apiClient.post).toHaveBeenCalledWith('/meetings/meeting-1/actions', create);
		const update = {
			request_id: 'request-2',
			title: 'Action',
			description: null,
			assignee_id: null,
			due_date: null,
			expected_version: 1,
			state: 'completed' as const
		};
		await meetingsApi.updateAction('meeting-1', 'action-1', update);
		expect(apiClient.put).toHaveBeenCalledWith('/meetings/meeting-1/actions/action-1', update);
		expect(update).not.toHaveProperty('related_decision_id');
	});

	it('fetches all pages when no limit is provided', async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) => ({
			id: `meeting-${index}`,
			name: `meeting-${index}.md`
		}));
		const secondPage = [{ id: 'meeting-100', name: 'meeting-100.md' }];

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(meetingsApi.list()).resolves.toHaveLength(101);
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/meetings?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/meetings?page=2&per_page=100');
	});
});
