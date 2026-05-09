import { getWebSocketClient, disconnectWebSocket } from './client';
import { queryClient } from '$lib/query-client';
import { replicationStore, type ReplicationStatus } from '$lib/stores/replication';
import { toastStore } from '$lib/stores/toast';
import { resolveNotificationTarget } from '$lib/utils/shared';
import { truncateFilename } from '$lib/utils/format';
import type {
	WebSocketEvent,
	FileUploadedPayload,
	FileModifiedPayload,
	FileRenamedPayload,
	FileMovedPayload,
	FileDeletedPayload,
	FileRestoredPayload,
	FolderCreatedPayload,
	FolderRenamedPayload,
	FolderMovedPayload,
	FolderDeletedPayload,
	ShareCreatedPayload,
	ShareRevokedPayload,
	ShareUpdatedPayload
} from './events';

const managerState = {
	currentUserId: null as string | null,
	eventHandlersRegistered: false
};

const MAX_TOAST_FILENAME_LENGTH = 12;

/**
 * Initialize WebSocket connection for the authenticated browser session.
 * Sets up all event handlers for real-time sync
 */
export async function initializeWebSocket(token: string | null, userId: string): Promise<void> {
	managerState.currentUserId = userId;
	const wsClient = getWebSocketClient();

	// Register event handlers only once
	if (!managerState.eventHandlersRegistered) {
		registerEventHandlers(wsClient);
		managerState.eventHandlersRegistered = true;
	}

	try {
		await wsClient.connect(token);
		console.log('[WebSocket Manager] Connected successfully');
	} catch (error) {
		console.error('[WebSocket Manager] Failed to connect:', error);
		throw error;
	}
}

/**
 * Disconnect WebSocket and cleanup
 */
export function cleanupWebSocket(): void {
	disconnectWebSocket();
	managerState.currentUserId = null;
	managerState.eventHandlersRegistered = false;
}

/**
 * Register all event handlers for real-time sync
 */
function registerEventHandlers(wsClient: ReturnType<typeof getWebSocketClient>): void {
	// File events
	wsClient.on('FileUploaded', handleFileUploaded);
	wsClient.on('FileModified', handleFileModified);
	wsClient.on('FileRenamed', handleFileRenamed);
	wsClient.on('FileMoved', handleFileMoved);
	wsClient.on('FileDeleted', handleFileDeleted);
	wsClient.on('FileRestored', handleFileRestored);

	// Folder events
	wsClient.on('FolderCreated', handleFolderCreated);
	wsClient.on('FolderRenamed', handleFolderRenamed);
	wsClient.on('FolderMoved', handleFolderMoved);
	wsClient.on('FolderDeleted', handleFolderDeleted);

	// Share events
	wsClient.on('ShareCreated', handleShareCreated);
	wsClient.on('ShareRevoked', handleShareRevoked);
	wsClient.on('ShareUpdated', handleShareUpdated);
	wsClient.on('ReplicationStateChanged', handleReplicationStateChanged);
	wsClient.on('NotificationCreated', handleNotificationCreated);
}

// Helper to check if event is from current user
function isOwnEvent(event: WebSocketEvent): boolean {
	return event.user_id === managerState.currentUserId;
}

function isOwnOrSystemEvent(event: WebSocketEvent): boolean {
	return !event.user_id || event.user_id === managerState.currentUserId;
}

// File Event Handlers
function handleFileUploaded(event: WebSocketEvent): void {
	const payload = event.payload as FileUploadedPayload | undefined;
	if (!payload) return;

	// Invalidate BOTH folder contents and file workspace queries
	queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });

	if (!isOwnEvent(event)) {
		const fileName = payload.file_name || 'New file';
		toastStore.show(`${truncateFilename(fileName, MAX_TOAST_FILENAME_LENGTH)} uploaded`, 'info');
	}
}

