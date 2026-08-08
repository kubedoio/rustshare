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
	color?: string | null;
	// Share indicators
	is_shared?: boolean;
	share_count?: number;
	/// Earliest share expiration date (ISO 8601 format), null if shares don't expire
	share_expires_at?: string | null;
	effective_permission?: 'View' | 'Edit' | 'Admin' | null;
	// Vault sync metadata (optional, only present for vault files)
	vault_id?: string;
	vault_name?: string;
	adapter?: 'ObsidianVault';
	source_type?: 'external_vault';
	server_rev?: number;
	last_synced_at?: string;
	last_writer_device_id?: string;
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
			return 'Group';
		case 'user':
			return 'Specific people';
		case 'public':
			return 'Link';
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

export interface NoteAttachment {
	file_id: string;
	name: string;
	mime_type: string;
	size: number;
	created_at: string;
}

export interface NoteConflict {
	kind: string;
	message: string;
	yaml_title?: string | null;
	folder_name?: string | null;
	manifest_title?: string | null;
	yaml_id?: string | null;
	sidecar_id?: string | null;
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
	attachments?: NoteAttachment[];
	okf_id?: string | null;
	acl_hash?: string | null;
	acl_version?: number | null;
	conflict?: NoteConflict | null;
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
	okf_id?: string | null;
	conflict?: NoteConflict | null;
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
	attachment_count?: number;
	drawing_count?: number;
	export_count?: number;
	okf_id?: string | null;
	conflict?: NoteConflict | null;
}

export class ApiError extends Error {
	constructor(
		public status: number,
		public message: string,
		public details?: Record<string, unknown>
	) {
		super(message);
		this.name = 'ApiError';
	}

	/** Structured conflict fields (present on 409 responses from the vault-sync API). */
	get client_rev(): number | undefined {
		return typeof this.details?.client_rev === 'number' ? this.details.client_rev : undefined;
	}

	get current_rev(): number | undefined {
		return typeof this.details?.current_rev === 'number' ? this.details.current_rev : undefined;
	}

	get server_sha256(): string | undefined {
		return typeof this.details?.server_sha256 === 'string' ? this.details.server_sha256 : undefined;
	}

	get resolution(): string | undefined {
		return typeof this.details?.resolution === 'string' ? this.details.resolution : undefined;
	}
}

// ---------------------------------------------------------------------------
// Application & Template Types
// ---------------------------------------------------------------------------

export interface ApplicationContribution {
	id: string;
	label?: string;
	icon?: string;
	route?: string;
	renderer?: string;
	action?: string;
	template?: string;
	order?: number;
}

export interface ApplicationManifest {
	apiVersion: string;
	kind: 'Application';
	metadata: {
		id: string;
		name: string;
		version: string;
		description: string;
	};
	runtime: { kind: 'embedded' | 'service' | 'bridge' };
	contracts: {
		provides: Array<{ id: string; version: string }>;
		requires: Array<{ id: string; version: string }>;
	};
	resources: Array<{ type: string; actions: string[] }>;
	contributions: {
		navigation: ApplicationContribution[];
		routes: ApplicationContribution[];
		commands: ApplicationContribution[];
		dashboard: ApplicationContribution[];
		settings: ApplicationContribution[];
		searchProviders: ApplicationContribution[];
		renderers: ApplicationContribution[];
		admin: ApplicationContribution[];
	};
	integrationEvents: { publishes: string[]; subscribes: string[] };
	memory?: { sourceTypes: string[]; publication: string };
	configuration: { schema: string };
	data: { owner: string; preserveOnDisable: boolean; exportSupported: boolean };
	health?: { liveness: string; readiness: string };
}

export interface ApplicationShellEntry {
	manifest: ApplicationManifest;
	enabled: boolean;
	configuration: Record<string, unknown>;
	health: 'healthy' | 'degraded' | 'unavailable';
}

export interface ApplicationConfig {
	id: string;
	application_id: string;
	display_name: string;
	description: string;
	enabled: boolean;
	root_path: string;
	renderer: string;
	default_template: string | null;
	icon: string;
	schema_version: string;
	permissions: ApplicationPermissions;
	ai_indexing: AiIndexingPolicy;
	audit: AuditPolicy;
	ui_config?: ApplicationUiConfig;
	created_at: string;
	updated_at: string;
}

export interface OkfApplicationUiConfig {
	enabled: boolean;
	conceptType: string;
	frontmatterRequired: boolean;
	preserveUnknownFields?: boolean;
}

