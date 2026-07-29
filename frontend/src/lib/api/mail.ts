import { apiClient } from './client';

export interface MailMessage {
	id: string;
	account_id: string | null;
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
	is_seen?: boolean;
	source_mode: MailSourceMode;
	in_reply_to?: string | null;
}

export type MailSourceMode =
	'eml_upload' | 'imap_selected' | 'imap_archive' | 'inbound_address' | 'outbound' | 'draft';

/**
 * Sort order for mail list endpoints. `date_desc` (newest message date first)
 * is the server default; omitted params mean `date_desc`.
 */
export type MailSortOrder = 'date_desc' | 'date_asc';

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
	display_name: string;
	delimiter: string | null;
	role: 'archive' | 'drafts' | 'sent' | 'trash' | null;
	unseen: number | null;
	total: number | null;
}

export interface MailAccountMessage {
	uid: number;
	subject: string | null;
	from_address: string | null;
	from_name: string | null;
	sent_at: string | null;
	size_bytes: number;
	is_seen?: boolean;
	is_flagged: boolean;
	imported_message_id: string | null;
}

export interface MailAddress {
	name: string | null;
	address: string;
}

export interface MailRemoteMessageBody {
	uid: number;
	subject: string | null;
	from_address: string | null;
	from_name: string | null;
	to: MailAddress[];
	cc: MailAddress[];
	date: string | null;
	message_id: string | null;
	in_reply_to: string | null;
	/** RAW html — always sanitize before rendering. */
	html: string | null;
	text: string | null;
	attachments: MailRemoteAttachment[];
	is_seen: boolean;
	is_flagged: boolean;
}

