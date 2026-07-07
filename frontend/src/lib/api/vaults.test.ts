import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { getVaultFileContent, saveVaultFileContent, updateVaultWritePolicy } from './vaults';
import { ApiError } from './types';

describe('vault content API', () => {
	beforeEach(() => {
		globalThis.fetch = vi.fn();
		sessionStorage.setItem('rustshare.websocket_token', 'token');
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it('loads file content', async () => {
		const content: import('./types').VaultFileContent = {
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
	});

	it('throws ApiError on load failure', async () => {
		vi.mocked(fetch).mockResolvedValueOnce({
			ok: false,
			status: 403
		} as Response);

		await expect(getVaultFileContent('vault-1', 'note.md')).rejects.toBeInstanceOf(ApiError);
	});

	it('saves file content', async () => {
		const response: import('./types').SaveVaultFileContentResponse = {
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
	});

	it('updates vault write policy', async () => {
		const vault: import('./types').Vault = {
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
	});
});
