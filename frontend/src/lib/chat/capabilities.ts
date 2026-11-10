import type { NostrEvent } from './nostr';

export type ChatProfile = 'fork_client_held' | 'upstream_managed' | 'local_client_held';
export type ChatRole = 'owner' | 'admin' | 'member' | 'guest' | 'bot';
export type ChatCapability = { available: true } | { available: false; reason: string };

/** Protocol support is not an authorization grant or a live acceptance result. */
export interface ChatCapabilities {
	directory: ChatCapability;
	createChannel: ChatCapability;
	members: ChatCapability;
	metadata: ChatCapability;
	roles: ChatCapability;
	archive: ChatCapability;
	deleteChannel: ChatCapability;
	messages: ChatCapability;
	threads: ChatCapability;
	reactions: ChatCapability;
	deleteMessage: ChatCapability;
	editMessage: ChatCapability;
	readOnlyGuests: ChatCapability;
}

export function getChatCapabilities(profile: ChatProfile): ChatCapabilities {
	const supported: ChatCapability = { available: true };
	const unavailable = (reason: string): ChatCapability => ({ available: false, reason });
	const protocol =
		profile === 'fork_client_held'
			? supported
			: unavailable(
					profile === 'upstream_managed'
						? 'Upstream-managed Chat remains disabled by its security gates.'
						: 'An authoritative Buzz relay is required.'
				);
	return {
		directory: protocol,
		createChannel: protocol,
		members: protocol,
		metadata: protocol,
		roles: protocol,
		archive: protocol,
		deleteChannel: protocol,
		messages: protocol,
		threads: protocol,
		reactions: protocol,
		deleteMessage: protocol,
		editMessage: unavailable('Message edits are not compatible with standard Buzz clients.'),
		readOnlyGuests: unavailable('The supported relay does not establish guest write denial.')
	};
}

export interface ChatMember {
	pubkey: string;
	role: ChatRole | null;
}

export interface ChatChannelState {
	channelId: string;
	name: string;
	description: string;
	visibility: 'open' | 'private';
	channelType: string | null;
	archived: boolean;
	members: ChatMember[];
	role: ChatRole | null;
}

function roleOf(value: string | undefined): ChatRole | null {
	return value === 'owner' ||
		value === 'admin' ||
		value === 'member' ||
		value === 'guest' ||
		value === 'bot'
		? value
		: null;
}

/** Parse snapshots already verified against the trusted relay key by the transport.
 * Both channel metadata and roster are required; missing state never grants posting. */
export function parseChannelState(
	events: NostrEvent[],
	channelId: string,
	pubkey: string
): ChatChannelState | null {
	const snapshot = (kind: number) =>
		events
			.filter(
				(event) =>
					event.kind === kind && event.tags.some((tag) => tag[0] === 'd' && tag[1] === channelId)
			)
			.sort((a, b) => b.created_at - a.created_at || a.id.localeCompare(b.id))[0];
	const metadata = snapshot(39000);
	const roster = snapshot(39002);
	if (!metadata || !roster) return null;
	const value = (key: string) => metadata.tags.find((tag) => tag[0] === key)?.[1];
	const members = roster.tags
		.filter((tag) => tag[0] === 'p' && /^[0-9a-f]{64}$/.test(tag[1] ?? ''))
		.map((tag) => ({ pubkey: tag[1], role: roleOf(tag[3]) }));
	return {
		channelId,
		name: value('name') ?? channelId,
		description: value('about') ?? '',
		visibility: metadata.tags.some((tag) => tag[0] === 'private') ? 'private' : 'open',
		channelType: value('t') ?? null,
		archived: value('archived') === 'true',
		members,
		role: members.find((member) => member.pubkey === pubkey)?.role ?? null
	};
}

/** Conservative UI eligibility only; Buzz remains the final ingest authority. */
export function canPostToChannel(state: ChatChannelState | null): boolean {
	return (
		state !== null &&
		!state.archived &&
		(state.role === 'owner' ||
			state.role === 'admin' ||
			state.role === 'member' ||
			state.role === 'bot')
	);
}
