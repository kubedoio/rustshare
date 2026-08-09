// Regression: avatar and folder-download URLs must be built from the
// configured API base URL, not hardcoded against the frontend origin.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getAvatarUrl, uploadAvatar } from '$lib/api/users';
import { downloadFolder } from '$lib/api/folders';
import { beginOidcLogin } from '$lib/api/auth';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		getBaseURL: vi.fn().mockReturnValue('https://api.example.com/api/v1')
	},
	getCsrfToken: vi.fn().mockReturnValue('csrf')
}));

describe('avatar URL construction', () => {
	beforeEach(() => {
		vi.restoreAllMocks();
	});

	it('builds avatar URLs from the configured API base URL', () => {
		expect(getAvatarUrl('user-1')).toBe('https://api.example.com/api/v1/users/user-1/avatar');
	});

	it('uploads avatars to the configured API base URL', async () => {
		const fetchMock = vi.fn().mockResolvedValue({
			ok: true,
			json: async () => ({ avatar_path: '/avatars/x.png' })
		});
		vi.stubGlobal('fetch', fetchMock);

		await uploadAvatar(new File(['x'], 'a.png', { type: 'image/png' }));

		const [url] = fetchMock.mock.calls[0];
		expect(url).toBe('https://api.example.com/api/v1/users/me/avatar');
	});
});

describe('folder download URL construction', () => {
	it('downloads folders from the configured API base URL', async () => {
		const fetchMock = vi.fn().mockResolvedValue({
			ok: true,
			blob: async () => new Blob(['zip'])
		});
		vi.stubGlobal('fetch', fetchMock);

		await downloadFolder('folder-1');

		const [url] = fetchMock.mock.calls[0];
		expect(url).toBe('https://api.example.com/api/v1/folders/folder-1/download');
	});
});

describe('OIDC login URL construction', () => {
	it('starts OIDC login against the configured API base URL', () => {
		// happy-dom supports location.href assignment without navigation.
		beginOidcLogin('/dashboard');
		expect(window.location.href).toContain(
			'https://api.example.com/api/v1/auth/oidc/login?redirect_to=%2Fdashboard'
		);
	});
});
