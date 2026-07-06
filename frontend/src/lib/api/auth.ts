import { apiClient } from './client';
import type { User } from './types';

interface LoginRequest {
	email: string;
	password: string;
}

interface LoginResponse {
	token?: string;
	user: User;
}

export interface AuthConfig {
	password_login_enabled: boolean;
	oidc_enabled: boolean;
	oidc_login_label?: string | null;
	oidc_mobile_enabled: boolean;
}

export interface DeviceRequestResponse {
	user_code: string;
	device_code: string;
	expires_in: number;
	verification_uri: string;
	verification_uri_complete: string;
}

export type DevicePollResponse =
	{ status: 'pending' } | { status: 'approved'; token: string } | { status: 'expired' };

export async function login(email: string, password: string): Promise<LoginResponse> {
	return apiClient.post<LoginResponse>('/auth/login', { email, password });
}

export async function getAuthConfig(): Promise<AuthConfig> {
	return apiClient.get<AuthConfig>('/auth/config');
}

export function beginOidcLogin(redirectTo = '/files'): void {
	if (typeof window === 'undefined') {
		return;
	}

	const target = new URL('/api/v1/auth/oidc/login', window.location.origin);
	target.searchParams.set('redirect_to', redirectTo);
	window.location.href = target.toString();
}

export async function logout(): Promise<void> {
	try {
		await apiClient.postVoid('/auth/logout', null);
	} catch (error) {
		console.error('Failed to logout cleanly:', error);
	}
}

export async function requestDevicePairing(): Promise<DeviceRequestResponse> {
	return apiClient.post<DeviceRequestResponse>('/auth/device/request', null);
}

export async function pollDevicePairing(device_code: string): Promise<DevicePollResponse> {
	return apiClient.post<DevicePollResponse>('/auth/device/poll', { device_code });
}

export async function approveDevicePairing(user_code: string): Promise<{ device_name: string }> {
	return apiClient.post<{ device_name: string }>('/auth/device/approve', { user_code });
}

export async function approveDevicePairingByDeviceCode(
	device_code: string
): Promise<{ device_name: string }> {
	return apiClient.post<{ device_name: string }>('/auth/device/approve', { device_code });
}

export interface DeviceQrInfoResponse {
	instance_url: string;
	device_pairing_path: string;
}

export async function getDeviceQrInfo(): Promise<DeviceQrInfoResponse> {
	return apiClient.get<DeviceQrInfoResponse>('/auth/device/qr-info');
}
