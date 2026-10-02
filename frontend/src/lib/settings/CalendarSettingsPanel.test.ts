import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import { ApiError } from '$lib/api/types';
import CalendarSettingsPanel from './CalendarSettingsPanel.svelte';

const mocks = vi.hoisted(() => ({
	listSources: vi.fn(),
	listImportJobs: vi.fn(),
	listProviders: vi.fn(),
	uploadIcs: vi.fn(),
	connectSource: vi.fn(),
	disconnectSource: vi.fn(),
	resyncSource: vi.fn()
}));

vi.mock('$lib/api/calendar', () => ({
	calendarApi: {
		listSources: mocks.listSources,
		listImportJobs: mocks.listImportJobs,
		listProviders: mocks.listProviders,
		uploadIcs: mocks.uploadIcs,
		connectSource: mocks.connectSource,
		disconnectSource: mocks.disconnectSource,
		resyncSource: mocks.resyncSource
	}
}));

const toastSpy = vi.hoisted(() => vi.fn());
vi.mock('$lib/stores/toast', () => ({
	toastStore: { show: toastSpy }
}));

const replaceStateSpy = vi.hoisted(() => vi.fn());
vi.mock('$app/navigation', () => ({
	replaceState: replaceStateSpy
}));

const internalSource = {
	id: 'src-internal',
	kind: 'internal',
	display_name: 'My calendar',
	external_account: null,
	external_calendar_id: null,
	is_enabled: true,
	status: 'healthy',
	last_synced_at: null,
	last_error: null,
	created_at: '2026-10-01T00:00:00Z'
};

const googleSource = {
	...internalSource,
	id: 'src-google',
	kind: 'google',
	display_name: 'Google (alice@example.com)',
	external_account: 'alice@example.com',
	last_synced_at: '2026-10-01T10:00:00Z'
};

function setUrl(search: string) {
	// test-setup.ts replaces location with a static mock; redefine it per test
	// (history.replaceState therefore cannot update it, which is why the
	// redirect-param tests spy on replaceState instead).
	Object.defineProperty(window, 'location', {
		value: new URL(`http://localhost:3000/settings/apps/calendar${search}`),
		configurable: true,
		writable: true
	});
}

