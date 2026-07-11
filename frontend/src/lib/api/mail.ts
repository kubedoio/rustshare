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

export interface ListMailMessagesResponse {
	messages: MailMessage[];
}

export interface ListMailMessagePartsResponse {
	parts: MailMessagePart[];
}

export interface ListMailMessageAttachmentsResponse {
	attachments: MailAttachment[];
}

export const mailApi = {
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

	downloadSourceUrl: (messageId: string): string => {
		return `${apiClient.getBaseURL()}/mail/messages/${messageId}/source`;
	}
};
