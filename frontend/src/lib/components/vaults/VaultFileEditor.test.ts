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
});