export interface MailRemoteAttachment {
	index: number;
	filename: string | null;
	mime_type: string;
	size_bytes: number;
	content_id: string | null;
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

export interface SendMailResponse {
	message_id: string | null;
	stored: boolean;
	append_failed: boolean;
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
	next_cursor_at: string | null;
	next_cursor_id: string | null;
}

export interface ListMailAccountsResponse {
	accounts: MailAccount[];
}

export interface ListMailFoldersResponse {
	folders: MailFolder[];
}

export interface ListMailAccountMessagesResponse {
	uidvalidity: number | null;
	next_cursor: number | null;
	messages: MailAccountMessage[];
}

export interface MailFolderActionRequest {
	folder: string;
	source_uidvalidity?: number | null;
	destination_folder?: string;
}

export interface MailFolderMoveRequest extends MailFolderActionRequest {
	destination_folder: string;
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

export interface SendMailMessageRequest {
	to: string[];
	cc?: string[];
	bcc?: string[];
	subject: string;
	body: string;
}

export interface SendMailMessageResponse {
	ok: boolean;
}

export interface MailSmtpSettings {
	id: string;
	tenant_id: string;
	owner_id: string;
	mail_account_id: string;
	host: string;
	port: number;
	username: string;
	tls_mode: 'tls' | 'starttls';
	from_address: string;
	from_name?: string | null;
	reply_to?: string | null;
	sent_folder?: string | null;
	is_enabled: boolean;
}

export interface CreateOrUpdateSmtpSettingsRequest {
	host: string;
	port: number;
	username: string;
	password?: string | null;
	tls_mode: 'tls' | 'starttls';
	from_address: string;
	from_name?: string | null;
	reply_to?: string | null;
	sent_folder?: string | null;
	is_enabled: boolean;
}

export interface SendOutboundMailRequest {
	to: string[];
	cc?: string[];
	bcc?: string[];
	subject: string;
	body: string;
	/** Sanitized HTML alternative for the body, generated from the rich-text editor. */
	body_html?: string | null;
	attachments?: string[];
	in_reply_to_msg_id?: string | null;
	/** Raw Message-ID header value; backend normalizes brackets. in_reply_to_msg_id wins when both set. */
	in_reply_to?: string | null;
	references?: string[] | null;
	idempotency_key?: string;
}

export type SaveDraftRequest = SendOutboundMailRequest;

export interface MailDraft {
	message: MailMessage;
	body: string;
	attachments: string[];
}

export const mailApi = {
	listAccounts: async (): Promise<MailAccount[]> => {
		const res = await apiClient.get<ListMailAccountsResponse>('/mail/accounts');
		return res.accounts;
	},

	createAccount: async (input: CreateMailAccountRequest): Promise<MailAccount> => {
		return apiClient.post<MailAccount>('/mail/accounts', input);
	},

	updateAccount: async (
		accountId: string,
		input: Partial<CreateMailAccountRequest> & { is_enabled?: boolean }
	): Promise<MailAccount> => {
		return apiClient.patch<MailAccount>(`/mail/accounts/${accountId}`, input);
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
		limit = 100,
		cursor?: number | null,
		search = '',
		sort?: MailSortOrder
	): Promise<ListMailAccountMessagesResponse> => {
		const cursorParam = cursor ? `&cursor=${cursor}` : '';
		const searchParam = search ? `&search=${encodeURIComponent(search)}` : '';
		const sortParam = sort ? `&sort=${sort}` : '';
		return apiClient.get<ListMailAccountMessagesResponse>(
			`/mail/accounts/${accountId}/messages?folder=${encodeURIComponent(folder)}&limit=${limit}${cursorParam}${searchParam}${sortParam}`
		);
	},

	markMessageRead: async (
		accountId: string,
		uid: number,
		folder: string,
		source_uidvalidity?: number | null
	): Promise<void> => {
		await apiClient.postVoid(`/mail/accounts/${accountId}/messages/${uid}/mark-read`, {
			folder,
			source_uidvalidity
		});
	},

	markMessageUnread: async (
		accountId: string,
		uid: number,
		folder: string,
		source_uidvalidity?: number | null
	): Promise<void> => {
		await apiClient.postVoid(`/mail/accounts/${accountId}/messages/${uid}/mark-unread`, {
			folder,
			source_uidvalidity
		});
	},

	moveMessage: async (
		accountId: string,
		uid: number,
		folder: string,
		destination_folder: string,
		source_uidvalidity?: number | null
	): Promise<void> => {
		await apiClient.postVoid(`/mail/accounts/${accountId}/messages/${uid}/move`, {
			folder,
			destination_folder,
			source_uidvalidity
		});
	},

	archiveMessage: async (
		accountId: string,
		uid: number,
		folder: string,
		source_uidvalidity?: number | null,
		destination_folder?: string
	): Promise<void> => {
		await apiClient.postVoid(`/mail/accounts/${accountId}/messages/${uid}/archive`, {
			folder,
			source_uidvalidity,
			destination_folder
		});
	},

	trashMessage: async (
		accountId: string,
		uid: number,
		folder: string,
		source_uidvalidity?: number | null,
		destination_folder?: string
	): Promise<void> => {
		await apiClient.postVoid(`/mail/accounts/${accountId}/messages/${uid}/trash`, {
			folder,
			source_uidvalidity,
			destination_folder
		});
	},

	deleteMessage: async (
		accountId: string,
		uid: number,
		folder: string,
		source_uidvalidity?: number | null
	): Promise<void> => {
		await apiClient.delete(`/mail/accounts/${accountId}/messages/${uid}`, {
			folder,
			source_uidvalidity
		});
	},

	getRemoteMessageBody: async (
		accountId: string,
		uid: number,
		folder: string,
		source_uidvalidity?: number | null
	): Promise<MailRemoteMessageBody> => {
		const params = new URLSearchParams({ folder });
		if (source_uidvalidity != null) params.set('source_uidvalidity', String(source_uidvalidity));
		return apiClient.get<MailRemoteMessageBody>(
			`/mail/accounts/${accountId}/messages/${uid}/body?${params}`
		);
	},

	remoteAttachmentUrl: (
		accountId: string,
		uid: number,
		index: number,
		folder: string,
		source_uidvalidity?: number | null
	): string => {
		const params = new URLSearchParams({ folder });
		if (source_uidvalidity != null) params.set('source_uidvalidity', String(source_uidvalidity));
		return `${apiClient.getBaseURL()}/mail/accounts/${accountId}/messages/${uid}/attachments/${index}?${params}`;
	},

	starMessage: async (
		accountId: string,
		uid: number,
		folder: string,
		source_uidvalidity?: number | null
	): Promise<void> => {
		await apiClient.postVoid(`/mail/accounts/${accountId}/messages/${uid}/star`, {
			folder,
			source_uidvalidity
		});
	},

	unstarMessage: async (
		accountId: string,
		uid: number,
		folder: string,
		source_uidvalidity?: number | null
	): Promise<void> => {
		await apiClient.postVoid(`/mail/accounts/${accountId}/messages/${uid}/unstar`, {
			folder,
			source_uidvalidity
		});
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

	listImportJobs: async (): Promise<MailImportJob[]> => {
		const res = await apiClient.get<{ jobs: MailImportJob[] }>('/mail/import-jobs');
		return res.jobs;
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

	sendMessage: async (input: SendMailMessageRequest): Promise<SendMailMessageResponse> => {
		return apiClient.post<SendMailMessageResponse>('/mail/send', input);
	},

	listMessages: async (): Promise<MailMessage[]> => {
		const res = await apiClient.get<ListMailMessagesResponse>('/mail/messages');
		return res.messages;
	},

	listMessagesPage: async (
		search = '',
		cursorAt?: string | null,
		cursorId?: string | null,
		sort?: MailSortOrder
	): Promise<ListMailMessagesResponse> => {
		const params = new URLSearchParams({ limit: '50' });
		if (search) params.set('search', search);
		if (sort) params.set('sort', sort);
		if (cursorAt && cursorId) {
			params.set('cursor_at', cursorAt);
			params.set('cursor_id', cursorId);
		}
		return apiClient.get<ListMailMessagesResponse>(`/mail/messages?${params}`);
	},

	listDrafts: async (accountId: string): Promise<MailMessage[]> => {
		const res = await apiClient.get<ListMailMessagesResponse>(`/mail/accounts/${accountId}/drafts`);
		return res.messages;
	},

	saveDraft: async (accountId: string, input: SaveDraftRequest): Promise<MailMessage> => {
		return apiClient.post<MailMessage>(`/mail/accounts/${accountId}/drafts`, input);
	},

	updateDraft: async (
		accountId: string,
		draftId: string,
		input: SaveDraftRequest
	): Promise<MailMessage> => {
		return apiClient.put<MailMessage>(`/mail/accounts/${accountId}/drafts/${draftId}`, input);
	},

	discardDraft: async (accountId: string, draftId: string): Promise<void> => {
		await apiClient.delete(`/mail/accounts/${accountId}/drafts/${draftId}`);
	},

	sendDraft: async (accountId: string, draftId: string): Promise<SendMailResponse> => {
		return apiClient.post<SendMailResponse>(
			`/mail/accounts/${accountId}/drafts/${draftId}/send`,
			{}
		);
	},

	getMessage: async (id: string): Promise<MailMessage> => {
		return apiClient.get<MailMessage>(`/mail/messages/${id}`);
	},

	getDraft: async (accountId: string, draftId: string): Promise<MailDraft> => {
		const message = await apiClient.get<MailMessage>(
			`/mail/accounts/${accountId}/drafts/${draftId}`
		);
		const [parts, attachments] = await Promise.all([
			mailApi.listParts(draftId),
			mailApi.listAttachments(draftId)
		]);
		const bodyPart = parts.find(
			(part) => part.is_body && part.content_type.startsWith('text/plain')
		);
		const body = bodyPart ? await mailApi.getPartContent(draftId, bodyPart.id) : '';
		return {
			message,
			body,
			attachments: attachments
				.map((attachment) => attachment.file_id)
				.filter((id): id is string => !!id)
		};
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
	},

	getSmtpSettings: async (accountId: string): Promise<MailSmtpSettings | null> => {
		try {
			return await apiClient.get<MailSmtpSettings>(`/mail/accounts/${accountId}/smtp`);
		} catch (err: unknown) {
			if (
				typeof err === 'object' &&
				err !== null &&
				'status' in err &&
				(err as { status?: number }).status === 404
			) {
				return null;
			}
			throw err;
		}
	},

	updateSmtpSettings: async (
		accountId: string,
		input: CreateOrUpdateSmtpSettingsRequest
	): Promise<MailSmtpSettings> => {
		return apiClient.put<MailSmtpSettings>(`/mail/accounts/${accountId}/smtp`, input);
	},

	deleteSmtpSettings: async (accountId: string): Promise<void> => {
		await apiClient.delete(`/mail/accounts/${accountId}/smtp`);
	},

	testSmtpConnection: async (accountId: string): Promise<{ ok: boolean }> => {
		return apiClient.post<{ ok: boolean }>(`/mail/accounts/${accountId}/smtp/test`, {});
	},

	sendOutboundMail: async (
		accountId: string,
		input: SendOutboundMailRequest
	): Promise<SendMailResponse> => {
		return apiClient.post<SendMailResponse>(`/mail/accounts/${accountId}/send`, input);
	},

	replyMail: async (
		accountId: string,
		input: SendOutboundMailRequest
	): Promise<SendMailResponse> => {
		return apiClient.post<SendMailResponse>(`/mail/accounts/${accountId}/reply`, input);
	},

	replyAllMail: async (
		accountId: string,
		input: SendOutboundMailRequest
	): Promise<SendMailResponse> => {
		return apiClient.post<SendMailResponse>(`/mail/accounts/${accountId}/reply-all`, input);
	},

	forwardMail: async (
		accountId: string,
		input: SendOutboundMailRequest
	): Promise<SendMailResponse> => {
		return apiClient.post<SendMailResponse>(`/mail/accounts/${accountId}/forward`, input);
	}
};