function handleFileModified(event: WebSocketEvent): void {
	const payload = event.payload as FileModifiedPayload | undefined;
	if (!payload) return;

	// Invalidate file details, folder contents and file workspace
	queryClient.invalidateQueries({ queryKey: ['file', payload.file_id] });
	queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });

	if (!isOwnEvent(event)) {
		const fileName = payload.file_name || 'File';
		toastStore.show(
			`${truncateFilename(fileName, MAX_TOAST_FILENAME_LENGTH)} modified (v${payload.version})`,
			'info'
		);
	}
}

function handleFileRenamed(event: WebSocketEvent): void {
	const payload = event.payload as FileRenamedPayload | undefined;
	if (!payload) return;

	// Invalidate file details, folder contents and file workspace
	queryClient.invalidateQueries({ queryKey: ['file', payload.file_id] });
	queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });

	if (!isOwnEvent(event)) {
		const oldName = payload.old_name || 'File';
		const newName = payload.new_name || 'File';
		toastStore.show(
			`${truncateFilename(oldName, MAX_TOAST_FILENAME_LENGTH)} renamed to ${truncateFilename(newName, MAX_TOAST_FILENAME_LENGTH)}`,
			'info'
		);
	}
}

function handleFileMoved(event: WebSocketEvent): void {
	const payload = event.payload as FileMovedPayload | undefined;
	if (!payload) return;

	// Invalidate both old and new folder contents
	queryClient.invalidateQueries({
		queryKey: ['folder-contents', payload.old_folder_id]
	});
	queryClient.invalidateQueries({
		queryKey: ['folder-contents', payload.new_folder_id]
	});
	queryClient.invalidateQueries({ queryKey: ['file', payload.file_id] });
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });

	// Invalidate root folders if needed
	if (!payload.old_folder_id || !payload.new_folder_id) {
		queryClient.invalidateQueries({ queryKey: ['folder-contents', null] });
		queryClient.invalidateQueries({ queryKey: ['file-workspace', 'all', null] });
	}

	if (!isOwnEvent(event)) {
		const fileName = payload.file_name || 'File';
		toastStore.show(`${truncateFilename(fileName, MAX_TOAST_FILENAME_LENGTH)} moved`, 'info');
	}
}

function handleFileDeleted(event: WebSocketEvent): void {
	const payload = event.payload as FileDeletedPayload | undefined;
	if (!payload) return;

	// Invalidate file details and folder contents
	queryClient.invalidateQueries({ queryKey: ['file', payload.file_id] });
	queryClient.invalidateQueries({
		queryKey: ['folder-contents', payload.folder_id]
	});
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
	replicationStore.remove(payload.file_id);

	// Invalidate root if folder_id is null
	if (!payload.folder_id) {
		queryClient.invalidateQueries({ queryKey: ['folder-contents', null] });
		queryClient.invalidateQueries({ queryKey: ['file-workspace', 'all', null] });
	}

	if (!isOwnEvent(event)) {
		const fileName = payload.file_name || 'File';
		toastStore.show(`${truncateFilename(fileName, MAX_TOAST_FILENAME_LENGTH)} deleted`, 'info');
	}
}

function handleFileRestored(event: WebSocketEvent): void {
	const payload = event.payload as FileRestoredPayload | undefined;
	if (!payload) return;

	// Invalidate file details and folder contents
	queryClient.invalidateQueries({ queryKey: ['file', payload.file_id] });
	queryClient.invalidateQueries({
		queryKey: ['folder-contents', payload.folder_id]
	});
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });

	if (!payload.folder_id) {
		queryClient.invalidateQueries({ queryKey: ['folder-contents', null] });
		queryClient.invalidateQueries({ queryKey: ['file-workspace', 'all', null] });
	}

	if (!isOwnEvent(event)) {
		const fileName = payload.file_name || 'File';
		toastStore.show(`${truncateFilename(fileName, MAX_TOAST_FILENAME_LENGTH)} restored`, 'success');
	}
}

