import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import VaultFileEditor from './VaultFileEditor.svelte';
import * as vaultsApi from '$lib/api/vaults';
import { queryClient } from '$lib/query-client';
import type { VaultManifestEntry } from '$lib/api/types';

vi.mock('$lib/api/vaults');

describe('VaultFileEditor', () => {
	beforeEach(() => {
		vi.restoreAllMocks();
		queryClient.clear();
	});

	it('shows read-only message for non-editable vault policy', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Read Only',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 11
		});

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'read_only', file } });

		await waitFor(() => {
			expect(screen.getByText('read-only vault')).toBeTruthy();
		});
	});

	it('renders textarea for editable Markdown file', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		await waitFor(() => {
			expect(screen.getByDisplayValue('# Hello')).toBeTruthy();
		});
	});

	it('shows non-editable message for a non-editable file type', () => {
		const file: VaultManifestEntry = {
			path: 'image.png',
			server_rev: 1,
			mtime_server: '',
			deleted: false,
			size: 1234,
			content_type: 'image/png'
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		expect(
			screen.getByText('This file type cannot be edited in the WebUI. Download it to edit locally.')
		).toBeTruthy();
	});

	it('saves updated content and updates revision text', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			server_rev: 2,
			updated_at: '2026-07-07T00:00:00Z'
		});

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# Updated' } });

		const saveButton = screen.getByRole('button', { name: /save/i });
		await fireEvent.click(saveButton);

		await waitFor(() => {
			expect(vaultsApi.saveVaultFileContent).toHaveBeenCalledWith('v1', 'note.md', {
				content: '# Updated',
				expected_revision: 1
			});
		});

		await waitFor(() => {
			expect(screen.getByText(/rev 2/i)).toBeTruthy();
		});
	});

	it('shows conflict message when save fails with 409', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent')
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Hello',
				server_rev: 1,
				content_type: 'text/markdown',
				size: 7
			})
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Changed elsewhere',
				server_rev: 2,
				content_type: 'text/markdown',
				size: 19
			});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({ status: 409 });

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# Conflict' } });

		const saveButton = screen.getByRole('button', { name: /save/i });
		await fireEvent.click(saveButton);

		await waitFor(() => {
			expect(
				screen.getByText(
					'This file was changed elsewhere. Copy your changes, reload, and try again.'
				)
			).toBeTruthy();
		});
	});

	it('marks the editor stale when save fails with a 409 current revision', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent')
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Hello',
				server_rev: 1,
				content_type: 'text/markdown',
				size: 7
			})
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Changed elsewhere',
				server_rev: 2,
				content_type: 'text/markdown',
				size: 19
			});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({
			status: 409,
			current_rev: 2
		});

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# Conflict' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		await waitFor(() => {
			expect(screen.getByText('stale')).toBeTruthy();
			expect(screen.getByRole<HTMLButtonElement>('button', { name: /save/i }).disabled).toBe(true);
		});
	});

	it('shows permission message when save fails with 403', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({ status: 403 });

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# No permission' } });

		const saveButton = screen.getByRole('button', { name: /save/i });
		await fireEvent.click(saveButton);

		await waitFor(() => {
			expect(screen.getByText('You do not have permission to edit this file.')).toBeTruthy();
		});
	});

	it('replaces clean cached content when a newer refetch arrives', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent')
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Cached',
				server_rev: 1,
				content_type: 'text/markdown',
				size: 8
			})
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Fresh',
				server_rev: 2,
				content_type: 'text/markdown',
				size: 7
			});

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		await waitFor(() => {
			expect(screen.getByDisplayValue('# Cached')).toBeTruthy();
		});

		queryClient.invalidateQueries({ queryKey: ['vault-file-content', 'v1', 'note.md'] });

		await waitFor(() => {
			expect(screen.getByDisplayValue('# Fresh')).toBeTruthy();
		});
		expect(screen.queryByDisplayValue('# Cached')).toBeNull();
	});

	it('preserves dirty editor content when the query refetches a newer revision', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent')
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Hello',
				server_rev: 1,
				content_type: 'text/markdown',
				size: 7
			})
			.mockResolvedValue({
				path: 'note.md',
				content: '# Changed elsewhere',
				server_rev: 2,
				content_type: 'text/markdown',
				size: 19
			});

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# Local edit' } });

		queryClient.invalidateQueries({ queryKey: ['vault-file-content', 'v1', 'note.md'] });

		await waitFor(() => {
			expect(vaultsApi.getVaultFileContent).toHaveBeenCalledWith('v1', 'note.md');
			expect(vi.mocked(vaultsApi.getVaultFileContent).mock.calls.length).toBeGreaterThanOrEqual(2);
		});

		expect(screen.getByDisplayValue('# Local edit')).toBeTruthy();
		await waitFor(() => {
			expect(screen.getByText('stale')).toBeTruthy();
		});
	});

	it('resets loaded revision when switching files and saves with the new file revision', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent')
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Hello',
				server_rev: 1,
				content_type: 'text/markdown',
				size: 7
			})
			.mockResolvedValueOnce({
				path: 'other.md',
				content: '# Other',
				server_rev: 10,
				content_type: 'text/markdown',
				size: 7
			});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockResolvedValueOnce({
			path: 'other.md',
			server_rev: 11,
			updated_at: '2026-07-07T00:00:00Z'
		});

		const file1: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		const { rerender } = render(VaultFileEditor, {
			props: { vaultId: 'v1', policy: 'web_editing_enabled', file: file1 }
		});

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# Edited note' } });

		const file2: VaultManifestEntry = {
			path: 'other.md',
			server_rev: 10,
			mtime_server: '',
			deleted: false
		};
		await rerender({ vaultId: 'v1', policy: 'web_editing_enabled', file: file2 });

		await waitFor(() => {
			expect(screen.getByDisplayValue('# Other')).toBeTruthy();
		});

		const textarea2 = screen.getByRole('textbox');
		await fireEvent.input(textarea2, { target: { value: '# Edited other' } });

		const saveButton = screen.getByRole('button', { name: /save/i });
		await fireEvent.click(saveButton);

		await waitFor(() => {
			expect(vaultsApi.saveVaultFileContent).toHaveBeenCalledWith('v1', 'other.md', {
				content: '# Edited other',
				expected_revision: 10
			});
		});
	});

	it('resets loaded content when switching vaults with the same selected path', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent')
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Vault 1',
				server_rev: 1,
				content_type: 'text/markdown',
				size: 9
			})
			.mockResolvedValueOnce({
				path: 'note.md',
				content: '# Vault 2',
				server_rev: 5,
				content_type: 'text/markdown',
				size: 9
			});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			server_rev: 6,
			updated_at: '2026-07-07T00:00:00Z'
		});

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		const { rerender } = render(VaultFileEditor, {
			props: { vaultId: 'v1', policy: 'web_editing_enabled', file }
		});

		const textarea = await waitFor(() => screen.getByDisplayValue('# Vault 1'));
		await fireEvent.input(textarea, { target: { value: '# Edited vault 1' } });

		await rerender({ vaultId: 'v2', policy: 'web_editing_enabled', file });

		const vault2Textarea = await waitFor(() => screen.getByDisplayValue('# Vault 2'));
		expect(screen.queryByDisplayValue('# Edited vault 1')).toBeNull();

		await fireEvent.input(vault2Textarea, { target: { value: '# Edited vault 2' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		await waitFor(() => {
			expect(vaultsApi.saveVaultFileContent).toHaveBeenCalledWith('v2', 'note.md', {
				content: '# Edited vault 2',
				expected_revision: 5
			});
		});
	});

	it('ignores stale content responses from a previously selected file', async () => {
		let resolveFirst: (value: Awaited<ReturnType<typeof vaultsApi.getVaultFileContent>>) => void;
		let resolveSecond: (value: Awaited<ReturnType<typeof vaultsApi.getVaultFileContent>>) => void;
		vi.spyOn(vaultsApi, 'getVaultFileContent')
			.mockReturnValueOnce(
				new Promise((resolve) => {
					resolveFirst = resolve;
				})
			)
			.mockReturnValueOnce(
				new Promise((resolve) => {
					resolveSecond = resolve;
				})
			);
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockResolvedValueOnce({
			path: 'other.md',
			server_rev: 11,
			updated_at: '2026-07-07T00:00:00Z'
		});

		const file1: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		const { rerender } = render(VaultFileEditor, {
			props: { vaultId: 'v1', policy: 'web_editing_enabled', file: file1 }
		});

		const file2: VaultManifestEntry = {
			path: 'other.md',
			server_rev: 10,
			mtime_server: '',
			deleted: false
		};
		await rerender({ vaultId: 'v1', policy: 'web_editing_enabled', file: file2 });

		resolveFirst!({
			path: 'note.md',
			content: '# Stale note',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 12
		});
		resolveSecond!({
			path: 'other.md',
			content: '# Other',
			server_rev: 10,
			content_type: 'text/markdown',
			size: 7
		});

		const textarea = await waitFor(() => screen.getByDisplayValue('# Other'));
		expect(screen.queryByDisplayValue('# Stale note')).toBeNull();

		await fireEvent.input(textarea, { target: { value: '# Edited other' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		await waitFor(() => {
			expect(vaultsApi.saveVaultFileContent).toHaveBeenCalledWith('v1', 'other.md', {
				content: '# Edited other',
				expected_revision: 10
			});
		});
	});
});
