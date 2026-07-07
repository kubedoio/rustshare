import type { VaultManifestEntry, VaultWritePolicy } from '$lib/api/types';

export const MAX_WEBUI_EDIT_SIZE = 1024 * 1024; // 1 MiB

export function isEditableVaultPolicy(policy: VaultWritePolicy): boolean {
	return policy === 'web_editing_enabled';
}

export function isEditableVaultFile(file: VaultManifestEntry): boolean {
	if (file.deleted) return false;
	if (file.size !== undefined && file.size !== null && file.size > MAX_WEBUI_EDIT_SIZE) {
		return false;
	}
	const path = file.path.toLowerCase();
	if (path.endsWith('.md') || path.endsWith('.markdown')) return true;
	if (path.endsWith('.txt')) {
		if (file.content_type) return file.content_type.startsWith('text/');
		return true;
	}
	return false;
}
