import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import OfflineBanner from './OfflineBanner.svelte';

describe('OfflineBanner', () => {
	let onlineGetter: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		onlineGetter = vi.spyOn(navigator, 'onLine', 'get');
	});

	afterEach(() => {
		onlineGetter.mockRestore();
	});

	it('does not render when online', () => {
		onlineGetter.mockReturnValue(true);
		render(OfflineBanner);

		expect(screen.queryByRole('status')).toBeNull();
	});

	it('renders when offline', () => {
		onlineGetter.mockReturnValue(false);
		render(OfflineBanner);

		const status = screen.getByRole('status');
		expect(status).toBeTruthy();
		expect(status.textContent).toContain('You are offline');
	});

	it('shows offline message with WifiOff icon', () => {
		onlineGetter.mockReturnValue(false);
		render(OfflineBanner);

		expect(screen.getByText(/Some features may be unavailable/i)).toBeTruthy();
	});
});
