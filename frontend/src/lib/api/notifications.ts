import { apiClient } from '$lib/api/client';
import type { Notification } from '$lib/api/types';

export interface ListNotificationsParams {
  limit?: number;
  offset?: number;
  unreadOnly?: boolean;
}

export interface ListNotificationsResponse {
  notifications: Notification[];
  total: number;
}

export interface UnreadNotificationCountResponse {
  count: number;
}

export async function listNotifications(
  params: ListNotificationsParams = {}
): Promise<ListNotificationsResponse> {
  const searchParams = new URLSearchParams();

  if (params.limit !== undefined) {
    searchParams.set('limit', String(params.limit));
  }

  if (params.offset !== undefined) {
    searchParams.set('offset', String(params.offset));
  }

  if (params.unreadOnly) {
    searchParams.set('unread_only', 'true');
  }

  const query = searchParams.toString();
  const endpoint = query ? `/notifications?${query}` : '/notifications';

  return apiClient.get<ListNotificationsResponse>(endpoint);
}

export async function markNotificationRead(notificationId: string): Promise<Notification> {
  return apiClient.put<Notification>(`/notifications/${notificationId}/read`);
}

export async function deleteNotification(notificationId: string): Promise<void> {
  return apiClient.delete<void>(`/notifications/${notificationId}`);
}

export async function getUnreadNotificationCount(): Promise<UnreadNotificationCountResponse> {
  return apiClient.get<UnreadNotificationCountResponse>('/notifications/unread-count');
}
