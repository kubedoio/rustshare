export type MailAccountStatus = 'connected' | 'partial' | 'failed' | 'untested' | 'disabled';

export function mailAccountStatus(
	account: {
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
