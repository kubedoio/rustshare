import type { Notification, ReceivedShare } from '$lib/api/types';

export function sharedResourcePath(
	resourceType: ReceivedShare['resource_type'],
	resourceId: string,
	options?: { folderId?: string | null }
): string {
	const basePath = `/shared-with-me/${resourceType}/${resourceId}`;

	if (resourceType === 'folder' && options?.folderId && options.folderId !== resourceId) {
		const searchParams = new URLSearchParams({ folder: options.folderId });
		return `${basePath}?${searchParams.toString()}`;
	}

	return basePath;
}

export function resolveNotificationTarget(notification: Notification): string {
	if (notification.notification_type === 'share_revoked') {
		return '/shared-with-me';
	}

	if (notification.action_url?.startsWith('/shared-with-me/')) {
		return notification.action_url;
	}

	if (
		notification.resource_type === 'file' &&
		(notification.action_url?.startsWith('/files/') || !notification.action_url)
	) {
		return sharedResourcePath('file', notification.resource_id);
	}

	if (
		notification.resource_type === 'folder' &&
		(notification.action_url?.startsWith('/folders/') || !notification.action_url)
	) {
		return sharedResourcePath('folder', notification.resource_id);
	}

	if (notification.action_url?.startsWith('/') && !notification.action_url.startsWith('//')) {
		return notification.action_url;
	}

	return '/shared-with-me';
}
