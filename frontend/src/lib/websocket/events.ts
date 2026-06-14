// WebSocket event type definitions
export type WebSocketEventType =
	| 'FileUploaded'
	| 'FileModified'
	| 'FileRenamed'
	| 'FileMoved'
	| 'FileDeleted'
	| 'FileRestored'
	| 'FolderCreated'
	| 'FolderRenamed'
	| 'FolderMoved'
	| 'FolderDeleted'
	| 'ShareCreated'
	| 'ShareRevoked'
	| 'ShareUpdated'
	| 'ReplicationStateChanged'
	| 'NotificationCreated'
	| 'BrainstormBoardModified'
	| 'MeetingNoteModified'
	| 'DecisionModified'
	| 'StandupModified'
	| 'KanbanModified'
	| 'NoteModified';

export interface WebSocketEvent {
	event_id?: string;
	event_type?: WebSocketEventType;
	type?: WebSocketEventType;
	aggregate_id?: string;
	aggregate_type?: string;
	user_id?: string;
	timestamp?: string;
	payload?:
		| FileUploadedPayload
		| FileModifiedPayload
		| FileRenamedPayload
		| FileMovedPayload
		| FileDeletedPayload
		| FileRestoredPayload
		| FolderCreatedPayload
		| FolderRenamedPayload
		| FolderMovedPayload
		| FolderDeletedPayload
		| ShareCreatedPayload
		| ShareRevokedPayload
		| ShareUpdatedPayload
		| BrainstormBoardModifiedPayload
		| MeetingNoteModifiedPayload
		| DecisionModifiedPayload
		| StandupModifiedPayload
		| KanbanModifiedPayload
		| NoteModifiedPayload;
	file_id?: string;
	file_version_id?: string;
	replication_state?: ReplicationStateValue;
	job_status?: string | null;
	attempt_count?: number;
	next_attempt_at?: string | null;
	last_error?: string | null;
	updated_at?: string;
	notification_id?: string;
	title?: string;
	notification_type?: string;
	message?: string;
	resource_id?: string;
	resource_type?: string;
	action_url?: string | null;
}

export type ReplicationStateValue =
	| 'primary_written'
	| 'queued'
	| 'syncing'
	| 'fully_replicated'
	| 'degraded'
	| 'failed';

// Event payload interfaces
export interface FileUploadedPayload {
	file_id: string;
	file_name: string;
	folder_id: string | null;
	size: number;
	mime_type: string;
}

export interface FileModifiedPayload {
	file_id: string;
	file_name: string;
	version: number;
}

export interface FileRenamedPayload {
	file_id: string;
	old_name: string;
	new_name: string;
}

export interface FileMovedPayload {
	file_id: string;
	file_name: string;
	old_folder_id: string | null;
	new_folder_id: string | null;
}

export interface FileDeletedPayload {
	file_id: string;
	file_name: string;
	folder_id: string | null;
}

export interface FileRestoredPayload {
	file_id: string;
	file_name: string;
	folder_id: string | null;
}

export interface FolderCreatedPayload {
	folder_id: string;
	folder_name: string;
	parent_folder_id: string | null;
}

export interface FolderRenamedPayload {
	folder_id: string;
	old_name: string;
	new_name: string;
}

export interface FolderMovedPayload {
	folder_id: string;
	folder_name: string;
	old_parent_id: string | null;
	new_parent_id: string | null;
}

export interface FolderDeletedPayload {
	folder_id: string;
	folder_name: string;
	parent_folder_id: string | null;
}

export interface ShareCreatedPayload {
	share_id: string;
	file_id: string;
	file_name: string;
	permissions: string;
}

export interface ShareRevokedPayload {
	share_id: string;
	file_id: string;
}

export interface ShareUpdatedPayload {
	share_id: string;
	file_id: string;
	permissions: string;
}

export interface BrainstormBoardModifiedPayload {
	board_id: string;
	title: string;
}

export interface MeetingNoteModifiedPayload {
	meeting_id: string;
	title: string;
}

export interface DecisionModifiedPayload {
	decision_id: string;
	title: string;
}

export interface StandupModifiedPayload {
	standup_id: string;
	title: string;
}

export interface KanbanModifiedPayload {
	board_id: string | null;
	card_id: string | null;
}

export interface NoteModifiedPayload {
	note_id: string;
	title: string;
}

export type EventHandler = (event: WebSocketEvent) => void;