// Folder Event Handlers
function handleFolderCreated(event: WebSocketEvent): void {
	const payload = event.payload as FolderCreatedPayload | undefined;
	if (!payload) return;

	// Invalidate parent folder contents, folder tree and file workspace
	queryClient.invalidateQueries({
		queryKey: ['folder-contents', payload.parent_folder_id]
	});
	queryClient.invalidateQueries({ queryKey: ['folders'] });
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });

	if (!payload.parent_folder_id) {
		queryClient.invalidateQueries({ queryKey: ['folder-contents', null] });
		queryClient.invalidateQueries({ queryKey: ['file-workspace', 'all', null] });
	}

	if (!isOwnEvent(event)) {
		const folderName = payload.folder_name || 'New folder';
		toastStore.show(
			`Folder ${truncateFilename(folderName, MAX_TOAST_FILENAME_LENGTH)} created`,
			'info'
		);
	}
}

function handleFolderRenamed(event: WebSocketEvent): void {
	const payload = event.payload as FolderRenamedPayload | undefined;
	if (!payload) return;

	// Invalidate folder tree, all folder contents and file workspace
	queryClient.invalidateQueries({ queryKey: ['folders'] });
	queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });

	if (!isOwnEvent(event)) {
		const oldName = payload.old_name || 'Folder';
		const newName = payload.new_name || 'Folder';
		toastStore.show(
			`Folder ${truncateFilename(oldName, MAX_TOAST_FILENAME_LENGTH)} renamed to ${truncateFilename(newName, MAX_TOAST_FILENAME_LENGTH)}`,
			'info'
		);
	}
}

function handleFolderMoved(event: WebSocketEvent): void {
	const payload = event.payload as FolderMovedPayload | undefined;
	if (!payload) return;

	// Invalidate both old and new parent folder contents and folder tree
	queryClient.invalidateQueries({
		queryKey: ['folder-contents', payload.old_parent_id]
	});
	queryClient.invalidateQueries({
		queryKey: ['folder-contents', payload.new_parent_id]
	});
	queryClient.invalidateQueries({ queryKey: ['folders'] });
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });

	if (!payload.old_parent_id || !payload.new_parent_id) {
		queryClient.invalidateQueries({ queryKey: ['folder-contents', null] });
		queryClient.invalidateQueries({ queryKey: ['file-workspace', 'all', null] });
	}

	if (!isOwnEvent(event)) {
		const folderName = payload.folder_name || 'Folder';
		toastStore.show(
			`Folder ${truncateFilename(folderName, MAX_TOAST_FILENAME_LENGTH)} moved`,
			'info'
		);
	}
}

function handleFolderDeleted(event: WebSocketEvent): void {
	const payload = event.payload as FolderDeletedPayload | undefined;
	if (!payload) return;

	// Invalidate parent folder contents, folder tree and file workspace
	queryClient.invalidateQueries({
		queryKey: ['folder-contents', payload.parent_folder_id]
	});
	queryClient.invalidateQueries({ queryKey: ['folders'] });
	queryClient.invalidateQueries({ queryKey: ['file-workspace'] });

	if (!payload.parent_folder_id) {
		queryClient.invalidateQueries({ queryKey: ['folder-contents', null] });
		queryClient.invalidateQueries({ queryKey: ['file-workspace', 'all', null] });
	}

	if (!isOwnEvent(event)) {
		const folderName = payload.folder_name || 'Folder';
		toastStore.show(
			`Folder ${truncateFilename(folderName, MAX_TOAST_FILENAME_LENGTH)} deleted`,
			'info'
		);
	}
}

