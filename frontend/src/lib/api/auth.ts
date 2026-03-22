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
		await apiClient.post<void>('/auth/logout', null);
	} catch (error) {
		console.error('Failed to logout cleanly:', error);
	}

	if (typeof window !== 'undefined') {
		window.location.href = '/login';
	}
}
