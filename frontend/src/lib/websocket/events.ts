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
  | 'ShareUpdated';

export interface WebSocketEvent {
  event_id: string;
  type: WebSocketEventType;
  aggregate_id: string;
  user_id: string;
  timestamp: string;
  payload: FileUploadedPayload | FileModifiedPayload | FileRenamedPayload | FileMovedPayload | FileDeletedPayload | FileRestoredPayload | FolderCreatedPayload | FolderRenamedPayload | FolderMovedPayload | FolderDeletedPayload | ShareCreatedPayload | ShareRevokedPayload | ShareUpdatedPayload;
}

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

export type EventHandler = (event: WebSocketEvent) => void;
