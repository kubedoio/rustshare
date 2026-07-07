import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
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
});
