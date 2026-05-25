import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	getOidcConfig,
	updateOidcConfig,
	testOidcConfig,
	type OidcConfig,
	type OidcConfigRequest
} from '$lib/api/admin';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		postVoid: vi.fn(),
		patchVoid: vi.fn(),
		requestText: vi.fn(),
		requestVoid: vi.fn(),
		get: vi.fn(),
		put: vi.fn(),
		post: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('OidcConfigForm admin API functions', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('fetches OIDC config', async () => {
		const config: OidcConfig = {
			enabled: true,
			provider_name: 'Google',
			client_id: 'my-client-id',
			client_secret: null,
			issuer_url: 'https://accounts.google.com',
			redirect_url: 'https://files.example.edu/api/v1/auth/oidc/callback',
			login_label: 'Continue with SSO',
			scopes: ['openid', 'email', 'profile'],
			auto_provision_users: true
		};
		vi.mocked(apiClient.get).mockResolvedValue(config);

		const result = await getOidcConfig();

		expect(apiClient.get).toHaveBeenCalledWith('/admin/config/oidc');
		expect(result.provider_name).toBe('Google');
		expect(result.enabled).toBe(true);
	});

	it('updates OIDC config without secret', async () => {
		const request: OidcConfigRequest = {
			enabled: true,
			provider_name: 'Okta',
			client_id: 'okta-client',
			issuer_url: 'https://okta.example.com',
			redirect_url: 'https://files.example.edu/api/v1/auth/oidc/callback',
			login_label: 'Continue with SSO',
			scopes: ['openid', 'email'],
			auto_provision_users: false
		};
		const response: OidcConfig = {
			enabled: true,
			provider_name: 'Okta',
			client_id: 'okta-client',
			client_secret: null,
			issuer_url: 'https://okta.example.com',
			redirect_url: 'https://files.example.edu/api/v1/auth/oidc/callback',
			login_label: 'Continue with SSO',
			scopes: ['openid', 'email'],
			auto_provision_users: false
		};
		vi.mocked(apiClient.put).mockResolvedValue(response);

		const result = await updateOidcConfig(request);

		expect(apiClient.put).toHaveBeenCalledWith('/admin/config/oidc', request);
		expect(result.provider_name).toBe('Okta');
	});

	it('updates OIDC config with new secret', async () => {
		const request: OidcConfigRequest = {
			enabled: true,
			client_secret: 'super-secret-value',
			auto_provision_users: true
		};
		const response: OidcConfig = {
			enabled: true,
			client_secret: null,
			auto_provision_users: true
		};
		vi.mocked(apiClient.put).mockResolvedValue(response);

		await updateOidcConfig(request);

		expect(apiClient.put).toHaveBeenCalledWith('/admin/config/oidc', request);
	});

	it('disables OIDC config', async () => {
		const request: OidcConfigRequest = { enabled: false };
		const response: OidcConfig = {
			enabled: false,
			client_secret: null,
			auto_provision_users: false
		};
		vi.mocked(apiClient.put).mockResolvedValue(response);

		const result = await updateOidcConfig(request);

		expect(result.enabled).toBe(false);
	});

	it('tests OIDC connection', async () => {
		const testResponse = { success: true, message: 'Connection successful' };
		vi.mocked(apiClient.post).mockResolvedValue(testResponse);

		const result = await testOidcConfig();

		expect(apiClient.post).toHaveBeenCalledWith('/admin/config/oidc/test');
		expect(result.success).toBe(true);
	});

	it('handles OIDC test failure', async () => {
		const testResponse = { success: false, message: 'Could not connect to issuer' };
		vi.mocked(apiClient.post).mockResolvedValue(testResponse);

		const result = await testOidcConfig();

		expect(result.success).toBe(false);
		expect(result.message).toBe('Could not connect to issuer');
	});

	it('secret field is null when not set server-side', async () => {
		const config: OidcConfig = {
			enabled: false,
			client_secret: null,
			auto_provision_users: false
		};
		vi.mocked(apiClient.get).mockResolvedValue(config);

		const result = await getOidcConfig();

		// Secret is null from server — UI should show placeholder, not the value
		expect(result.client_secret).toBeNull();
	});

	it('keeps redirect and login label fields in the OIDC contract', async () => {
		const request: OidcConfigRequest = {
			redirect_url: 'https://files.example.test/api/v1/auth/oidc/callback',
			login_label: 'Continue with SSO'
		};
		const response: OidcConfig = {
			enabled: true,
			client_secret: null,
			redirect_url: 'https://files.example.test/api/v1/auth/oidc/callback',
			login_label: 'Continue with SSO',
			auto_provision_users: false
		};
		vi.mocked(apiClient.put).mockResolvedValue(response);

		const result = await updateOidcConfig(request);

		expect(apiClient.put).toHaveBeenCalledWith('/admin/config/oidc', request);
		expect(result.redirect_url).toBe('https://files.example.test/api/v1/auth/oidc/callback');
		expect(result.login_label).toBe('Continue with SSO');
	});
});
