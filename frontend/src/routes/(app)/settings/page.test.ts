import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import Page from './+page.svelte';
import type { UserDevice, UserSession } from '$lib/api/users';

const mocks = vi.hoisted(() => ({
	url: new URL('http://localhost/settings?tab=security'),
	pageSubscriber: undefined as ((value: { url: URL }) => void) | undefined,
	listSessions: vi.fn(),
	listDevices: vi.fn(),
	revokeSession: vi.fn(),
	revokeDevice: vi.fn(),
	approveDevice: vi.fn(),
	logout: vi.fn()
}));

vi.mock('$app/stores', () => ({
	page: {
		subscribe: (run: (value: { url: URL }) => void) => {
			mocks.pageSubscriber = run;
			run({ url: mocks.url });
			return () => {
				mocks.pageSubscriber = undefined;
			};
		}
	}
}));
vi.mock('$lib/stores/auth', async () => {
	const { readable } = await import('svelte/store');
	return {
		currentUser: readable({ id: 'user-1', tenant_id: 'tenant-1' }),
		authStore: { logout: mocks.logout }
	};
});
vi.mock('$lib/api/users', () => ({
	listUserSessions: mocks.listSessions,
	listUserDevices: mocks.listDevices,
	revokeUserSession: mocks.revokeSession,
	revokeUserDevice: mocks.revokeDevice,
	getProfile: vi.fn(async () => ({ email_sharing_enabled: true })),
	listUserSecurityEvents: vi.fn(async () => []),
	updateUserPassword: vi.fn(),
	updateProfile: vi.fn(),
	updateTrashRetention: vi.fn()
}));
vi.mock('$lib/api/auth', () => ({ approveDevicePairing: mocks.approveDevice }));
vi.mock('$lib/api/files', () => ({ listAllFiles: vi.fn(async () => []) }));
vi.mock('$lib/stores/theme', async () => {
	const { readable } = await import('svelte/store');
	return { themeStore: { ...readable('light'), setTheme: vi.fn() } };
});
vi.mock('$lib/applications/registry', async () => {
	const { readable } = await import('svelte/store');
	return { applicationsStore: readable([]) };
});
vi.mock('$lib/stores/userApplicationPreferences', async () => {
	const { readable } = await import('svelte/store');
	return { userApplicationPreferences: readable({ preferences: {} }) };
});
vi.mock('$lib/query-compat', async () => {
	const { readable } = await import('svelte/store');
	return { createQuery: () => readable({ data: [] }) };
});
vi.mock('$lib/settings/MailSettingsPanel.svelte', () => ({ default: vi.fn() }));

const session: UserSession = {
	id: 'session-1',
	user_agent: 'Lost laptop',
	ip_address: null,
	created_at: '2026-01-01T00:00:00Z',
	last_seen_at: '2026-02-01T00:00:00Z',
	expires_at: '2999-01-01T00:00:00Z',
	is_current: false
};
const device: UserDevice = {
	id: 'token-1',
	device_name: 'Lost phone',
	created_at: '2026-01-01T00:00:00Z',
	last_used_at: null
};

beforeEach(() => {
	vi.clearAllMocks();
	mocks.url = new URL('http://localhost/settings?tab=security');
	mocks.listSessions.mockReset().mockResolvedValue([session]);
	mocks.listDevices.mockReset().mockResolvedValue([device]);
	mocks.revokeSession.mockReset().mockResolvedValue(undefined);
	mocks.revokeDevice.mockReset().mockResolvedValue(undefined);
	mocks.approveDevice.mockReset().mockResolvedValue({ device_name: 'New client' });
	mocks.logout.mockReset().mockResolvedValue(undefined);
});
afterEach(() => vi.restoreAllMocks());

