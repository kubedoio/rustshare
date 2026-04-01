import { apiClient } from './client';

// Users
export const listAdminUsers = (params?: {
	search?: string;
	status?: string;
	page?: number;
	per_page?: number;
}) => apiClient.get<{ users: AdminUser[]; total: number }>('/admin/users' + buildQuery(params));

export const createAdminUser = (data: CreateUserRequest) =>
	apiClient.post<AdminUserDetail>('/admin/users', data);

export const getAdminUser = (id: string) =>
	apiClient.get<AdminUserDetail>(`/admin/users/${id}`);

export const updateAdminUser = (id: string, data: UpdateUserRequest) =>
	apiClient.patch<AdminUserDetail>(`/admin/users/${id}`, data);

export const disableAdminUser = (id: string) =>
	apiClient.post<void>(`/admin/users/${id}/disable`);

export const enableAdminUser = (id: string) =>
	apiClient.post<void>(`/admin/users/${id}/enable`);

export const deleteAdminUser = (id: string) =>
	apiClient.delete<void>(`/admin/users/${id}`);

// Groups
export const listAdminGroups = () =>
	apiClient.get<AdminGroup[]>('/admin/groups');

export const createAdminGroup = (data: { name: string; description?: string }) =>
	apiClient.post<AdminGroupDetail>('/admin/groups', data);

export const getAdminGroup = (id: string) =>
	apiClient.get<AdminGroupDetail>(`/admin/groups/${id}`);

export const updateAdminGroup = (id: string, data: { name?: string; description?: string }) =>
	apiClient.patch<AdminGroupDetail>(`/admin/groups/${id}`, data);

export const deleteAdminGroup = (id: string) =>
	apiClient.delete<void>(`/admin/groups/${id}`);

export const addGroupMember = (groupId: string, userId: string) =>
	apiClient.post<void>(`/admin/groups/${groupId}/members`, { user_id: userId });

export const removeGroupMember = (groupId: string, userId: string) =>
	apiClient.delete<void>(`/admin/groups/${groupId}/members/${userId}`);

// OIDC config
export const getOidcConfig = () => apiClient.get<OidcConfig>('/admin/config/oidc');
export const updateOidcConfig = (data: OidcConfigRequest) =>
	apiClient.put<OidcConfig>('/admin/config/oidc', data);
export const testOidcConfig = () =>
	apiClient.post<{ success: boolean; message?: string }>('/admin/config/oidc/test');

// SMTP config
export const getSmtpConfig = () => apiClient.get<SmtpConfig>('/admin/config/smtp');
export const updateSmtpConfig = (data: SmtpConfigRequest) =>
	apiClient.put<SmtpConfig>('/admin/config/smtp', data);
export const testSmtpConfig = () =>
	apiClient.post<{ success: boolean; message?: string }>('/admin/config/smtp/test');

// Webhooks
export const listWebhooks = () =>
	apiClient.get<{ webhooks: Webhook[] }>('/admin/integrations/webhooks');

export const createWebhook = (data: CreateWebhookRequest) =>
	apiClient.post<Webhook>('/admin/integrations/webhooks', data);

export const updateWebhook = (id: string, data: UpdateWebhookRequest) =>
	apiClient.patch<Webhook>(`/admin/integrations/webhooks/${id}`, data);

export const deleteWebhook = (id: string) =>
	apiClient.delete<void>(`/admin/integrations/webhooks/${id}`);

export const testWebhook = (id: string) =>
	apiClient.post<{ success: boolean; message?: string }>(
		`/admin/integrations/webhooks/${id}/test`
	);

// Audit
export const listAuditLog = (params?: {
	type?: string;
	user_id?: string;
	from?: string;
	to?: string;
	page?: number;
	per_page?: number;
}) =>
	apiClient.get<{ entries: AuditEntry[]; total: number }>(
		'/admin/audit' + buildQuery(params)
	);

function buildQuery(params?: Record<string, string | number | undefined>): string {
	if (!params) return '';
	const q = new URLSearchParams();
	for (const [k, v] of Object.entries(params)) {
		if (v !== undefined) q.set(k, String(v));
	}
	const s = q.toString();
	return s ? '?' + s : '';
}

// Types
export interface AdminUser {
	id: string;
	username: string;
	email: string;
	display_name: string;
	is_admin: boolean;
	storage_quota_bytes: number;
	disabled_at: string | null;
	created_at: string;
}

export interface AdminUserDetail extends AdminUser {
	storage_used_bytes: number;
}

export interface AdminGroup {
	id: string;
	name: string;
	description: string | null;
	member_count: number;
	created_at: string;
}

export interface AdminGroupDetail extends AdminGroup {
	members: GroupMember[];
}

export interface GroupMember {
	user_id: string;
	username: string;
	email: string;
	added_at: string;
}

export interface CreateUserRequest {
	username: string;
	email: string;
	password: string;
	display_name?: string;
	is_admin?: boolean;
	storage_quota_bytes?: number;
}

export interface UpdateUserRequest {
	display_name?: string;
	email?: string;
	storage_quota_bytes?: number;
	is_admin?: boolean;
}

export interface OidcConfig {
	enabled: boolean;
	provider_name?: string;
	client_id?: string;
	client_secret: string | null;
	issuer_url?: string;
	redirect_url?: string;
	login_label?: string;
	scopes?: string[];
	auto_provision_users: boolean;
	device_pair_code_ttl_seconds?: number | null;
}

export interface OidcConfigRequest {
	enabled?: boolean;
	provider_name?: string;
	client_id?: string;
	client_secret?: string;
	issuer_url?: string;
	redirect_url?: string;
	login_label?: string;
	scopes?: string[];
	auto_provision_users?: boolean;
	device_pair_code_ttl_seconds?: number;
}

export interface SmtpConfig {
	enabled: boolean;
	host?: string;
	port?: number;
	username?: string;
	password: string | null;
	from_address?: string;
	from_name?: string;
	tls_mode?: string;
}

export interface SmtpConfigRequest {
	enabled?: boolean;
	host?: string;
	port?: number;
	username?: string;
	password?: string;
	from_address?: string;
	from_name?: string;
	tls_mode?: string;
}

export interface Webhook {
	id: string;
	name: string;
	url: string;
	secret: string | null;
	enabled: boolean;
	events: string[];
	created_at: string;
}

export interface CreateWebhookRequest {
	name: string;
	url: string;
	secret?: string;
	events: string[];
}

export interface UpdateWebhookRequest {
	name?: string;
	url?: string;
	secret?: string;
	enabled?: boolean;
	events?: string[];
}

export interface AuditEntry {
	id: string;
	occurred_at: string;
	type: string;
	actor_label: string;
	action_type: string;
	target_label: string | null;
	detail: Record<string, unknown>;
}
