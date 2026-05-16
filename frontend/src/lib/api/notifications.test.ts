import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	deleteNotification,
	getUnreadNotificationCount,
	listNotifications,
	markNotificationRead
} from '$lib/api/notifications';

vi.mock('$lib/api/client', () => ({
	apiClient: {
			postVoid: vi.fn(),
			patchVoid: vi.fn(),
			requestText: vi.fn(),
			requestVoid: vi.fn(),
		get: vi.fn(),
		put: vi.fn(),
		delete: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('notifications API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('lists notifications with no filters', async () => {
		const response = {
			notifications: [],
			total: 0
		};

		vi.mocked(apiClient.get).mockResolvedValue(response);

		const result = await listNotifications();

		expect(apiClient.get).toHaveBeenCalledWith('/notifications');
		expect(result).toEqual(response);
	});

	it('lists notifications with unread filter and pagination', async () => {
		const response = {
			notifications: [],
			total: 0
		};

		vi.mocked(apiClient.get).mockResolvedValue(response);

		await listNotifications({
			limit: 25,
			offset: 50,
			unreadOnly: true
		});

		expect(apiClient.get).toHaveBeenCalledWith(
			'/notifications?limit=25&offset=50&unread_only=true'
		);
	});

	it('marks a notification as read', async () => {
		const notification = {
			id: 'notification-1',
			notification_type: 'share_received',
			title: 'Shared with you',
			message: 'A document was shared with you',
			resource_id: 'file-1',
			resource_type: 'file',
			action_url: '/files/file-1',
			read: true,
			created_at: '2026-03-20T10:00:00Z'
		};

		vi.mocked(apiClient.put).mockResolvedValue(notification);

		const result = await markNotificationRead('notification-1');

		expect(apiClient.put).toHaveBeenCalledWith('/notifications/notification-1/read');
		expect(result).toEqual(notification);
	});

	it('deletes a notification', async () => {
		vi.mocked(apiClient.delete).mockResolvedValue(undefined);

		await deleteNotification('notification-1');

		expect(apiClient.delete).toHaveBeenCalledWith('/notifications/notification-1');
	});

	it('gets unread notification count', async () => {
		const response = { count: 7 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		const result = await getUnreadNotificationCount();

		expect(apiClient.get).toHaveBeenCalledWith('/notifications/unread-count');
		expect(result).toEqual(response);
	});
});