describe('credential inventory and lost-device actions', () => {
	it('switches to Devices when the safety link navigates on the same route', async () => {
		render(Page);
		const safetyLink = await screen.findByRole('link', { name: 'Review device tokens' });
		await fireEvent.click(safetyLink);
		mocks.url = new URL(safetyLink.getAttribute('href')!, 'http://localhost');
		expect(mocks.pageSubscriber).toBeDefined();
		mocks.pageSubscriber!({ url: mocks.url });

		await screen.findByText('Lost phone');
		expect(screen.queryByRole('heading', { name: 'Chat devices and lost devices' })).toBeNull();
	});

	it.each(['SHORT', 'ABCDEFGHI'])(
		'rejects an approval code of the wrong length: %s',
		async (code) => {
			mocks.url = new URL('http://localhost/settings?tab=devices');
			render(Page);
			await screen.findByText('Lost phone');
			await fireEvent.input(screen.getByLabelText('Account device pairing code'), {
				target: { value: code }
			});
			await fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
			await screen.findByText('Please enter a valid 8-character pairing code');
			expect(mocks.approveDevice).not.toHaveBeenCalled();
		}
	);
	it('normalizes device approval and prevents conflicting inventory actions while approval is pending', async () => {
		mocks.url = new URL('http://localhost/settings?tab=devices');
		let complete!: (value: { device_name: string }) => void;
		mocks.approveDevice.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					complete = resolve;
				})
		);
		render(Page);
		await screen.findByText('Lost phone');
		await fireEvent.input(screen.getByPlaceholderText('XXXX-XXXX'), {
			target: { value: 'abcd-efgh' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
		await waitFor(() => expect(mocks.approveDevice).toHaveBeenCalledWith('ABCDEFGH'));
		expect(
			(screen.getByRole('button', { name: 'Refresh device tokens' }) as HTMLButtonElement).disabled
		).toBe(true);
		expect(
			(screen.getByRole('button', { name: 'Revoke device token: Lost phone' }) as HTMLButtonElement)
				.disabled
		).toBe(true);
		complete({ device_name: 'New client' });
		await waitFor(() => expect(mocks.listDevices).toHaveBeenCalledTimes(2));
	});

	it.each(['browser session', 'device token'] as const)(
		'serializes %s revocation and refresh',
		async (kind) => {
			let complete!: () => void;
			const revoke = kind === 'browser session' ? mocks.revokeSession : mocks.revokeDevice;
			revoke.mockImplementationOnce(
				() =>
					new Promise<void>((resolve) => {
						complete = resolve;
					})
			);
			mocks.listSessions.mockResolvedValue([
				session,
				{ ...session, id: 'session-2', user_agent: 'Other browser' }
			]);
			mocks.listDevices.mockResolvedValue([
				device,
				{ ...device, id: 'token-2', device_name: 'Other phone' }
			]);
			const label = kind === 'browser session' ? 'Lost laptop' : 'Lost phone';
			const otherLabel = kind === 'browser session' ? 'Other browser' : 'Other phone';
			const refreshLabel =
				kind === 'browser session' ? 'Refresh browser sessions' : 'Refresh device tokens';
			if (kind === 'device token') mocks.url = new URL('http://localhost/settings?tab=devices');
			render(Page);
			await screen.findByText(label);
			if (kind === 'device token') {
				await fireEvent.input(screen.getByPlaceholderText('XXXX-XXXX'), {
					target: { value: 'ABCD-EFGH' }
				});
				expect(
					(screen.getByRole('button', { name: 'Approve' }) as HTMLButtonElement).disabled
				).toBe(false);
			}
			await fireEvent.click(screen.getByRole('button', { name: `Revoke ${kind}: ${label}` }));
			expect(
				(screen.getByRole('button', { name: refreshLabel }) as HTMLButtonElement).disabled
			).toBe(true);
			await fireEvent.click(
				within(screen.getByRole('dialog')).getByRole('button', { name: 'Revoke credential' })
			);
			await waitFor(() => expect(revoke).toHaveBeenCalledTimes(1));
			expect(
				(screen.getByRole('button', { name: refreshLabel }) as HTMLButtonElement).disabled
			).toBe(true);
			expect(
				(screen.getByRole('button', { name: `Revoke ${kind}: ${otherLabel}` }) as HTMLButtonElement)
					.disabled
			).toBe(true);
			if (kind === 'device token') {
				expect(
					(screen.getByRole('button', { name: 'Approve' }) as HTMLButtonElement).disabled
				).toBe(true);
			}
			complete();
			await waitFor(() => expect(screen.queryByText(label)).toBeNull());
			expect(
				(screen.getByRole('button', { name: refreshLabel }) as HTMLButtonElement).disabled
			).toBe(false);
		}
	);
	it.each(['sessions', 'devices'] as const)(
		'disables %s revocation while refreshing its inventory',
		async (kind) => {
			const list = kind === 'sessions' ? mocks.listSessions : mocks.listDevices;
			if (kind === 'devices') mocks.url = new URL('http://localhost/settings?tab=devices');
			render(Page);
			const rowLabel = kind === 'sessions' ? 'Lost laptop' : 'Lost phone';
			const refreshLabel =
				kind === 'sessions' ? 'Refresh browser sessions' : 'Refresh device tokens';
			await screen.findByText(rowLabel);
			let complete!: (value: (typeof session)[] | (typeof device)[]) => void;
			list.mockImplementationOnce(
				() =>
					new Promise((resolve) => {
						complete = resolve as typeof complete;
					})
			);
			await fireEvent.click(screen.getByRole('button', { name: refreshLabel }));
			expect(
				screen.queryByRole('button', {
					name:
						kind === 'sessions'
							? 'Revoke browser session: Lost laptop'
							: 'Revoke device token: Lost phone'
				})
			).toBeNull();
			complete(kind === 'sessions' ? [session] : [device]);
			await screen.findByRole('button', {
				name:
					kind === 'sessions'
						? 'Revoke browser session: Lost laptop'
						: 'Revoke device token: Lost phone'
			});
		}
	);

	it('shows expired sessions accurately and identifies the current session', async () => {
		mocks.listSessions.mockResolvedValue([
			{ ...session, is_current: true },
			{
				...session,
				id: 'expired',
				user_agent: 'Expired browser',
				expires_at: '2020-01-01T00:00:00Z'
			}
		]);
		render(Page);
		await screen.findByText('Expired browser');
		expect(screen.getByText('Browser session · Expired')).toBeTruthy();
		expect(screen.getByText('Current')).toBeTruthy();
		expect(screen.getAllByRole('button', { name: /^Revoke/ })).toHaveLength(1);
	});

	it.each(['sessions', 'devices'] as const)(
		'reports failed %s inventory and allows retry',
		async (kind) => {
			const list = kind === 'sessions' ? mocks.listSessions : mocks.listDevices;
			list.mockRejectedValueOnce(new Error('offline'));
			if (kind === 'devices') mocks.url = new URL('http://localhost/settings?tab=devices');
			render(Page);
			const alert = await screen.findByRole('alert');
			expect(alert.textContent).toContain('Could not load');
			expect(screen.queryByText(/No active (sessions|device tokens)/)).toBeNull();
			await fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
			await screen.findByText(kind === 'sessions' ? session.user_agent! : device.device_name);
			expect(screen.queryByRole('alert')).toBeNull();
		}
	);

	it('requires confirmation and removes only the revoked session without another inventory read', async () => {
		mocks.listSessions.mockResolvedValue([
			session,
			{ ...session, id: 'current', user_agent: 'Current browser', is_current: true }
		]);
		render(Page);
		await screen.findByText('Lost laptop');
		await fireEvent.click(screen.getByRole('button', { name: /^Revoke/ }));
		let dialog = screen.getByRole('dialog');
		expect(dialog.textContent).toContain('does not revoke a copied Buzz signing key');
		await fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
		expect(mocks.revokeSession).not.toHaveBeenCalled();
		await fireEvent.click(screen.getByRole('button', { name: /^Revoke/ }));
		dialog = screen.getByRole('dialog');
		await fireEvent.click(within(dialog).getByRole('button', { name: 'Revoke credential' }));
		await waitFor(() => expect(mocks.revokeSession).toHaveBeenCalledWith(session.id));
		await waitFor(() => expect(screen.queryByText('Lost laptop')).toBeNull());
		expect(mocks.listSessions).toHaveBeenCalledTimes(1);
		expect(screen.getByText('Current browser')).toBeTruthy();
	});

	it('revokes the selected device token and preserves the other token', async () => {
		mocks.url = new URL('http://localhost/settings?tab=devices');
		mocks.listDevices.mockResolvedValue([
			device,
			{ ...device, id: 'token-2', device_name: 'Other phone' }
		]);
		render(Page);
		await screen.findByText('Lost phone');
		await fireEvent.click(screen.getByRole('button', { name: 'Revoke device token: Lost phone' }));
		const dialog = screen.getByRole('dialog');
		expect(dialog.textContent).toContain('device token');
		expect(dialog.textContent).toContain('does not revoke a copied Buzz signing key');
		await fireEvent.click(within(dialog).getByRole('button', { name: 'Revoke credential' }));
		await screen.findByText('Device token revoked');
		expect(mocks.revokeDevice).toHaveBeenCalledWith('token-1');
		expect(mocks.revokeSession).not.toHaveBeenCalled();
		expect(screen.queryByText('Lost phone')).toBeNull();
		expect(screen.getByText('Other phone')).toBeTruthy();
		expect(mocks.listDevices).toHaveBeenCalledTimes(1);
	});

	it('retains a device token when revocation fails and reports the failure', async () => {
		mocks.url = new URL('http://localhost/settings?tab=devices');
		mocks.revokeDevice.mockRejectedValueOnce(new Error('Revocation failed'));
		render(Page);
		await screen.findByText('Lost phone');
		expect(screen.getByText('Last active: Not available')).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: /^Revoke/ }));
		await fireEvent.click(
			within(screen.getByRole('dialog')).getByRole('button', { name: 'Revoke credential' })
		);
		await screen.findByText('Revocation failed');
		expect(screen.getByText('Lost phone')).toBeTruthy();
		expect(screen.queryByText('Device token revoked')).toBeNull();
		mocks.listDevices.mockResolvedValueOnce([]);
		await fireEvent.click(screen.getByRole('button', { name: 'Refresh device tokens' }));
		await waitFor(() => expect(screen.queryByText('Lost phone')).toBeNull());
	});

	it('reports a failed sign-out instead of leaving an unhandled request', async () => {
		mocks.logout.mockRejectedValueOnce(new Error('offline'));
		render(Page);
		await fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
		await screen.findByText(
			'Sign out failed. Your browser session may still be active. Try again.'
		);
	});
});