describe('CalendarSettingsPanel', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
		setUrl('');
		mocks.listSources.mockResolvedValue([internalSource, googleSource]);
		mocks.listImportJobs.mockResolvedValue([]);
		mocks.listProviders.mockResolvedValue({
			public_url: 'https://app.example.com',
			providers: [
				{
					kind: 'google',
					configured: true,
					redirect_uri: 'https://app.example.com/api/v1/calendar/oauth/google/callback'
				},
				{
					kind: 'outlook',
					configured: true,
					redirect_uri: 'https://app.example.com/api/v1/calendar/oauth/outlook/callback'
				}
			]
		});
	});

	it('renders sources and import jobs', async () => {
		mocks.listImportJobs.mockResolvedValue([
			{
				id: 'job-1',
				source_id: 'src-ical',
				filename: 'export.ics',
				status: 'completed',
				total_events: 5,
				processed_events: 5,
				failed_events: 0,
				last_error: null,
				started_at: null,
				completed_at: null,
				created_at: '2026-10-01T00:00:00Z'
			}
		]);
		render(CalendarSettingsPanel);

		expect(await screen.findByText('My calendar')).toBeTruthy();
		expect(await screen.findByText('export.ics')).toBeTruthy();
		expect(screen.getByText('5/5 events')).toBeTruthy();
	});

	it('navigates to the provider authorize URL on connect', async () => {
		const assign = vi.fn();
		Object.defineProperty(window.location, 'assign', { value: assign, configurable: true });
		mocks.connectSource.mockResolvedValue({ authorize_url: 'https://accounts.google.com/o/oauth' });
		render(CalendarSettingsPanel);

		await fireEvent.click(await screen.findByText('Connect Google Calendar'));

		await waitFor(() => {
			expect(mocks.connectSource).toHaveBeenCalledWith('google');
			expect(assign).toHaveBeenCalledWith('https://accounts.google.com/o/oauth');
		});
	});

	it('renders a disabled not-configured state when connect returns 503', async () => {
		mocks.connectSource.mockRejectedValue(new ApiError(503, 'Google OAuth is not configured'));
		render(CalendarSettingsPanel);

		const button = await screen.findByText('Connect Google Calendar');
		await fireEvent.click(button);

		const disabled = await screen.findByText('Connect Google Calendar', {
			selector: 'button:disabled'
		});
		expect(disabled).toBeTruthy();
		expect(disabled.title).toContain('not configured on this deployment');
		expect(toastSpy).toHaveBeenCalledWith(
			expect.stringContaining('not configured on this deployment'),
			'info'
		);
	});

	it('shows the expected redirect URI for a provider reported unconfigured', async () => {
		mocks.listProviders.mockResolvedValue({
			public_url: 'https://app.example.com',
			providers: [
				{
					kind: 'google',
					configured: false,
					redirect_uri: 'https://app.example.com/api/v1/calendar/oauth/google/callback'
				},
				{
					kind: 'outlook',
					configured: true,
					redirect_uri: 'https://app.example.com/api/v1/calendar/oauth/outlook/callback'
				}
			]
		});
		render(CalendarSettingsPanel);

		const disabled = await screen.findByText('Connect Google Calendar', {
			selector: 'button:disabled'
		});
		expect(disabled.title).toContain(
			'https://app.example.com/api/v1/calendar/oauth/google/callback'
		);
		// A configured provider stays enabled and shows no unconfigured copy.
		const outlook = await screen.findByText('Connect Outlook Calendar', {
			selector: 'button:not(:disabled)'
		});
		expect(outlook.title).not.toContain('not configured');
	});

	it('shows a success toast from the ?connected= redirect param and strips it', async () => {
		setUrl('?connected=google');
		render(CalendarSettingsPanel);

		await waitFor(() => {
			expect(toastSpy).toHaveBeenCalledWith('Connected Google Calendar', 'success');
		});
		expect(replaceStateSpy).toHaveBeenCalledWith('/settings/apps/calendar', {});
	});

	it('shows an error toast from the ?error=oauth_* redirect param', async () => {
		setUrl('?error=oauth_denied');
		render(CalendarSettingsPanel);

		await waitFor(() => {
			expect(toastSpy).toHaveBeenCalledWith('Provider access was denied.', 'error');
		});
		expect(replaceStateSpy).toHaveBeenCalledWith('/settings/apps/calendar', {});
	});

	it('maps reason=redirect_uri to actionable copy and strips reason from the URL', async () => {
		setUrl('?error=oauth_exchange&reason=redirect_uri');
		render(CalendarSettingsPanel);

		await waitFor(() => {
			expect(toastSpy).toHaveBeenCalledWith(
				'The provider rejected the redirect URI; check the registered callback URL.',
				'error'
			);
		});
		// Both error and reason must be consumed, leaving a bare path.
		expect(replaceStateSpy).toHaveBeenCalledWith('/settings/apps/calendar', {});
	});

	it('maps reason=not_configured to copy pointing at the providers endpoint', async () => {
		setUrl('?error=oauth_unconfigured&reason=not_configured');
		render(CalendarSettingsPanel);

		await waitFor(() => {
			expect(toastSpy).toHaveBeenCalledWith(
				expect.stringContaining('GET /api/v1/calendar/providers'),
				'error'
			);
		});
		expect(replaceStateSpy).toHaveBeenCalledWith('/settings/apps/calendar', {});
	});

	it('disconnects an OAuth source after confirmation', async () => {
		mocks.disconnectSource.mockResolvedValue(undefined);
		render(CalendarSettingsPanel);

		const googleRow = (await screen.findByText('Google (alice@example.com)')).closest('li');
		expect(googleRow).not.toBeNull();
		const disconnect = Array.from(googleRow!.querySelectorAll('button')).find((button) =>
			button.textContent?.includes('Disconnect')
		);
		await fireEvent.click(disconnect!);
		const confirm = Array.from(googleRow!.querySelectorAll('button')).find((button) =>
			button.textContent?.includes('Confirm disconnect')
		);
		await fireEvent.click(confirm!);

		await waitFor(() => {
			expect(mocks.disconnectSource).toHaveBeenCalledWith('src-google');
		});
	});

	it('tells Outlook users disconnect only removes local access', async () => {
		const outlookSource = {
			...internalSource,
			id: 'src-outlook',
			kind: 'outlook',
			display_name: 'Outlook (bob@example.com)',
			external_account: 'bob@example.com'
		};
		mocks.listSources.mockResolvedValue([internalSource, outlookSource]);
		render(CalendarSettingsPanel);

		const outlookRow = (await screen.findByText('Outlook (bob@example.com)')).closest('li');
		const disconnect = Array.from(outlookRow!.querySelectorAll('button')).find((button) =>
			button.textContent?.includes('Disconnect')
		);
		await fireEvent.click(disconnect!);

		expect(
			await screen.findByText(
				/To fully revoke Elembra's access, remove the app from your Microsoft account\./
			)
		).toBeTruthy();
	});

	it('requests a resync and surfaces the 409 lease-conflict as info', async () => {
		mocks.resyncSource.mockRejectedValue(
			new ApiError(409, 'A sync is already running for this source')
		);
		render(CalendarSettingsPanel);

		const googleRow = (await screen.findByText('Google (alice@example.com)')).closest('li');
		const resync = Array.from(googleRow!.querySelectorAll('button')).find((button) =>
			button.textContent?.includes('Resync')
		);
		await fireEvent.click(resync!);

		await waitFor(() => {
			expect(mocks.resyncSource).toHaveBeenCalledWith('src-google');
		});
		expect(toastSpy).toHaveBeenCalledWith('A sync is already running for this source.', 'info');
	});

	it('shows the fixed failure copy for a non-503 connect error', async () => {
		const assign = vi.fn();
		Object.defineProperty(window.location, 'assign', { value: assign, configurable: true });
		mocks.connectSource.mockRejectedValue(new Error('boom'));
		render(CalendarSettingsPanel);

		await fireEvent.click(await screen.findByText('Connect Google Calendar'));

		await waitFor(() => {
			expect(toastSpy).toHaveBeenCalledWith('Could not start Google connect. Try again.', 'error');
		});
		// The raw provider error must never reach the user.
		expect(toastSpy).not.toHaveBeenCalledWith(expect.stringContaining('boom'), expect.anything());
		expect(assign).not.toHaveBeenCalled();
	});

	it('connects Outlook through the same authorize flow', async () => {
		const assign = vi.fn();
		Object.defineProperty(window.location, 'assign', { value: assign, configurable: true });
		mocks.connectSource.mockResolvedValue({
			authorize_url: 'https://login.microsoftonline.com/common/oauth2/v2.0/authorize'
		});
		render(CalendarSettingsPanel);

		await fireEvent.click(await screen.findByText('Connect Outlook Calendar'));

		await waitFor(() => {
			expect(mocks.connectSource).toHaveBeenCalledWith('outlook');
			expect(assign).toHaveBeenCalledWith(
				'https://login.microsoftonline.com/common/oauth2/v2.0/authorize'
			);
		});
	});

	it('shows a success toast after a resync', async () => {
		mocks.resyncSource.mockResolvedValue(undefined);
		render(CalendarSettingsPanel);

		const googleRow = (await screen.findByText('Google (alice@example.com)')).closest('li');
		const resync = Array.from(googleRow!.querySelectorAll('button')).find((button) =>
			button.textContent?.includes('Resync')
		);
		await fireEvent.click(resync!);

		await waitFor(() => {
			expect(mocks.resyncSource).toHaveBeenCalledWith('src-google');
			expect(toastSpy).toHaveBeenCalledWith(
				'Resync requested for Google (alice@example.com)',
				'success'
			);
		});
	});

	it('keeps the source list usable when disconnect fails', async () => {
		mocks.disconnectSource.mockRejectedValue(new Error('boom'));
		render(CalendarSettingsPanel);

		const googleRow = (await screen.findByText('Google (alice@example.com)')).closest('li');
		const disconnect = Array.from(googleRow!.querySelectorAll('button')).find((button) =>
			button.textContent?.includes('Disconnect')
		);
		await fireEvent.click(disconnect!);
		const confirm = Array.from(googleRow!.querySelectorAll('button')).find((button) =>
			button.textContent?.includes('Confirm disconnect')
		);
		await fireEvent.click(confirm!);

		await waitFor(() => {
			expect(toastSpy).toHaveBeenCalledWith('Disconnect failed. Try again.', 'error');
		});
		// The failed action must not blow away the list.
		expect(await screen.findByText('My calendar')).toBeTruthy();
		expect(await screen.findByText('Google (alice@example.com)')).toBeTruthy();
	});

	it('offers Reconnect for a source parked in auth_required', async () => {
		const assign = vi.fn();
		Object.defineProperty(window.location, 'assign', { value: assign, configurable: true });
		const authRequiredSource = {
			...googleSource,
			id: 'src-google-stale',
			status: 'auth_required',
			display_name: 'Google (stale@example.com)',
			external_account: 'stale@example.com'
		};
		mocks.listSources.mockResolvedValue([internalSource, authRequiredSource]);
		mocks.connectSource.mockResolvedValue({ authorize_url: 'https://accounts.google.com/o/oauth' });
		render(CalendarSettingsPanel);

		const row = (await screen.findByText('Google (stale@example.com)')).closest('li');
		const reconnect = Array.from(row!.querySelectorAll('button')).find((button) =>
			button.textContent?.includes('Reconnect')
		);
		expect(reconnect).toBeTruthy();
		expect(await screen.findByText('Re-authorization required')).toBeTruthy();

		await fireEvent.click(reconnect!);

		await waitFor(() => {
			expect(mocks.connectSource).toHaveBeenCalledWith('google');
			expect(assign).toHaveBeenCalledWith('https://accounts.google.com/o/oauth');
		});
		// Reconnect must take the success path, not the generic failure toast.
		expect(toastSpy).not.toHaveBeenCalledWith(
			expect.stringContaining('Could not start'),
			expect.anything()
		);
	});
});
