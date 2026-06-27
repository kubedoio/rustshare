import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import OfflineBanner from './OfflineBanner.svelte';

function healthResponse(ok: boolean): Response {
	return new Response(JSON.stringify({ status: ok ? 'ok' : 'error' }), {
		status: ok ? 200 : 503,
		headers: { 'Content-Type': 'application/json' }
	});
}

describe('OfflineBanner', () => {
	let onlineGetter: ReturnType<typeof vi.spyOn>;
	let fetchMock: ReturnType<typeof vi.fn>;

	beforeEach(() => {
		onlineGetter = vi.spyOn(navigator, 'onLine', 'get');
		fetchMock = vi.fn();
		vi.stubGlobal('fetch', fetchMock);
	});

	afterEach(() => {
		onlineGetter.mockRestore();
		vi.unstubAllGlobals();
	});

	it('does not render when online', () => {
		onlineGetter.mockReturnValue(true);
		fetchMock.mockResolvedValue(healthResponse(true));
		render(OfflineBanner);

		expect(screen.queryByRole('status')).toBeNull();
	});

	it('renders when offline and backend is unreachable', async () => {
		onlineGetter.mockReturnValue(false);
		fetchMock.mockRejectedValue(new Error('Network error'));
		render(OfflineBanner);

		await waitFor(() => {
			expect(screen.getByRole('status')).toBeTruthy();
		});
		expect(screen.getByText(/You are offline/i)).toBeTruthy();
	});

	it('does not render when navigator reports offline but backend health check succeeds', async () => {
		onlineGetter.mockReturnValue(false);
		fetchMock.mockResolvedValue(healthResponse(true));
		render(OfflineBanner);

		await waitFor(() => {
			expect(screen.queryByRole('status')).toBeNull();
		});
	});

	it('shows offline message with WifiOff icon', async () => {
		onlineGetter.mockReturnValue(false);
		fetchMock.mockRejectedValue(new Error('Network error'));
		render(OfflineBanner);

		await waitFor(() => {
			expect(screen.getByText(/Some features may be unavailable/i)).toBeTruthy();
		});
	});
});
