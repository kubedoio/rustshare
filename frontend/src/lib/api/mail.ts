import { apiClient } from './client';

export interface MailMessage {
	id: string;
	subject: string | null;
	from_address: string | null;
	from_name: string | null;
	to_addresses: unknown;
	cc_addresses: unknown;
	bcc_addresses: unknown;
	sent_at: string | null;
	imported_at: string;
	size_bytes: number;
	has_attachments: boolean;
}

export interface MailMessagePart {
	id: string;
	part_index: number;
	content_type: string;
	charset: string | null;
	size_bytes: number | null;
	is_body: boolean;
}

export interface MailAttachment {
	id: string;
	file_id: string | null;
	filename: string;
	mime_type: string | null;
	size_bytes: number | null;
}

export interface MailAccount {
	id: string;
	name: string;
	host: string;
	port: number;
	username: string;
	tls_mode: string;
	is_enabled: boolean;
	last_connected_at: string | null;
	last_error: string | null;
	created_at: string;
}

export interface CreateMailAccountRequest {
	name: string;
	host: string;
	port: number;
	username: string;
	password: string;
	tls_mode: 'none' | 'starttls' | 'tls';
}

export interface MailFolder {
	name: string;
	delimiter: string | null;
}

export interface MailAccountMessage {
	uid: number;
	subject: string | null;
	from_address: string | null;
	from_name: string | null;
	sent_at: string | null;
	size_bytes: number;
}

export interface MailImportJob {
	id: string;
	account_id: string;
	folder_name: string;
	status: string;
	total_messages: number;
	processed_messages: number;
	failed_messages: number;
	last_error: string | null;
	started_at: string | null;
	completed_at: string | null;
	created_at: string;
}

export interface MailArchiveJob extends MailImportJob {
	source_mode: string;
	archive_since: string | null;
	archive_before: string | null;
	last_uid_validity: number | null;
	last_imported_uid: number | null;
	retention_days: number | null;
	retry_count: number;
	max_retries: number;
	updated_at: string;
}

export interface MailLink {
	id: string;
	message_id: string;
	target_type: string;
	target_id: string;
	created_by: string;
	created_at: string;
}

export interface ListMailMessagesResponse {
	messages: MailMessage[];
}

export interface ListMailAccountsResponse {
	accounts: MailAccount[];
}

export interface ListMailFoldersResponse {
	folders: MailFolder[];
}

export interface ListMailAccountMessagesResponse {
	uidvalidity: number | null;
	messages: MailAccountMessage[];
}

export interface ListMailArchiveJobsResponse {
	jobs: MailArchiveJob[];
}

export interface ListMailLinksResponse {
	links: MailLink[];
}

export interface ListMailMessagePartsResponse {
	parts: MailMessagePart[];
}

export interface ListMailMessageAttachmentsResponse {
	attachments: MailAttachment[];
}

export const mailApi = {
	listAccounts: async (): Promise<MailAccount[]> => {
		const res = await apiClient.get<ListMailAccountsResponse>('/mail/accounts');
		return res.accounts;
	},

	createAccount: async (input: CreateMailAccountRequest): Promise<MailAccount> => {
		return apiClient.post<MailAccount>('/mail/accounts', input);
	},

	deleteAccount: async (accountId: string): Promise<void> => {
		await apiClient.delete(`/mail/accounts/${accountId}`);
	},

	testAccount: async (accountId: string): Promise<{ ok: boolean }> => {
		return apiClient.post<{ ok: boolean }>(`/mail/accounts/${accountId}/test`, {});
	},

	listFolders: async (accountId: string): Promise<MailFolder[]> => {
		const res = await apiClient.get<ListMailFoldersResponse>(`/mail/accounts/${accountId}/folders`);
		return res.folders;
	},

	listAccountMessages: async (
		accountId: string,
		folder: string,
		limit = 100
	): Promise<ListMailAccountMessagesResponse> => {
		return apiClient.get<ListMailAccountMessagesResponse>(
			`/mail/accounts/${accountId}/messages?folder=${encodeURIComponent(folder)}&limit=${limit}`
		);
	},

	createImportJob: async (
		accountId: string,
		input: { folder_name: string; source_uidvalidity: number | null; selected_uids: number[] }
	): Promise<MailImportJob> => {
		return apiClient.post<MailImportJob>(`/mail/accounts/${accountId}/import`, input);
	},

	getImportJob: async (jobId: string): Promise<MailImportJob> => {
		return apiClient.get<MailImportJob>(`/mail/import-jobs/${jobId}`);
	},

	listArchiveJobs: async (accountId: string): Promise<MailArchiveJob[]> => {
		const res = await apiClient.get<ListMailArchiveJobsResponse>(
			`/mail/accounts/${accountId}/archive-jobs`
		);
		return res.jobs;
	},

	createArchiveJob: async (
		accountId: string,
		input: {
			folder_name: string;
			archive_since?: string | null;
			archive_before?: string | null;
			retention_days?: number | null;
			max_retries?: number | null;
		}
	): Promise<MailArchiveJob> => {
		return apiClient.post<MailArchiveJob>(`/mail/accounts/${accountId}/archive-jobs`, input);
	},

	cancelArchiveJob: async (jobId: string): Promise<MailArchiveJob> => {
		return apiClient.patch<MailArchiveJob>(`/mail/archive-jobs/${jobId}/cancel`, {});
	},

	uploadMessage: async (file: File): Promise<MailMessage> => {
		const form = new FormData();
		form.append('file', file);
		return apiClient.post<MailMessage>('/mail/upload', form);
	},

	listMessages: async (): Promise<MailMessage[]> => {
		const res = await apiClient.get<ListMailMessagesResponse>('/mail/messages');
		return res.messages;
	},

	getMessage: async (id: string): Promise<MailMessage> => {
		return apiClient.get<MailMessage>(`/mail/messages/${id}`);
	},

	listParts: async (messageId: string): Promise<MailMessagePart[]> => {
		const res = await apiClient.get<ListMailMessagePartsResponse>(
			`/mail/messages/${messageId}/parts`
		);
		return res.parts;
	},

	getPartContent: async (messageId: string, partId: string): Promise<string> => {
		return apiClient.requestText(`/mail/messages/${messageId}/parts/${partId}`);
	},

	listAttachments: async (messageId: string): Promise<MailAttachment[]> => {
		const res = await apiClient.get<ListMailMessageAttachmentsResponse>(
			`/mail/messages/${messageId}/attachments`
		);
		return res.attachments;
	},

	listLinks: async (messageId: string): Promise<MailLink[]> => {
		const res = await apiClient.get<ListMailLinksResponse>(`/mail/messages/${messageId}/links`);
		return res.links;
	},

	createLink: async (
		messageId: string,
		input: { target_type: string; target_id: string }
	): Promise<MailLink> => {
		return apiClient.post<MailLink>(`/mail/messages/${messageId}/links`, input);
	},

	deleteLink: async (messageId: string, linkId: string): Promise<void> => {
		await apiClient.delete(`/mail/messages/${messageId}/links/${linkId}`);
	},

	downloadSourceUrl: (messageId: string): string => {
		return `${apiClient.getBaseURL()}/mail/messages/${messageId}/source`;
	}
};
