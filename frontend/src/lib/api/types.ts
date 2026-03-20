export interface User {
	id: string;
	email: string;
	display_name: string;
	is_admin: boolean;
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
}

export interface Folder {
	id: string;
	name: string;
	path: string;
	parent_folder_id: string | null;
	owner_id: string;
	created_at: string;
	updated_at: string;
}

export interface Share {
	id: string;
	file_id: string;
	file_name?: string; // For shares list view
	share_token: string;
	permissions: 'View' | 'Edit' | 'Admin';
	password_protected: boolean;
	expires_at: string | null;
	created_at: string;
	created_by: string;
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

export interface FolderContents {
	folders: Folder[];
	files: File[];
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
