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
	storage_key?: string;
	size: number;
	mime_type: string;
	parent_folder_id: string | null;
	owner_id: string;
	current_version: number;
	created_at: string;
	modified_at: string;
	starred_at?: string | null;
	deleted_at?: string | null;
	// Share indicators
	is_shared?: boolean;
	share_count?: number;
	/// Earliest share expiration date (ISO 8601 format), null if shares don't expire
	share_expires_at?: string | null;
	effective_permission?: 'View' | 'Edit' | 'Admin' | null;
}

export interface Folder {
	id: string;
	name: string;
	path: string;
	parent_folder_id: string | null;
	owner_id: string;
	created_at: string;
	updated_at: string;
	starred_at?: string | null;
	deleted_at?: string | null;
	/// Total recursive size of all files within this folder (in bytes)
	size?: number;
	// Share indicators
	is_shared?: boolean;
	share_count?: number;
	/// Earliest share expiration date (ISO 8601 format), null if shares don't expire
	share_expires_at?: string | null;
	effective_permission?: 'View' | 'Edit' | 'Admin' | null;
}

export interface Share {
	id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	resource_name?: string;
	share_token: string | null; // null for user/group shares
	permissions: 'View' | 'Edit' | 'Admin';
	upload_only: boolean;
	password_protected: boolean;
	access_count: number;
	expires_at: string | null;
	created_at: string;
	created_by?: string;
	// Share type indicators
	recipient_user_id?: string | null;
	recipient_group_id?: string | null;
}

// Helper type for share classification
export type ShareType = 'public' | 'user' | 'group';

/**
 * Get the type of share based on recipient fields
 */
export function getShareType(share: Share): ShareType {
	if (share.recipient_group_id) return 'group';
	if (share.recipient_user_id) return 'user';
	return 'public';
}

/**
 * Get a human-readable label for the share type
 */
export function getShareTypeLabel(share: Share): string {
	const type = getShareType(share);
	switch (type) {
		case 'group':
			return 'Group Share';
		case 'user':
			return 'Shared with User';
		case 'public':
			return 'Public Link';
	}
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
	current_folder_permission?: 'View' | 'Edit' | 'Admin' | null;
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
	created_at: string;
	created_by_user_id: string;
	change_description?: string;
}

export interface NoteMetadata {
	kind: 'note';
	title: string;
	visibility: 'private' | 'public';
	public_share_id: string | null;
	created_at: string;
	updated_at: string;
	excerpt: string;
	mime_type: string;
	extension: string;
	pinned?: boolean;
	icon?: string | null;
	color?: string | null;
}

export interface Note {
	id: string;
	name: string;
	path: string;
	content: string;
	metadata: NoteMetadata;
	parent_folder_id: string | null;
	owner_id: string;
	current_version: number;
	created_at: string;
	modified_at: string;
}

