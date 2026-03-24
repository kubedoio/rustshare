import { apiClient } from './client';
import type { Theme } from '$lib/stores/theme';

export interface UserProfile {
	id: string;
	username: string;
	display_name: string;
	email: string;
	is_admin: boolean;
	storage_quota: number;
	theme: Theme;
	created_at: string;
	updated_at: string;
}

export interface UpdateThemeResponse {
	theme: Theme;
}

export interface UpdatePasswordRequest {
	current_password: string;
	new_password: string;
	confirm_password: string;
}

export interface UpdatePasswordResponse {
	message: string;
}

export interface UserSession {
	id: string;
	created_at: string;
	last_seen_at: string;
	expires_at: string;
	user_agent: string | null;
	ip_address: string | null;
	is_current: boolean;
}

export interface UserDevice {
	id: string;
	device_name: string;
	created_at: string;
	last_used_at: string | null;
}

export interface UserSecurityEvent {
	id: string;
	event_type: string;
	description: string;
	ip_address: string | null;
	user_agent: string | null;
	session_id: string | null;
	occurred_at: string;
}

/**
 * Get the current user's profile.
 */
export async function getUserProfile(): Promise<UserProfile> {
	const response = await apiClient.get<UserProfile>('/me');
	return response;
}

/**
 * Update the current user's theme preference.
 */
export async function updateUserTheme(theme: Theme): Promise<UpdateThemeResponse> {
	const response = await apiClient.patch<UpdateThemeResponse>('/me/theme', {
		theme
	});
	return response;
}

/**
 * Update the current user's password.
 */
export async function updateUserPassword(
	request: UpdatePasswordRequest
): Promise<UpdatePasswordResponse> {
	return apiClient.patch<UpdatePasswordResponse>('/me/password', request);
}

/**
 * List active browser sessions for the current user.
 */
export async function listUserSessions(): Promise<UserSession[]> {
	return apiClient.get<UserSession[]>('/me/sessions');
}

/**
 * Revoke a specific browser session.
 */
export async function revokeUserSession(sessionId: string): Promise<void> {
	return apiClient.delete<void>(`/me/sessions/${sessionId}`);
}

/**
 * List recent security activity for the current user.
 */
export async function listUserSecurityEvents(): Promise<UserSecurityEvent[]> {
	return apiClient.get<UserSecurityEvent[]>('/me/security-events');
}

/**
 * List active devices for the current user.
 */
export async function listUserDevices(): Promise<UserDevice[]> {
	return apiClient.get<UserDevice[]>('/user/devices');
}

/**
 * Revoke a specific device token.
 */
export async function revokeUserDevice(deviceId: string): Promise<void> {
	return apiClient.delete<void>(`/user/devices/${deviceId}`);
}
