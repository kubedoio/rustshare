import { apiClient } from './client';

export interface Group {
	id: string;
	name: string;
	description?: string;
	created_at: string;
	member_count: number;
}

export interface GroupMember {
	user_id: string;
	username: string;
	email: string;
	added_at: string;
}

export interface GroupDetail extends Group {
	members: GroupMember[];
}

export interface CreateGroupShareRequest {
	group_id: string;
	permission: 'View' | 'Edit' | 'Admin';
}

export interface GroupShareResponse {
	share_id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	group_id: string;
	group_name: string;
	permission: string;
	created_at: string;
}

/**
 * List all groups the current user is a member of.
 * GET /api/v1/groups/my
 */
export async function listMyGroups(): Promise<Group[]> {
	return apiClient.get<Group[]>('/groups/my');
}

/**
 * Get details of a specific group the user is a member of.
 * GET /api/v1/groups/my/:id
 */
export async function getMyGroup(groupId: string): Promise<GroupDetail> {
	return apiClient.get<GroupDetail>(`/groups/my/${groupId}`);
}

/**
 * Share a file with a group.
 * POST /api/v1/files/:id/share/group
 */
export async function createFileGroupShare(
	fileId: string,
	request: CreateGroupShareRequest
): Promise<GroupShareResponse> {
	return apiClient.post<GroupShareResponse>(`/files/${fileId}/share/group`, request);
}

/**
 * Share a folder with a group.
 * POST /api/v1/folders/:id/share/group
 */
export async function createFolderGroupShare(
	folderId: string,
	request: CreateGroupShareRequest
): Promise<GroupShareResponse> {
	return apiClient.post<GroupShareResponse>(`/folders/${folderId}/share/group`, request);
}

/**
 * List all group shares for a file.
 * GET /api/v1/files/:id/share/groups
 */
export async function listFileGroupShares(fileId: string): Promise<GroupShareResponse[]> {
	return apiClient.get<GroupShareResponse[]>(`/files/${fileId}/share/groups`);
}

/**
 * List all group shares for a folder.
 * GET /api/v1/folders/:id/share/groups
 */
export async function listFolderGroupShares(folderId: string): Promise<GroupShareResponse[]> {
	return apiClient.get<GroupShareResponse[]>(`/folders/${folderId}/share/groups`);
}
