export interface User {
	id: string;
	email: string;
	display_name: string;
	is_admin: boolean;
	avatar_path?: string | null;
	storage_quota?: number;
	storage_used?: number;
	created_at?: string;
	updated_at?: string;
}

export interface File {
	id: string;
	name: string;
	path: string;
	content_hash: string;
	storage_key?: string;
	size: number;
	mime_type: string;
	parent_folder_id: string | null;
	owner_id: string;
	current_version: number;
	created_at: string;
	modified_at: string;
	// Share indicators
	is_shared?: boolean;
	share_count?: number;
	/// Earliest share expiration date (ISO 8601 format), null if shares don't expire
	share_expires_at?: string | null;
}

export interface Folder {
	id: string;
	name: string;
	path: string;
	parent_folder_id: string | null;
	owner_id: string;
	created_at: string;
	updated_at: string;
	// Share indicators
	is_shared?: boolean;
	share_count?: number;
	/// Earliest share expiration date (ISO 8601 format), null if shares don't expire
	share_expires_at?: string | null;
}

export interface Share {
	id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	resource_name?: string;
	share_token: string;
	permissions: 'View' | 'Edit' | 'Admin';
	upload_only: boolean;
	password_protected: boolean;
	access_count: number;
	expires_at: string | null;
	created_at: string;
	created_by?: string;
}

export interface ShareAccessLogEntry {
	accessed_at: string;
	action: string;
	success: boolean;
	actor_type: string | null;
	actor_label: string | null;
	ip_address: string | null;
	user_agent: string | null;
	share_session_id: string | null;
	share_session_subject: string | null;
}

export interface ReceivedShare {
	share_id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	resource_name: string;
	resource_path: string;
	permission: 'View' | 'Edit' | 'Admin';
	shared_by: string;
	shared_by_name: string;
	shared_by_email: string;
	created_at: string;
}

export interface ShareRecipient {
	share_id: string;
	user_id: string;
	email: string;
	permission: 'View' | 'Edit' | 'Admin';
	added_at: string;
	added_by: string;
}

export interface FolderContents {
	folders: Folder[];
	files: File[];
}

export interface SharedFolderContents extends FolderContents {
	root_folder_id: string;
	current_folder_id: string;
	current_folder_name: string;
	path: string;
}

export interface Notification {
	id: string;
	notification_type: string;
	title: string;
	message: string;
	resource_id: string;
	resource_type: string;
	action_url: string | null;
	read: boolean;
	created_at: string;
}

export interface FileVersion {
	id: string;
	version_number: number;
	size: number;
	content_hash: string;
	created_at: string;
	created_by_user_id: string;
	change_description?: string;
}

export class ApiError extends Error {
	constructor(
		public status: number,
		public message: string
	) {
		super(message);
		this.name = 'ApiError';
	}
}
