import { apiClient } from './client';

const root = '/organization-admin';

function query(params: Record<string, string | number | undefined>): string {
	const search = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value !== undefined && value !== '') search.set(key, String(value));
	}
	const encoded = search.toString();
	return encoded ? `?${encoded}` : '';
}

export interface OrganizationAdminCapability {
	allowed: true;
}

export interface OrgUser {
	id: string;
	username: string;
	email: string;
	display_name: string;
	disabled_at: string | null;
}

export interface Page<T> {
	users?: T[];
	groups?: T[];
	members?: T[];
	invitations?: T[];
	entries?: T[];
	total: number;
	page: number;
	per_page: number;
}

export interface OrgGroup {
	id: string;
	name: string;
	description: string | null;
	created_at: string;
	updated_at?: string;
	member_count: number;
}

export interface OrgMember {
	user_id: string;
	username: string;
	email: string;
	added_at: string;
}

export interface BulkInviteRow {
	row: number;
	outcome: string;
}

export interface OffboardRow {
	user_id: string;
	outcome: string;
	shares_revoked: number;
	group_memberships_removed: number;
}

export interface OrganizationInvitation {
	id: string;
	recipient_email: string;
	recipient_display_name: string | null;
	sender_name: string;
	status: 'pending' | 'sent' | 'failed' | 'expired' | 'accepted' | 'cancelled';
	created_at: string;
	expires_at: string;
	delivery_attempted_at: string | null;
	accepted_at: string | null;
	cancelled_at: string | null;
}

export interface OrganizationAuditEvent {
	id: string;
	actor_id: string | null;
	occurred_at: string;
	action_type: string;
	target_type: string | null;
	target_id: string | null;
	detail: Record<string, unknown>;
}

export const getOrganizationAdminCapability = () =>
	apiClient.get<OrganizationAdminCapability>(`${root}/capability`);

export const listOrganizationUsers = (params: {
	page?: number;
	per_page?: number;
	search?: string;
	status?: 'all' | 'active' | 'disabled';
}) => apiClient.get<Page<OrgUser>>(`${root}/users${query(params)}`);

export const bulkInviteOrganizationUsers = (
	users: Array<{
		email: string;
		display_name: string;
		group_ids: string[];
	}>,
	dry_run: boolean
) =>
	apiClient.post<{ dry_run: boolean; rows: BulkInviteRow[] }>(`${root}/users/bulk`, {
		users,
		dry_run
	});

export const offboardOrganizationUsers = (user_ids: string[], dry_run: boolean) =>
	apiClient.post<{ dry_run: boolean; rows: OffboardRow[] }>(`${root}/users/offboard`, {
		user_ids,
		dry_run
	});

export const listOrganizationGroups = (params: { page?: number; per_page?: number }) =>
	apiClient.get<Page<OrgGroup>>(`${root}/groups${query(params)}`);

export const getOrganizationGroup = (id: string, params: { page?: number; per_page?: number }) =>
	apiClient.get<{
		id: string;
		name: string;
		description: string | null;
		members: OrgMember[];
		total: number;
		page: number;
		per_page: number;
	}>(`${root}/groups/${encodeURIComponent(id)}${query(params)}`);

export const createOrganizationGroup = (name: string, description: string) =>
	apiClient.post<OrgGroup>(`${root}/groups`, { name, description: description || null });

export const updateOrganizationGroup = (id: string, name: string, description: string) =>
	apiClient.patch<OrgGroup>(`${root}/groups/${encodeURIComponent(id)}`, {
		name,
		description: description || null
	});

export const deleteOrganizationGroup = (id: string) =>
	apiClient.delete(`${root}/groups/${encodeURIComponent(id)}`);

export const listOrganizationGroupMembers = (
	id: string,
	params: { page?: number; per_page?: number }
) =>
	apiClient.get<Page<OrgMember>>(
		`${root}/groups/${encodeURIComponent(id)}/members${query(params)}`
	);

export const addOrganizationGroupMember = (id: string, user_id: string) =>
	apiClient.postVoid(`${root}/groups/${encodeURIComponent(id)}/members`, { user_id });

export const removeOrganizationGroupMember = (id: string, user_id: string) =>
	apiClient.delete(
		`${root}/groups/${encodeURIComponent(id)}/members/${encodeURIComponent(user_id)}`
	);

export const listOrganizationInvitations = (params: { page?: number; per_page?: number }) =>
	apiClient.get<Page<OrganizationInvitation>>(`${root}/invitations${query(params)}`);

export const resendOrganizationInvitation = (id: string, idempotencyKey: string) =>
	apiClient.request<{
		invitation_id: string;
		delivery_status: string;
		invitation_status: string;
		replayed: boolean;
	}>(`${root}/invitations/${encodeURIComponent(id)}/resend`, {
		method: 'POST',
		headers: { 'Idempotency-Key': idempotencyKey },
		body: '{}'
	});

export const cancelOrganizationInvitation = (id: string) =>
	apiClient.post<{
		invitation_id: string;
		status: string;
		changed: boolean;
		group_assignments_removed: number;
	}>(`${root}/invitations/${encodeURIComponent(id)}/cancel`, {});

export const listOrganizationAudit = (params: { page?: number; per_page?: number }) =>
	apiClient.get<Page<OrganizationAuditEvent>>(`${root}/audit${query(params)}`);
