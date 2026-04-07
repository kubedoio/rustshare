import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '$lib/api/types';

const { goto, approveDevicePairingByDeviceCode, pageStore, authStoreValue } = vi.hoisted(() => ({
	goto: vi.fn(),
	approveDevicePairingByDeviceCode: vi.fn(),
	pageStore: createMockStore({
		url: new URL('http://localhost/device/approve?device_code=device-code-123')
	}),
	authStoreValue: createMockStore({
		user: null,
		isAuthenticated: true,
		isLoading: false
	})
}));

function createMockStore<T>(initialValue: T) {
	let value = initialValue;
	const subscribers = new Set<(value: T) => void>();

	return {
		subscribe(run: (value: T) => void) {
			run(value);
			subscribers.add(run);
			return () => subscribers.delete(run);
		},
		set(nextValue: T) {
			value = nextValue;
			for (const subscriber of subscribers) {
				subscriber(value);
			}
		}
	};
}

vi.mock('$app/navigation', () => ({
	goto
}));

vi.mock('$app/environment', () => ({
	browser: true
}));

vi.mock('$app/stores', () => ({
	page: {
		subscribe: pageStore.subscribe
	}
}));

vi.mock('$lib/stores/auth', () => ({
	authStore: {
		subscribe: authStoreValue.subscribe
	}
}));

vi.mock('$lib/api/auth', () => ({
	approveDevicePairingByDeviceCode
}));

import DeviceApprovePage from '../+page.svelte';

describe('device approval page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		pageStore.set({
			url: new URL('http://localhost/device/approve?device_code=device-code-123')
		});
		authStoreValue.set({
			user: null,
			isAuthenticated: true,
			isLoading: false
		});
	});

	it('shows invalid state when device_code is missing', async () => {
		pageStore.set({
			url: new URL('http://localhost/device/approve')
		});

		render(DeviceApprovePage);

		await waitFor(() => {
			expect(
				screen.getByText(/missing its device token/i)
			).toBeTruthy();
		});
	});

	it('redirects unauthenticated users to login with a return target', async () => {
		authStoreValue.set({
			user: null,
			isAuthenticated: false,
			isLoading: false
		});

		render(DeviceApprovePage);

		await waitFor(() => {
			expect(goto).toHaveBeenCalledWith(
				'/login?redirect_to=%2Fdevice%2Fapprove%3Fdevice_code%3Ddevice-code-123'
			);
		});
	});

	it('submits device_code approval for authenticated users', async () => {
		approveDevicePairingByDeviceCode.mockResolvedValue({ device_name: 'Device' });

		render(DeviceApprovePage);

		await fireEvent.click(screen.getByRole('button', { name: /approve device/i }));

		expect(approveDevicePairingByDeviceCode).toHaveBeenCalledWith('device-code-123');

		await waitFor(() => {
			expect(screen.getByText(/device approved/i)).toBeTruthy();
		});
	});

	it('shows an expired-link style error when approval returns not found', async () => {
		approveDevicePairingByDeviceCode.mockRejectedValue(new ApiError(404, 'code_not_found'));

		render(DeviceApprovePage);

		await fireEvent.click(screen.getByRole('button', { name: /approve device/i }));

		await waitFor(() => {
			expect(screen.getByText(/invalid or has expired/i)).toBeTruthy();
		});
	});
});