// Share Event Handlers
function handleShareCreated(event: WebSocketEvent): void {
	const payload = event.payload as ShareCreatedPayload | undefined;
	if (!payload) return;

	// Invalidate shares list and file details
	queryClient.invalidateQueries({ queryKey: ['user-shares'] });
	queryClient.invalidateQueries({ queryKey: ['file', payload.file_id] });

	if (!isOwnEvent(event)) {
		const fileName = payload.file_name || 'File';
		toastStore.show(
			`Share created for ${truncateFilename(fileName, MAX_TOAST_FILENAME_LENGTH)}`,
			'info'
		);
	}
}

function handleShareRevoked(event: WebSocketEvent): void {
	const payload = event.payload as ShareRevokedPayload | undefined;
	if (!payload) return;

	// Invalidate shares list and file details
	queryClient.invalidateQueries({ queryKey: ['user-shares'] });
	queryClient.invalidateQueries({ queryKey: ['file', payload.file_id] });

	if (!isOwnEvent(event)) {
		toastStore.show('Share was revoked', 'info');
	}
}

function handleShareUpdated(event: WebSocketEvent): void {
	const payload = event.payload as ShareUpdatedPayload | undefined;
	if (!payload) return;

	// Invalidate shares list and file details
	queryClient.invalidateQueries({ queryKey: ['user-shares'] });
	queryClient.invalidateQueries({ queryKey: ['file', payload.file_id] });

	if (!isOwnEvent(event)) {
		toastStore.show('Share was updated', 'info');
	}
}

function handleReplicationStateChanged(event: WebSocketEvent): void {
	const fileId = event.file_id;
	const fileVersionId = event.file_version_id;
	const replicationState = event.replication_state;

	if (!fileId || !fileVersionId || !replicationState || !event.updated_at) {
		console.warn('[WebSocket Manager] ReplicationStateChanged missing required fields', event);
		return;
	}

	const status: ReplicationStatus = {
		fileId,
		fileVersionId,
		replicationState,
		jobStatus: event.job_status ?? null,
		attemptCount: event.attempt_count ?? 0,
		nextAttemptAt: event.next_attempt_at ?? null,
		lastError: event.last_error ?? null,
		updatedAt: event.updated_at
	};

	replicationStore.upsert(status);
	queryClient.invalidateQueries({ queryKey: ['file', fileId] });

	if (!isOwnOrSystemEvent(event)) {
		return;
	}

	if (replicationState === 'fully_replicated') {
		toastStore.show('File replication completed', 'success');
	} else if (replicationState === 'degraded') {
		toastStore.show('Replication delayed, retrying in background', 'info');
	} else if (replicationState === 'failed') {
		toastStore.show('Replication failed for a file version', 'error');
	}
}

function handleNotificationCreated(event: WebSocketEvent): void {
	queryClient.invalidateQueries({ queryKey: ['notifications'] });
	queryClient.invalidateQueries({ queryKey: ['received-shares'] });

	if (!isOwnOrSystemEvent(event)) {
		return;
	}

	const message = event.message;
	const notificationType = event.notification_type;
	const target =
		event.resource_id && event.resource_type
			? resolveNotificationTarget({
					id: event.notification_id || 'notification',
					notification_type: notificationType || 'notification',
					title: event.title || 'Notification',
					message: message || 'A new notification arrived',
					resource_id: event.resource_id,
					resource_type: event.resource_type,
					action_url: event.action_url ?? null,
					read: false,
					created_at: event.timestamp || new Date().toISOString()
				})
			: null;

	if (notificationType === 'share_revoked') {
		toastStore.show(
			message || 'Access to a shared resource was revoked',
			'info',
			target
				? {
						actionLabel: 'View',
						actionHref: target
					}
				: undefined
		);
		return;
	}

	if (notificationType === 'permission_changed') {
		toastStore.show(
			message || 'A shared resource permission changed',
			'info',
			target
				? {
						actionLabel: 'Open',
						actionHref: target
					}
				: undefined
		);
		return;
	}

	toastStore.show(
		message || 'A new share notification arrived',
		'success',
		target
			? {
					actionLabel: 'Open',
					actionHref: target
				}
			: undefined
	);
}
