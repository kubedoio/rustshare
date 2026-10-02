import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import { ApiError } from '$lib/api/types';
import CalendarSettingsPanel from './CalendarSettingsPanel.svelte';

const mocks = vi.hoisted(() => ({
	listSources: vi.fn(),
	listImportJobs: vi.fn(),
	uploadIcs: vi.fn(),
	connectSource: vi.fn(),
	disconnectSource: vi.fn(),
	resyncSource: vi.fn()
}));

vi.mock('$lib/api/calendar', () => ({
	calendarApi: {
		listSources: mocks.listSources,
		listImportJobs: mocks.listImportJobs,
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

	it('shows a success toast from the ?connected= redirect param and strips it', async () => {
		setUrl('?connected=google');
		const replaceState = vi.spyOn(window.history, 'replaceState');
		render(CalendarSettingsPanel);

		await waitFor(() => {
			expect(toastSpy).toHaveBeenCalledWith('Connected Google Calendar', 'success');
		});
		expect(replaceState).toHaveBeenCalledWith(null, '', '/settings/apps/calendar');
		replaceState.mockRestore();
	});

	it('shows an error toast from the ?error=oauth_* redirect param', async () => {
		setUrl('?error=oauth_denied');
		const replaceState = vi.spyOn(window.history, 'replaceState');
		render(CalendarSettingsPanel);

		await waitFor(() => {
			expect(toastSpy).toHaveBeenCalledWith('Provider access was denied.', 'error');
		});
		expect(replaceState).toHaveBeenCalledWith(null, '', '/settings/apps/calendar');
		replaceState.mockRestore();
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
});