export interface ApplicationUiConfig {
	documentFormat?: string;
	okf?: OkfApplicationUiConfig;
	sidebar?: SidebarConfig;
	dashboard?: DashboardConfig;
	page?: ApplicationPageDefinition;
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

export interface ApplicationPageDefinition {
	enabled: boolean;
	route: string;
	renderer: string;
	layout: string;
	emptyStateTitle: string;
	emptyStateDescription: string;
	emptyStateAction: string;
	primaryAction?: PrimaryActionConfig;
	searchPlaceholder?: string;
	filterLabel?: string;
	sortLabel?: string;
	itemSingular?: string;
	itemPlural?: string;
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

export interface ApplicationPermissions {
	admin_can_configure: boolean;
	workspace_members_can_use: boolean;
	allow_public_share: boolean;
	allow_internal_share: boolean;
}

export interface AiIndexingPolicy {
	enabled: boolean;
	source?: string;
	permission_aware?: boolean;
}

export interface AuditPolicy {
	enabled: boolean;
}

export interface TemplateConfig {
	id: string;
	template_key: string;
	name: string;
	application_id: string;
	version: string;
	description: string;
	ui_config?: TemplateUiConfig;
	folder_structure: string[];
	default_files: TemplateDefaultFile[];
	metadata_schema: Record<string, unknown>;
	renderer: string | null;
	visibility_policy: string;
	application_config?: Record<string, unknown>;
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

export interface ApplicationSummary {
	application_id: string;
	mode: string;
	total_items: number;
	recent_items: SummaryItem[];
	extra?: Record<string, unknown> | null;
}

// ---------------------------------------------------------------------------
// Kanban Types
// ---------------------------------------------------------------------------

export interface KanbanLabel {
	id: string;
	name: string;
	color: 'green' | 'yellow' | 'orange' | 'red' | 'purple' | 'blue' | 'gray';
}

export interface KanbanAssignee {
	id: string;
	display_name: string;
	initials: string;
	avatar_url: string | null;
}

export interface KanbanChecklist {
	done: number;
	total: number;
}

export interface KanbanChecklistItem {
	id: string;
	text: string;
	done: boolean;
}

export interface KanbanChecklistGroup {
	id: string;
	title: string;
	items: KanbanChecklistItem[];
}

export interface KanbanSettings {
	show_description_on_cards: boolean;
	description_preview_lines: number;
	show_assignees: boolean;
	show_labels: boolean;
	show_due_date: boolean;
	show_attachment_badge: boolean;
	show_checklist_badge: boolean;
}

export interface KanbanBoardSummary {
	id: string;
	title: string;
	slug: string;
	path: string;
	column_count: number;
	card_count: number;
	created_at: string;
	updated_at: string;
	archived: boolean;
}

export interface KanbanBoard {
	id: string;
	title: string;
	slug: string;
	path: string;
	columns: KanbanColumn[];
	labels: KanbanLabel[];
	settings: KanbanSettings;
	created_at: string;
	updated_at: string;
	archived: boolean;
}

export interface KanbanColumn {
	id: string;
	title: string;
	slug: string;
	order: number;
	status: string;
	wip_limit: number | null;
	cards: KanbanCard[];
}

export interface KanbanCard {
	id: string;
	title: string;
	slug: string;
	content: string;
	description_preview: string;
	column_id: string;
	status: string;
	order: number;
	labels: KanbanLabel[];
	assignees: KanbanAssignee[];
	due_date: string | null;
	priority: 'low' | 'normal' | 'high' | 'urgent';
	attachments_count: number;
	checklist: KanbanChecklist;
	checklists: KanbanChecklistGroup[];
	archived: boolean;
	created_at: string;
	updated_at: string;
	path: string;
	schema_version: string;
}
export interface KanbanCardAttachment {
	id: string;
	name: string;
	size: number;
	mime_type: string;
	created_at: string;
	created_by: string;
}

export interface KanbanEvent {
	event_type: string;
	timestamp: string;
	actor: string;
	payload: unknown;
}

export interface KanbanCardDetail extends KanbanCard {
	attachments: KanbanCardAttachment[];
	activity: KanbanEvent[];
}

// ---------------------------------------------------------------------------
// Vault Sync Types
// ---------------------------------------------------------------------------

export type VaultWritePolicy = 'read_only' | 'web_editing_enabled' | 'sync_client_only';

export interface Vault {
	id: string;
	name: string;
	adapter: 'ObsidianVault';
	root_path?: string;
	write_policy: VaultWritePolicy;
	server_rev: number;
	created_at: string;
	updated_at: string;
}

export interface VaultFile {
	id: string;
	vault_id: string;
	relative_path: string;
	content_type?: string;
	sha256?: string;
	size?: number;
	server_rev: number;
	mtime_server: string;
	deleted: boolean;
	deleted_at?: string | null;
	last_writer_device_id?: string | null;
}

export interface VaultManifest {
	vault_id: string;
	adapter: 'ObsidianVault';
	server_rev: number;
	generated_at: string;
	files: VaultManifestEntry[];
}

export interface VaultManifestEntry {
	path: string;
	sha256?: string;
	size?: number;
	content_type?: string;
	server_rev: number;
	mtime_server: string;
	deleted: boolean;
	deleted_at?: string | null;
}

export interface VaultDevice {
	id: string;
	device_name: string;
	client_type: string;
	client_version?: string;
	last_sync_rev?: number;
	revoked_at?: string | null;
	created_at: string;
	last_seen_at: string;
}

export interface VaultFileContent {
	path: string;
	content: string;
	server_rev: number;
	content_type: string | null;
	size: number;
}

export interface SaveVaultFileContentRequest {
	content: string;
	expected_revision: number;
}

export interface SaveVaultFileContentResponse {
	path: string;
	server_rev: number;
	updated_at: string;
}

export interface UpdateVaultWritePolicyRequest {
	write_policy: VaultWritePolicy;
}

export interface CreateVaultRequest {
	name: string;
	adapter: 'ObsidianVault';
	client_vault_id?: string;
	device_id: string;
}

export interface RenameVaultFileRequest {
	old_path: string;
	new_path: string;
	base_server_rev: number;
	device_id: string;
}
