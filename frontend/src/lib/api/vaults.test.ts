import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { getVaultFileContent, saveVaultFileContent, updateVaultWritePolicy } from './vaults';
import { ApiError } from './types';
import type { Vault, VaultFileContent, SaveVaultFileContentResponse } from './types';

describe('vault content API', () => {
	beforeEach(() => {
		globalThis.fetch = vi.fn();
		sessionStorage.setItem('rustshare.websocket_token', 'token');
		document.cookie = 'rustshare_csrf_token=csrf-token; path=/';
	});

	afterEach(() => {
		vi.restoreAllMocks();
		sessionStorage.clear();
		document.cookie = 'rustshare_csrf_token=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
	});

	it('loads file content', async () => {
		const content: VaultFileContent = {
			path: 'note.md',
			content: '# Hello',
			server_rev: 3,
			content_type: 'text/markdown',
			size: 7
		};
		vi.mocked(fetch).mockResolvedValueOnce({
			ok: true,
			status: 200,
			json: async () => content
		} as Response);

		const result = await getVaultFileContent('vault-1', 'note.md');
		expect(result).toEqual(content);

		const [url, init] = vi.mocked(fetch).mock.calls[0];
		expect(url).toContain('/vaults/vault-1/content/note.md');
		expect(init?.method).toBe('GET');
		expect(init?.headers).toMatchObject({
			Authorization: 'Bearer token'
		});
	});

	it('throws ApiError with status and message on load failure', async () => {
		vi.mocked(fetch).mockResolvedValueOnce({
			ok: false,
			status: 403,
			statusText: 'Forbidden',
			json: async () => ({ error: 'Access denied' })
		} as Response);

		const error = await getVaultFileContent('vault-1', 'note.md').catch((e) => e);
		expect(error).toBeInstanceOf(ApiError);
		expect(error).toMatchObject({
			status: 403,
			message: 'Access denied'
		});
	});

	it('saves file content', async () => {
		const response: SaveVaultFileContentResponse = {
			path: 'note.md',
			server_rev: 4,
			updated_at: '2026-07-07T13:23:44Z'
		};
		vi.mocked(fetch).mockResolvedValueOnce({
			ok: true,
			status: 200,
			json: async () => response
		} as Response);

		const result = await saveVaultFileContent('vault-1', 'note.md', {
			content: '# Updated',
			expected_revision: 3
		});
		expect(result).toEqual(response);

		const [url, init] = vi.mocked(fetch).mock.calls[0];
		expect(url).toContain('/vaults/vault-1/content/note.md');
		expect(init?.method).toBe('PUT');
		expect(init?.headers).toMatchObject({
			'Content-Type': 'application/json',
			'X-Rustshare-Csrf': 'csrf-token'
		});
		expect(JSON.parse(init?.body as string)).toEqual({
			content: '# Updated',
			expected_revision: 3
		});
	});

	it('throws ApiError with status and message on save failure', async () => {
		vi.mocked(fetch).mockResolvedValueOnce({
			ok: false,
			status: 409,
			statusText: 'Conflict',
			json: async () => ({ error: 'Revision mismatch' })
		} as Response);

		const error = await saveVaultFileContent('vault-1', 'note.md', {
			content: '# Updated',
			expected_revision: 2
		}).catch((e) => e);
		expect(error).toBeInstanceOf(ApiError);
		expect(error).toMatchObject({
			status: 409,
			message: 'Revision mismatch'
		});
	});

	it('updates vault write policy', async () => {
		const vault: Vault = {
			id: 'vault-1',
			name: 'Test Vault',
			adapter: 'ObsidianVault',
			write_policy: 'web_editing_enabled',
			server_rev: 1,
			created_at: '2026-07-07T13:23:44Z',
			updated_at: '2026-07-07T13:23:44Z'
		};
		vi.mocked(fetch).mockResolvedValueOnce({
			ok: true,
			status: 200,
			json: async () => vault
		} as Response);

		const result = await updateVaultWritePolicy('vault-1', 'web_editing_enabled');
		expect(result).toEqual(vault);

		const [url, init] = vi.mocked(fetch).mock.calls[0];
		expect(url).toContain('/vaults/vault-1/write-policy');
		expect(init?.method).toBe('PATCH');
		expect(init?.headers).toMatchObject({
			'Content-Type': 'application/json',
			'X-Rustshare-Csrf': 'csrf-token'
		});
		expect(JSON.parse(init?.body as string)).toEqual({
			write_policy: 'web_editing_enabled'
		});
	});

	it('throws ApiError with status and message on policy update failure', async () => {
		vi.mocked(fetch).mockResolvedValueOnce({
			ok: false,
			status: 400,
			statusText: 'Bad Request',
			json: async () => ({ error: 'Invalid write policy' })
		} as Response);

		const error = await updateVaultWritePolicy('vault-1', 'web_editing_enabled').catch((e) => e);
		expect(error).toBeInstanceOf(ApiError);
		expect(error).toMatchObject({
			status: 400,
			message: 'Invalid write policy'
		});
	});
});
