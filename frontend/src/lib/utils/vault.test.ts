import { describe, it, expect } from 'vitest';
import { isEditableVaultFile, isEditableVaultPolicy } from './vault';
import type { VaultManifestEntry, VaultWritePolicy } from '$lib/api/types';

function makeFile(path: string, overrides: Partial<VaultManifestEntry> = {}): VaultManifestEntry {
	return {
		path,
		server_rev: 1,
		mtime_server: '2026-07-07T13:23:44Z',
		deleted: false,
		...overrides
	};
}

describe('vault file eligibility', () => {
	it('allows Markdown files', () => {
		expect(isEditableVaultFile(makeFile('note.md'))).toBe(true);
		expect(isEditableVaultFile(makeFile('note.markdown'))).toBe(true);
	});

	it('allows txt files treated as text', () => {
		expect(isEditableVaultFile(makeFile('note.txt', { content_type: 'text/plain' }))).toBe(true);
	});

	it('rejects binary files', () => {
		expect(isEditableVaultFile(makeFile('image.png', { content_type: 'image/png' }))).toBe(false);
		expect(isEditableVaultFile(makeFile('doc.pdf', { content_type: 'application/pdf' }))).toBe(
			false
		);
	});

	it('rejects large files', () => {
		expect(isEditableVaultFile(makeFile('big.md', { size: 2 * 1024 * 1024 }))).toBe(false);
	});

	it('rejects deleted files', () => {
		expect(isEditableVaultFile(makeFile('note.md', { deleted: true }))).toBe(false);
	});
});

describe('vault policy eligibility', () => {
	it('only web_editing_enabled is editable', () => {
		expect(isEditableVaultPolicy('web_editing_enabled')).toBe(true);
		expect(isEditableVaultPolicy('read_only')).toBe(false);
		expect(isEditableVaultPolicy('sync_client_only')).toBe(false);
	});
});
