import { render, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { goto, getDeviceQrInfo, pageStore, authStoreValue, qrCodeToDataURL, requestDevicePairing } =
	vi.hoisted(() => ({
		goto: vi.fn(),
		getDeviceQrInfo: vi.fn(),
		pageStore: createMockStore({
			url: new URL('http://localhost/device')
		}),
		authStoreValue: createMockStore({
			user: null,
			isAuthenticated: true,
			isLoading: false
		}),
		qrCodeToDataURL: vi.fn(),
		requestDevicePairing: vi.fn()
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
		subscribe: authStoreValue.subscribe,
		refreshSession: vi.fn()
	}
}));

vi.mock('$lib/api/auth', () => ({
	getDeviceQrInfo,
	requestDevicePairing,
	pollDevicePairing: vi.fn()
}));

vi.mock('qrcode', () => ({
	default: {
		toDataURL: qrCodeToDataURL
	}
}));

import DevicePage from '../+page.svelte';

describe('/device page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		pageStore.set({
			url: new URL('http://localhost/device')
		});
		authStoreValue.set({
			user: null,
			isAuthenticated: true,
			isLoading: false
		});
	});

	it('uses verification_uri_complete when generating the QR payload', async () => {
		requestDevicePairing.mockResolvedValue({
			user_code: 'ABCD-EFGH',
			device_code: 'device-code-123',
			expires_in: 300,
			verification_uri: 'https://approved.example.com/device/approve',
			verification_uri_complete:
				'https://approved.example.com/device/approve?device_code=device-code-123'
		});
		getDeviceQrInfo.mockResolvedValue({
			instance_url: 'https://fallback.example.invalid',
			device_pairing_path: '/device'
		});
		qrCodeToDataURL.mockResolvedValue('data:image/png;base64,qr');

		const { unmount } = render(DevicePage);

		await waitFor(() => {
			expect(qrCodeToDataURL).toHaveBeenCalledWith(
				'https://approved.example.com/device/approve?device_code=device-code-123',
				{
					width: 280,
					margin: 2,
					color: {
						dark: '#000000',
						light: '#ffffff'
					},
					errorCorrectionLevel: 'M'
				}
			);
		});

		expect(getDeviceQrInfo).not.toHaveBeenCalled();
		unmount();
	});

	it('falls back to a locally composed pairing URL when the complete link is missing', async () => {
		requestDevicePairing.mockResolvedValue({
			user_code: 'ABCD-EFGH',
			device_code: 'device-code-123',
			expires_in: 300,
			verification_uri: 'https://approved.example.com/device/approve',
			verification_uri_complete: ''
		});
		getDeviceQrInfo.mockResolvedValue({
			instance_url: 'https://fallback.example.invalid',
			device_pairing_path: '/device'
		});
		qrCodeToDataURL.mockResolvedValue('data:image/png;base64,qr');

		const { unmount } = render(DevicePage);

		await waitFor(() => {
			expect(qrCodeToDataURL).toHaveBeenCalledWith(
				'https://fallback.example.invalid/device?device_code=device-code-123',
				{
					width: 280,
					margin: 2,
					color: {
						dark: '#000000',
						light: '#ffffff'
					},
					errorCorrectionLevel: 'M'
				}
			);
		});

		expect(getDeviceQrInfo).toHaveBeenCalled();
		unmount();
	});
});
