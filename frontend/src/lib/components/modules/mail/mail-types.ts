import type { MailAccountMessage, MailFolder, MailMessage } from '$lib/api/mail';

/**
 * Which "folder" is selected in the workspace. `imap` folders are remote
 * listings, `imported`/`drafts` are local RustShare stores that stay
 * available even when the IMAP server is unreachable.
 */
export type FolderSelection =
	{ kind: 'imported' } | { kind: 'drafts' } | { kind: 'imap'; name: string };

/** What is shown in the viewer pane. */
export type ViewerTarget =
	{ kind: 'imap'; message: MailAccountMessage } | { kind: 'stored'; id: string } | null;

/** Normalized row for the message list pane. */
export type MailListItem =
	| { kind: 'imap'; uid: number; message: MailAccountMessage }
	| { kind: 'stored'; id: string; message: MailMessage };

export function formatMailAddresses(value: unknown): string {
	return mailAddressStrings(value).join(', ');
}

export function mailAddressStrings(value: unknown): string[] {
	if (!Array.isArray(value)) return value ? [String(value)] : [];
	return value
		.map((item) => (typeof item === 'string' ? item : (item as { address?: string })?.address))
		.filter((entry): entry is string => Boolean(entry));
}

export function formatMailBytes(value: number | null | undefined): string {
	if (!value) return '0 B';
	if (value < 1024) return `${value} B`;
	if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
	return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

/** Compact date for list rows: time for today, date otherwise. */
export function formatMailDate(value: string | null | undefined): string {
	if (!value) return '';
	const date = new Date(value);
	const now = new Date();
	if (date.toDateString() === now.toDateString()) {
		return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
	}
	return date.toLocaleDateString([], { month: 'short', day: 'numeric', year: 'numeric' });
}

export function formatSourceMode(mode: MailMessage['source_mode']): string {
	switch (mode) {
		case 'draft':
			return 'Draft';
		case 'outbound':
			return 'Sent';
		case 'imap_archive':
			return 'Archived';
		case 'imap_selected':
			return 'Mailbox';
		case 'inbound_address':
			return 'Inbound';
		case 'eml_upload':
			return 'Imported';
		default:
			return mode;
	}
}

/** Short source badge for imported rows. */
export function sourceBadge(mode: MailMessage['source_mode']): string {
	switch (mode) {
		case 'eml_upload':
			return 'EML';
		case 'imap_selected':
		case 'imap_archive':
			return 'IMAP';
		case 'outbound':
			return 'Sent';
		case 'inbound_address':
			return 'Inbound';
		default:
			return formatSourceMode(mode);
	}
}

export function findFolderByRole(
	folders: MailFolder[],
	role: MailFolder['role'],
	names: string[]
): string | undefined {
	return (
		folders.find((folder) => folder.role === role) ??
		folders.find((folder) => names.includes(folder.display_name.toLowerCase()))
	)?.name;
}

export type MailAccountStatus = 'connected' | 'partial' | 'failed' | 'untested' | 'disabled';

export function mailAccountStatus(
	account: Pick<MailMessage, never> & {
		is_enabled: boolean;
		last_error: string | null;
		last_connected_at: string | null;
	},
	hasSmtp: boolean | null
): MailAccountStatus {
	if (!account.is_enabled) return 'disabled';
	if (account.last_error) return 'failed';
	if (!account.last_connected_at) return 'untested';
	if (hasSmtp === false) return 'partial';
	return 'connected';
}

export function mailAccountStatusLabel(status: MailAccountStatus): string {
	switch (status) {
		case 'connected':
			return 'Connected';
		case 'partial':
			return 'Partially connected';
		case 'failed':
			return 'Connection failed';
		case 'untested':
			return 'Not tested';
		case 'disabled':
			return 'Disabled';
	}
}
