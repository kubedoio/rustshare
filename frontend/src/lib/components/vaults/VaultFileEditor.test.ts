import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import VaultFileEditor from './VaultFileEditor.svelte';
import * as vaultsApi from '$lib/api/vaults';
import { sha256Hex } from '$lib/utils/sha256';
import { queryClient } from '$lib/query-client';
import type { VaultManifestEntry } from '$lib/api/types';

vi.mock('$lib/api/vaults');
vi.mock('$lib/utils/sha256', () => ({ sha256Hex: vi.fn() }));

const { beforeNavigateCallbacks } = vi.hoisted(() => ({
	beforeNavigateCallbacks: [] as Array<(navigation: { cancel: () => void }) => void>
}));
vi.mock('$app/navigation', () => ({
	beforeNavigate: vi.fn((callback: (navigation: { cancel: () => void }) => void) => {
		beforeNavigateCallbacks.push(callback);
		return vi.fn();
	})
}));

describe('VaultFileEditor', () => {
	beforeEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
		beforeNavigateCallbacks.length = 0;
		vi.mocked(sha256Hex).mockResolvedValue(null);
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

	it('shows the tombstone panel when save fails with a 409 without current_rev', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		// Tombstone conflicts come back as a plain 409 with no current_rev.
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({ status: 409 });

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# My local work' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		await waitFor(() => {
			expect(screen.getByText('This file was deleted on the server.')).toBeTruthy();
		});
		expect(screen.getByRole('button', { name: /copy my changes/i })).toBeTruthy();
		expect(screen.getByRole('button', { name: /download my version/i })).toBeTruthy();
		expect(screen.getByRole('button', { name: /close file/i })).toBeTruthy();
		// A reload would 404, so no reload action is offered.
		expect(screen.queryByRole('button', { name: /reload server version/i })).toBeNull();
		// The file can never be saved again.
		expect(screen.getByRole<HTMLButtonElement>('button', { name: /save/i }).disabled).toBe(true);
		// Local unsaved text stays in the editor.
		expect(screen.getByDisplayValue('# My local work')).toBeTruthy();
	});

	it('copies local changes to the clipboard from the tombstone panel', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({ status: 409 });
		const writeText = vi.mocked(navigator.clipboard.writeText);
		writeText.mockClear();

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# My local work' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		const copyButton = await waitFor(() =>
			screen.getByRole('button', { name: /copy my changes/i })
		);
		await fireEvent.click(copyButton);

		await waitFor(() => {
			expect(writeText).toHaveBeenCalledWith('# My local work');
		});
		await waitFor(() => {
			expect(screen.getByText('Copied!')).toBeTruthy();
		});
	});

	it('downloads the local version as the file name from the tombstone panel', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'notes/note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({ status: 409 });

		// URL.createObjectURL / revokeObjectURL are mocked globally in test-setup.
		const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
		const createObjectURL = vi.mocked(URL.createObjectURL);
		const revokeObjectURL = vi.mocked(URL.revokeObjectURL);
		createObjectURL.mockClear();
		revokeObjectURL.mockClear();

		const file: VaultManifestEntry = {
			path: 'notes/note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# My local work' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		const downloadButton = await waitFor(() =>
			screen.getByRole('button', { name: /download my version/i })
		);
		await fireEvent.click(downloadButton);

		expect(createObjectURL).toHaveBeenCalledTimes(1);
		expect(createObjectURL.mock.calls[0][0]).toBeInstanceOf(Blob);
		expect(clickSpy).toHaveBeenCalledTimes(1);
		const anchor = clickSpy.mock.contexts[0] as HTMLAnchorElement;
		expect(anchor.download).toBe('note.md');
		expect(anchor.href).toContain('blob:');
		expect(revokeObjectURL).toHaveBeenCalledTimes(1);
		expect(revokeObjectURL.mock.calls[0][0]).toContain('blob:');
	});

	it('closes the file and clears the editor from the tombstone panel', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
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
		await fireEvent.input(textarea, { target: { value: '# My local work' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		const closeButton = await waitFor(() => screen.getByRole('button', { name: /close file/i }));
		await fireEvent.click(closeButton);

		await waitFor(() => {
			expect(screen.getByText('Select a file from the manifest to view or edit.')).toBeTruthy();
		});
		expect(screen.queryByDisplayValue('# My local work')).toBeNull();
		expect(screen.queryByText('This file was deleted on the server.')).toBeNull();
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

	it('shows conflict panel with actions on 409 and preserves the editor text', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({
			status: 409,
			current_rev: 2,
			server_sha256: 'differentsha'
		});
		vi.mocked(sha256Hex).mockResolvedValue('localsnapshotsha');

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# My local work' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		await waitFor(() => {
			expect(screen.getByText(/A newer server revision exists \(rev 2\)/)).toBeTruthy();
		});
		expect(screen.getByRole('button', { name: /copy my changes/i })).toBeTruthy();
		expect(screen.getByRole('button', { name: /download my version/i })).toBeTruthy();
		expect(screen.getByRole('button', { name: /reload server version/i })).toBeTruthy();
		// Local unsaved text stays in the editor.
		expect(screen.getByDisplayValue('# My local work')).toBeTruthy();
	});

	it('copies local changes to the clipboard from the conflict panel', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({
			status: 409,
			current_rev: 2,
			server_sha256: 'differentsha'
		});
		vi.mocked(sha256Hex).mockResolvedValue('localsnapshotsha');
		const writeText = vi.mocked(navigator.clipboard.writeText);
		writeText.mockClear();

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# My local work' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		const copyButton = await waitFor(() =>
			screen.getByRole('button', { name: /copy my changes/i })
		);
		await fireEvent.click(copyButton);

		await waitFor(() => {
			expect(writeText).toHaveBeenCalledWith('# My local work');
		});
		await waitFor(() => {
			expect(screen.getByText('Copied!')).toBeTruthy();
		});
	});

	it('downloads the local version as the file name from the conflict panel', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'notes/note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({
			status: 409,
			current_rev: 2,
			server_sha256: 'differentsha'
		});
		vi.mocked(sha256Hex).mockResolvedValue('localsnapshotsha');

		// URL.createObjectURL / revokeObjectURL are mocked globally in test-setup.
		const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
		const createObjectURL = vi.mocked(URL.createObjectURL);
		const revokeObjectURL = vi.mocked(URL.revokeObjectURL);
		createObjectURL.mockClear();
		revokeObjectURL.mockClear();

		const file: VaultManifestEntry = {
			path: 'notes/note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# My local work' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		const downloadButton = await waitFor(() =>
			screen.getByRole('button', { name: /download my version/i })
		);
		await fireEvent.click(downloadButton);

		expect(createObjectURL).toHaveBeenCalledTimes(1);
		expect(createObjectURL.mock.calls[0][0]).toBeInstanceOf(Blob);
		expect(clickSpy).toHaveBeenCalledTimes(1);
		const anchor = clickSpy.mock.contexts[0] as HTMLAnchorElement;
		expect(anchor.download).toBe('note.md');
		expect(anchor.href).toContain('blob:');
		expect(revokeObjectURL).toHaveBeenCalledTimes(1);
		expect(revokeObjectURL.mock.calls[0][0]).toContain('blob:');
	});

	it('reloads the server version from the conflict panel after confirmation', async () => {
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
				content: '# Server version',
				server_rev: 2,
				content_type: 'text/markdown',
				size: 16
			});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({
			status: 409,
			current_rev: 2,
			server_sha256: 'differentsha'
		});
		vi.mocked(sha256Hex).mockResolvedValue('localsnapshotsha');
		vi.stubGlobal(
			'confirm',
			vi.fn(() => true)
		);

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# My local work' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		const reloadButton = await waitFor(() =>
			screen.getByRole('button', { name: /reload server version/i })
		);
		await fireEvent.click(reloadButton);

		expect(confirm).toHaveBeenCalled();
		await waitFor(() => {
			expect(screen.getByDisplayValue('# Server version')).toBeTruthy();
		});
		expect(screen.queryByText(/A newer server revision exists/)).toBeNull();
		expect(screen.queryByDisplayValue('# My local work')).toBeNull();
	});

	it('does not reload the server version when the user cancels the confirmation', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({
			status: 409,
			current_rev: 2,
			server_sha256: 'differentsha'
		});
		vi.mocked(sha256Hex).mockResolvedValue('localsnapshotsha');
		vi.stubGlobal(
			'confirm',
			vi.fn(() => false)
		);

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# My local work' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		const reloadButton = await waitFor(() =>
			screen.getByRole('button', { name: /reload server version/i })
		);
		await fireEvent.click(reloadButton);

		expect(confirm).toHaveBeenCalled();
		expect(screen.getByDisplayValue('# My local work')).toBeTruthy();
		expect(screen.getByText(/A newer server revision exists/)).toBeTruthy();
	});

	it('adopts the server revision silently when the 409 server_sha256 matches the editor content', async () => {
		vi.spyOn(vaultsApi, 'getVaultFileContent').mockResolvedValueOnce({
			path: 'note.md',
			content: '# Hello',
			server_rev: 1,
			content_type: 'text/markdown',
			size: 7
		});
		vi.spyOn(vaultsApi, 'saveVaultFileContent').mockRejectedValueOnce({
			status: 409,
			current_rev: 2,
			server_sha256: 'identicalsha'
		});
		vi.mocked(sha256Hex).mockResolvedValue('identicalsha');

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# Same change' } });
		await fireEvent.click(screen.getByRole('button', { name: /save/i }));

		await waitFor(() => {
			expect(screen.getByText(/rev 2/i)).toBeTruthy();
		});
		expect(screen.queryByText(/A newer server revision exists/)).toBeNull();
		expect(screen.queryByText('stale')).toBeNull();
		// Treated as saved: editor content kept, no longer dirty.
		expect(screen.getByDisplayValue('# Same change')).toBeTruthy();
		await waitFor(() => {
			expect(screen.getByRole<HTMLButtonElement>('button', { name: /save/i }).disabled).toBe(true);
		});
	});

	it('suppresses the conflict when a refetched revision has identical content', async () => {
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
				content: '# Same change',
				server_rev: 2,
				content_type: 'text/markdown',
				size: 13
			});

		const file: VaultManifestEntry = {
			path: 'note.md',
			server_rev: 1,
			mtime_server: '',
			deleted: false
		};
		render(VaultFileEditor, { props: { vaultId: 'v1', policy: 'web_editing_enabled', file } });

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		await fireEvent.input(textarea, { target: { value: '# Same change' } });

		queryClient.invalidateQueries({ queryKey: ['vault-file-content', 'v1', 'note.md'] });

		await waitFor(() => {
			expect(vi.mocked(vaultsApi.getVaultFileContent).mock.calls.length).toBeGreaterThanOrEqual(2);
		});
		await waitFor(() => {
			expect(screen.getByText(/rev 2/i)).toBeTruthy();
		});
		expect(screen.queryByText('stale')).toBeNull();
		expect(screen.queryByText(/A newer server revision exists/)).toBeNull();
		expect(screen.getByDisplayValue('# Same change')).toBeTruthy();
	});

	it('shows the conflict panel with the current revision on a refetch mismatch with differing content', async () => {
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
				server_rev: 3,
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
			expect(screen.getByText(/A newer server revision exists \(rev 3\)/)).toBeTruthy();
		});
		expect(screen.getByDisplayValue('# Local edit')).toBeTruthy();
	});

	it('prompts beforeunload only when the editor has unsaved changes', async () => {
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

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));

		const cleanEvent = new Event('beforeunload', { cancelable: true });
		window.dispatchEvent(cleanEvent);
		expect(cleanEvent.defaultPrevented).toBe(false);

		await fireEvent.input(textarea, { target: { value: '# Dirty edit' } });

		const dirtyEvent = new Event('beforeunload', { cancelable: true });
		window.dispatchEvent(dirtyEvent);
		expect(dirtyEvent.defaultPrevented).toBe(true);
	});

	it('cancels in-app navigation with unsaved changes unless the user confirms', async () => {
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

		const textarea = await waitFor(() => screen.getByDisplayValue('# Hello'));
		expect(beforeNavigateCallbacks).toHaveLength(1);
		const guard = beforeNavigateCallbacks[0];

		const cleanNavigation = { cancel: vi.fn() };
		guard(cleanNavigation);
		expect(cleanNavigation.cancel).not.toHaveBeenCalled();

		await fireEvent.input(textarea, { target: { value: '# Dirty edit' } });

		vi.stubGlobal(
			'confirm',
			vi.fn(() => false)
		);
		const blockedNavigation = { cancel: vi.fn() };
		guard(blockedNavigation);
		expect(blockedNavigation.cancel).toHaveBeenCalledTimes(1);

		vi.stubGlobal(
			'confirm',
			vi.fn(() => true)
		);
		const allowedNavigation = { cancel: vi.fn() };
		guard(allowedNavigation);
		expect(allowedNavigation.cancel).not.toHaveBeenCalled();
	});
});