export interface NoteSummary {
	id: string;
	name: string;
	path: string;
	metadata: NoteMetadata;
	parent_folder_id: string | null;
	owner_id: string;
	current_version: number;
	size: number;
	created_at: string;
	modified_at: string;
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

// ---------------------------------------------------------------------------
// Module & Template Types
// ---------------------------------------------------------------------------

export interface ModuleConfig {
	id: string;
	module_key: string;
	display_name: string;
	description: string;
	enabled: boolean;
	root_path: string;
	renderer: string;
	default_template: string | null;
	icon: string;
	schema_version: string;
	permissions: ModulePermissions;
	ai_indexing: AiIndexingPolicy;
	audit: AuditPolicy;
	ui_config?: ModuleUiConfig;
	created_at: string;
	updated_at: string;
}

export interface ModuleUiConfig {
	sidebar?: SidebarConfig;
	dashboard?: DashboardConfig;
	modulePage?: ModulePageConfig;
	page?: ModulePageDefinition;
}

export interface SidebarConfig {
	enabled: boolean;
	order: number;
	icon: string;
	label: string;
}

export interface DashboardConfig {
	enabled: boolean;
	order: number;
	cardTitle?: string;
	cardDescription?: string;
	summaryMode?: string;
	maxItems?: number;
	primaryAction?: PrimaryActionConfig;
	widget?: WorkspaceWidgetConfig;
}

export interface PrimaryActionConfig {
	label: string;
	action: string;
	template?: string;
}

export interface ModulePageConfig {
	layout: string;
	emptyStateTitle: string;
	emptyStateDescription: string;
	emptyStateAction: string;
}

export interface ModulePageDefinition {
	enabled: boolean;
	route: string;
	renderer: string;
	layout: string;
	emptyStateTitle: string;
	emptyStateDescription: string;
	emptyStateAction: string;
	primaryAction?: PrimaryActionConfig;
}

export type WorkspaceWidgetSize = 'small' | 'medium' | 'large';

export interface WorkspaceWidgetColumns {
	desktop: number;
	tablet: number;
	mobile: number;
}

export interface WorkspaceWidgetConfig {
	enabled: boolean;
	type: string;
	title: string;
	description: string;
	size: WorkspaceWidgetSize;
	columns: WorkspaceWidgetColumns;
	maxItems: number;
	primaryAction?: PrimaryActionConfig;
}

export interface WorkspaceSurfaceLayout {
	type: string;
	columns: number;
	gap: number;
	compactOverview: boolean;
}

export interface WorkspaceSurfaceSection {
	key: string;
	type: string;
	enabled: boolean;
	order: number;
	title?: string;
	renderer: string;
}

export interface WorkspaceSurfaceDefinition {
	id: string;
	key: string;
	name: string;
	version: string;
	enabled: boolean;
	layout: WorkspaceSurfaceLayout;
	sections: WorkspaceSurfaceSection[];
}

export interface ModulePermissions {
	admin_can_configure: boolean;
	workspace_members_can_use: boolean;
	allow_public_share: boolean;
	allow_internal_share: boolean;
}

export interface AiIndexingPolicy {
	enabled: boolean;
}

export interface AuditPolicy {
	enabled: boolean;
}

export interface TemplateConfig {
	id: string;
	template_key: string;
	name: string;
	module_key: string;
	version: string;
	description: string;
	ui_config?: TemplateUiConfig;
	folder_structure: string[];
	default_files: TemplateDefaultFile[];
	metadata_schema: Record<string, unknown>;
	renderer: string | null;
	visibility_policy: string;
	enabled: boolean;
	system_template: boolean;
	created_by: string | null;
	created_at: string;
	updated_at: string;
}

export interface TemplateDefaultFile {
	path: string;
	content?: string;
	content_type?: string;
}

export interface TemplateUiConfig {
	createLabel?: string;
	icon?: string;
	form?: Record<string, unknown>;
}

export interface CreateFromTemplateRequest {
	template_key: string;
	name: string;
	parent_folder_id?: string | null;
}

export interface CreateFromTemplateResponse {
	object_id: string;
	object_type: 'file' | 'folder';
	path: string;
}

export interface SummaryItem {
	id: string;
	name: string;
	item_type: 'file' | 'folder';
	updated_at: string;
}

export interface ModuleSummary {
	module_key: string;
	mode: string;
	total_items: number;
	recent_items: SummaryItem[];
	extra?: Record<string, unknown> | null;
}

// ---------------------------------------------------------------------------
// Kanban Types
// ---------------------------------------------------------------------------

export interface KanbanBoardSummary {
	id: string;
	title: string;
	slug: string;
	path: string;
	column_count: number;
	card_count: number;
	created_at: string;
	updated_at: string;
}

export interface KanbanBoard {
	id: string;
	title: string;
	slug: string;
	path: string;
	columns: KanbanColumn[];
	created_at: string;
	updated_at: string;
}

export interface KanbanColumn {
	id: string;
	title: string;
	slug: string;
	order: number;
	status: string;
	cards: KanbanCard[];
}

export interface KanbanCard {
	id: string;
	title: string;
	slug: string;
	content: string;
	column_id: string;
	status: string;
	order: number;
	assignees: string[];
	tags: string[];
	priority: string;
	archived: boolean;
	created_at: string;
	updated_at: string;
}
