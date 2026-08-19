import { apiClient } from './client';

export interface ChatCommunityMapping {
	community_id: string;
	relay_url: string;
}

export interface ChatBinding {
	status: string;
	buzz_pubkey: string;
}

export interface ChatStatus {
	chat_enabled: boolean;
	mapping: ChatCommunityMapping | null;
	binding: ChatBinding | null;
	admission_active: boolean;
	ask_available: boolean;
}

export interface ChatChannelInfo {
	channel_id: string;
	name: string | null;
	channel_kind: string;
	channel_type: string | null;
	visibility: string | null;
	member: boolean | null;
	latest_event_at: string;
}

export interface ChatMessageAuthor {
	display_name: string;
	avatar_url: string | null;
	is_current_user: boolean;
}

/** Identifier-only attachment reference on a chat message (issue #242). Never
 * authority and never tenant-hinting: opening reauthorizes through the Files
 * owner at read time via `openChatAttachment`. */
export interface ChatAttachmentDto {
	application: string;
	resourceType: string;
	resourceId: string;
	version: string | null;
}

export interface ChatMessageDto {
	message_id: string;
	event_id: string;
	community_id: string;
	channel_id: string;
	channel_kind: string;
	author_pubkey: string;
	author: ChatMessageAuthor | null;
	event_created_at: string;
	thread_root_id: string | null;
	body: string | null;
	attachments: ChatAttachmentDto[];
}

/**
 * Reauthorize and open a chat attachment through the Files owner. The server
 * streams the authorized bytes, or answers an existence-hiding 404 when the
 * file is missing or the caller may not read it.
 */
export function openChatAttachment(attachment: ChatAttachmentDto): Promise<Blob> {
	return apiClient.requestBlob('/applications/chat/attachments/open', {
		method: 'POST',
		body: JSON.stringify({ resource: attachment })
	});
}

export interface ChatMessagesPage {
	messages: ChatMessageDto[];
	next_before: string | null;
}

export function getChatStatus(): Promise<ChatStatus> {
	return apiClient.get<ChatStatus>('/applications/chat/status');
}

export function getChatChannels(): Promise<ChatChannelInfo[]> {
	return apiClient.get<ChatChannelInfo[]>('/applications/chat/channels');
}

export function getChatMessages(
	channelId: string,
	before?: string | null
): Promise<ChatMessagesPage> {
	const params = new URLSearchParams({ channel_id: channelId });
	if (before) params.set('before', before);
	return apiClient.get<ChatMessagesPage>(`/applications/chat/messages?${params.toString()}`);
}

export function getChatMessage(messageId: string): Promise<ChatMessageDto> {
	return apiClient.get<ChatMessageDto>(
		`/applications/chat/messages/${encodeURIComponent(messageId)}`
	);
}

/** Canonical message ResourceRef for the UI's citation/deep-link handling. */
export function chatMessageRef(messageId: string): string {
	return `elembra://io.elembra.chat/message/${messageId}`;
}

/** Parse a message id out of a chat message ResourceRef URI. */
export function chatMessageIdFromRef(resourceRef: string): string | null {
	const prefix = 'elembra://io.elembra.chat/message/';
	if (!resourceRef.startsWith(prefix)) return null;
	const id = resourceRef.slice(prefix.length).split('?')[0];
	return id.length > 0 ? id : null;
}

export interface ChatProvisionResult {
	status: 'created' | 'already_configured';
	community_id: string;
	relay_url: string;
	relay_pubkey: string;
}

export interface AdminChatCommunityMapping {
	community_id: string;
	relay_url: string;
	relay_pubkey: string | null;
	active: boolean;
}

export function provisionChatCommunity(workspaceId: string): Promise<ChatProvisionResult> {
	return apiClient.post<ChatProvisionResult>(
		`/admin/applications/chat/workspaces/${encodeURIComponent(workspaceId)}/provision`,
		{}
	);
}

export function getChatCommunityMapping(workspaceId: string): Promise<AdminChatCommunityMapping> {
	return apiClient.get<AdminChatCommunityMapping>(
		`/admin/applications/chat/workspaces/${encodeURIComponent(workspaceId)}/community`
	);
}

export function connectChatCommunity(
	workspaceId: string,
	body: { community_id: string; relay_url: string; relay_pubkey?: string }
): Promise<void> {
	return apiClient.postVoid(
		`/admin/applications/chat/workspaces/${encodeURIComponent(workspaceId)}/community`,
		body
	);
}
