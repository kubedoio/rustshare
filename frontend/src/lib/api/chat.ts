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
	channel_kind: string;
	latest_event_at: string;
}

export interface ChatMessageDto {
	message_id: string;
	event_id: string;
	community_id: string;
	channel_id: string;
	channel_kind: string;
	author_pubkey: string;
	event_created_at: string;
	thread_root_id: string | null;
	body: string | null;
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
