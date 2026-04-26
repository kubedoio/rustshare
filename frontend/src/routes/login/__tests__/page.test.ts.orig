import { render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$lib/stores/auth', () => ({
	authStore: {
		setLoading: vi.fn(),
		login: vi.fn()
	}
}));

vi.mock('$lib/api/auth', () => ({
	beginOidcLogin: vi.fn(),
	getAuthConfig: vi.fn()
}));

import LoginPage from '../+page.svelte';
import { getAuthConfig } from '$lib/api/auth';

describe('login page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('renders the OIDC-first view when only SSO is enabled', async () => {
		vi.mocked(getAuthConfig).mockResolvedValue({
			password_login_enabled: false,
			oidc_enabled: true,
			oidc_login_label: 'Continue with SSO',
			oidc_mobile_enabled: false
		});

		render(LoginPage);

		await waitFor(() => {
			expect(screen.getByRole('button', { name: 'Continue with SSO' })).toBeTruthy();
		});
		expect(screen.queryByLabelText('Email')).toBeNull();
	});

	it('shows both the SSO action and the password fallback when mixed mode is enabled', async () => {
		vi.mocked(getAuthConfig).mockResolvedValue({
			password_login_enabled: true,
			oidc_enabled: true,
			oidc_login_label: 'Continue with district SSO',
			oidc_mobile_enabled: false
		});

		render(LoginPage);

		await waitFor(() => {
			expect(screen.getByRole('button', { name: 'Continue with district SSO' })).toBeTruthy();
		});
		expect(screen.getByText('Password fallback')).toBeTruthy();
		expect(screen.getByLabelText('Email')).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Sign in with password' })).toBeTruthy();
	});

	it('keeps a usable password fallback when auth-config fetch fails', async () => {
		vi.mocked(getAuthConfig).mockRejectedValue(new Error('boom'));

		render(LoginPage);

		await waitFor(() => {
			expect(screen.getByText(/could not confirm the active login mode/i)).toBeTruthy();
		});
		expect(screen.getByLabelText('Email')).toBeTruthy();
		expect(screen.queryByText(/No login method is enabled for this deployment/i)).toBeNull();
	});

	it('surfaces the deployment mistake when no login method is enabled', async () => {
		vi.mocked(getAuthConfig).mockResolvedValue({
			password_login_enabled: false,
			oidc_enabled: false,
			oidc_login_label: null,
			oidc_mobile_enabled: false
		});

		render(LoginPage);

		await waitFor(() => {
			expect(screen.getByText(/No login method is enabled for this deployment/i)).toBeTruthy();
		});
	});
});
