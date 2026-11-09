// Channel lifecycle management against the pinned Buzz fork's NIP-29 surface
// (NOSTR.md "What Works" + crates/buzz-core/src/kind.rs). Every mutation is
// signed in the browser with the bound ordinary user identity and confirmed
// against relay-signed discovery events — never against the publish OK alone.
// Discovery reads reuse queryRelayEvents, which already enforces the trusted
// relay signer for kinds 39000–39003, so relay-reported state (including
// private-channel visibility boundaries) is the only state parsed here.
import {
	buildUnsignedEvent,
	publishEvent,
	publishSignedEvent,
	publicKeyOf,
	queryRelayEvents,
	isUuid,
	type NostrEvent,
	type NostrTag,
	type PublishResult,
	type RelayQueryResult
} from './nostr';
import type { ChatRole } from './capabilities';

// NIP-29 moderation/membership kinds supported by the pinned fork
// (crates/buzz-core/src/kind.rs). Guest assignment is deliberately absent:
// guests are read-only and the guest write-denial fix is still in review
// (kubedoio/buzz#8), so no UI may request that role.
export const NOSTR_KIND_CHANNEL_ADD_USER = 9000;
export const NOSTR_KIND_CHANNEL_REMOVE_USER = 9001;
export const NOSTR_KIND_CHANNEL_EDIT_METADATA = 9002;
export const NOSTR_KIND_CHANNEL_CREATE = 9007;
export const NOSTR_KIND_CHANNEL_JOIN = 9021;
export const NOSTR_KIND_CHANNEL_LEAVE = 9022;
export const NOSTR_KIND_GROUP_METADATA = 39000;
export const NOSTR_KIND_GROUP_MEMBERS = 39002;

export interface ChannelDirectoryEntry {
	channelId: string;
	name: string;
	description: string;
	visibility: 'open' | 'private';
	channelType: string | null;
	archived: boolean;
}

export interface ChannelRosterEntry {
	pubkey: string;
	role: ChatRole | null;
}

export type ChannelMutationResult =
	| { status: 'confirmed' }
	| { status: 'unconfirmed'; detail?: string }
	| { status: 'uncertain'; event: NostrEvent }
	| { status: 'rejected'; detail?: string }
	| { status: 'transport' };

export interface ChannelMutationInput {
	relayUrl: string;
	secretKey: string;
	relayPubkey: string;
	/** Receives the relay-state confirmation closure so an uncertain retry can
	 * re-verify the same outcome instead of trusting the retried OK frame. */
	onConfirm?: (confirm: () => Promise<boolean>) => void;
}

const PUBKEY_HEX = /^[0-9a-f]{64}$/;

function roleOf(value: string | undefined): ChatRole | null {
	return value === 'owner' ||
		value === 'admin' ||
		value === 'member' ||
		value === 'guest' ||
		value === 'bot'
		? value
		: null;
}

function tagValue(event: NostrEvent, name: string): string | undefined {
	return event.tags.find((tag) => tag[0] === name)?.[1];
}

/** Newest snapshot per channel id; on equal created_at the lowest event id
 *  wins — the same tie-break parseChannelState applies in capabilities.ts, so
 *  both newest-snapshot selectors agree deterministically. */
function latestSnapshots(events: NostrEvent[], kind: number): Map<string, NostrEvent> {
	const snapshots = new Map<string, NostrEvent>();
	for (const event of events) {
		if (event.kind !== kind) continue;
		const channelId = tagValue(event, 'd');
		if (!channelId) continue;
		const current = snapshots.get(channelId);
		if (
			!current ||
			event.created_at > current.created_at ||
			(event.created_at === current.created_at && event.id < current.id)
		)
			snapshots.set(channelId, event);
	}
	return snapshots;
}

/**
 * All channels the relay reports for this identity, including channels with
 * zero messages (directory membership is metadata-driven, not history-driven).
 * Hidden DM channels and archived channels stay out of the sidebar; access
 * control already happened at the relay, so a private channel either appears
 * (caller may see it) or never reaches this parser.
 *
 * Channel type is read from the `t` tag, which the pinned fork
 * (c9a373b, `side_effects.rs::emit_group_discovery_events`) ALWAYS emits on
 * kind 39000 as `["t", channel.channel_type]` with the value stored at the
 * kind 9007 ingest (`channel_type` tag, defaulting to "stream") — NOSTR.md's
 * 39000 tag table omits it, but the fork source is authoritative here. The
 * `?? null` fallback therefore only covers metadata events from other relays.
 */
export function parseChannelDirectory(events: NostrEvent[]): ChannelDirectoryEntry[] {
	const entries: ChannelDirectoryEntry[] = [];
	for (const [channelId, metadata] of latestSnapshots(events, NOSTR_KIND_GROUP_METADATA)) {
		if (metadata.tags.some((tag) => tag[0] === 'hidden')) continue;
		if (tagValue(metadata, 'archived') === 'true') continue;
		entries.push({
			channelId,
			name: tagValue(metadata, 'name') ?? channelId,
			description: tagValue(metadata, 'about') ?? '',
			visibility: metadata.tags.some((tag) => tag[0] === 'private') ? 'private' : 'open',
			channelType: tagValue(metadata, 't') ?? null,
			archived: false
		});
	}
	return entries.sort((a, b) => a.name.localeCompare(b.name));
}

/** Member roster from the newest relay-signed 39002 snapshot for a channel. */
export function parseChannelRoster(events: NostrEvent[], channelId: string): ChannelRosterEntry[] {
	const roster = latestSnapshots(events, NOSTR_KIND_GROUP_MEMBERS).get(channelId);
	if (!roster) return [];
	return roster.tags
		.filter((tag) => tag[0] === 'p' && PUBKEY_HEX.test(tag[1] ?? ''))
		.map((tag) => ({ pubkey: tag[1], role: roleOf(tag[3]) }));
}

/** The fork only emits `about` when the description is non-empty. */
function metadataMatches(snapshot: NostrEvent, name: string, description: string): boolean {
	return tagValue(snapshot, 'name') === name && (tagValue(snapshot, 'about') ?? '') === description;
}

async function queryChannelSnapshot(
	input: ChannelMutationInput,
	channelId: string,
	kinds: number[]
): Promise<RelayQueryResult> {
	return queryRelayEvents(
		input.relayUrl,
		{ kinds, '#d': [channelId] },
		input.secretKey,
		input.relayPubkey
	);
}

async function settle(
	published: PublishResult,
	confirm: () => Promise<boolean>
): Promise<ChannelMutationResult> {
	if (published.ok) return (await confirm()) ? { status: 'confirmed' } : { status: 'unconfirmed' };
	if (published.reason === 'uncertain') return { status: 'uncertain', event: published.event };
	if (published.reason === 'rejected') return { status: 'rejected', detail: published.detail };
	return { status: 'transport' };
}

/**
 * Retry an uncertain mutation with the exact signed bytes the first attempt
 * produced — re-signing would change the event id and could apply the same
 * logical operation twice. The relay deduplicates by event id, so resending
 * the identical event is safe and confirms the original outcome.
 */
export async function retryChannelMutation(
	input: ChannelMutationInput,
	signed: NostrEvent,
	confirm: () => Promise<boolean>
): Promise<ChannelMutationResult> {
	return settle(await publishSignedEvent(input.relayUrl, signed, input.secretKey), confirm);
}

export interface CreateChannelOptions {
	name: string;
	description?: string;
	visibility: 'open' | 'private';
}

export interface CreateChannelResult {
	channelId: string;
	result: ChannelMutationResult;
}

/**
 * Create a channel as the bound user. The client supplies the channel UUID in
 * the `h` tag (the fork pre-creates the channel with that id at ingest), so
 * the id is known before publish and creation is confirmed by re-querying the
 * relay-signed 39000 snapshot — the OK frame alone is not authoritative.
 */
export async function createChannel(
	input: ChannelMutationInput,
	options: CreateChannelOptions
): Promise<CreateChannelResult> {
	const name = options.name.trim();
	const description = (options.description ?? '').trim();
	const channelId = crypto.randomUUID();
	const tags: NostrTag[] = [
		['h', channelId],
		['name', name],
		['visibility', options.visibility],
		['channel_type', 'stream']
	];
	if (description) tags.push(['about', description]);
	const unsigned = await buildUnsignedEvent(
		NOSTR_KIND_CHANNEL_CREATE,
		'',
		tags,
		publicKeyOf(input.secretKey)
	);
	const confirm = async () => {
		const snapshot = await queryChannelSnapshot(input, channelId, [NOSTR_KIND_GROUP_METADATA]);
		return snapshot.ok
			? latestSnapshots(snapshot.events, NOSTR_KIND_GROUP_METADATA)
					.values()
					.some((event) => metadataMatches(event, name, description))
			: false;
	};
	input.onConfirm?.(confirm);
	const published = await publishEvent(input.relayUrl, unsigned, input.secretKey);
	const result = await settle(published, confirm);
	return { channelId, result };
}

export interface EditChannelOptions {
	name?: string;
	description?: string;
}

/**
 * Edit name/description via the fork's kind 9002 (name/about are privileged
 * tags: owner/admin per the fork's validator). Confirmed against a fresh
 * 39000 snapshot matching the requested values.
 */
export async function editChannel(
	input: ChannelMutationInput,
	channelId: string,
	options: EditChannelOptions
): Promise<ChannelMutationResult> {
	const name = options.name?.trim();
	const description = options.description?.trim();
	if (!name && description === undefined)
		return { status: 'rejected', detail: 'Nothing to change' };
	const tags: NostrTag[] = [['h', channelId]];
	if (name) tags.push(['name', name]);
	if (description !== undefined) tags.push(['about', description]);
	const unsigned = await buildUnsignedEvent(
		NOSTR_KIND_CHANNEL_EDIT_METADATA,
		'',
		tags,
		publicKeyOf(input.secretKey)
	);
	const confirm = async () => {
		const snapshot = await queryChannelSnapshot(input, channelId, [NOSTR_KIND_GROUP_METADATA]);
		if (!snapshot.ok) return false;
		const current = latestSnapshots(snapshot.events, NOSTR_KIND_GROUP_METADATA).get(channelId);
		if (!current) return false;
		if (name !== undefined && tagValue(current, 'name') !== name) return false;
		if (description !== undefined && (tagValue(current, 'about') ?? '') !== description)
			return false;
		return true;
	};
	input.onConfirm?.(confirm);
	return settle(await publishEvent(input.relayUrl, unsigned, input.secretKey), confirm);
}

/**
 * Add a member via kind 9000. No role tag is sent, so the fork defaults new
 * members to `member` and preserves an existing member's role; elevated and
 * guest grants are intentionally not offered. Confirmed against a fresh,
 * relay-signed 39002 roster containing the target — an EMPTY roster snapshot
 * is *unconfirmed* here: for an add the expected state is the target's
 * visible membership, so "no roster" can never mean success.
 */
export async function addChannelMember(
	input: ChannelMutationInput,
	channelId: string,
	targetPubkey: string
): Promise<ChannelMutationResult> {
	if (!PUBKEY_HEX.test(targetPubkey)) return { status: 'rejected', detail: 'Invalid pubkey' };
	const unsigned = await buildUnsignedEvent(
		NOSTR_KIND_CHANNEL_ADD_USER,
		'',
		[
			['h', channelId],
			['p', targetPubkey]
		],
		publicKeyOf(input.secretKey)
	);
	const confirm = async () => {
		const snapshot = await queryChannelSnapshot(input, channelId, [NOSTR_KIND_GROUP_MEMBERS]);
		return (
			snapshot.ok &&
			parseChannelRoster(snapshot.events, channelId).some(
				(member) => member.pubkey === targetPubkey
			)
		);
	};
	input.onConfirm?.(confirm);
	return settle(await publishEvent(input.relayUrl, unsigned, input.secretKey), confirm);
}

/** Remove a member via kind 9001; confirmed when the fresh roster omits them.
 *  An EMPTY roster snapshot intentionally counts as confirmation here (unlike
 *  add/join): the fork sends the publish OK only after the removal's side
 *  effects are awaited, and after a private-channel removal the roster is
 *  legitimately unreadable to the removed identity — emptiness means "no
 *  visible members", never "removal not applied". */
export async function removeChannelMember(
	input: ChannelMutationInput,
	channelId: string,
	targetPubkey: string
): Promise<ChannelMutationResult> {
	const unsigned = await buildUnsignedEvent(
		NOSTR_KIND_CHANNEL_REMOVE_USER,
		'',
		[
			['h', channelId],
			['p', targetPubkey]
		],
		publicKeyOf(input.secretKey)
	);
	const confirm = async () => {
		const snapshot = await queryChannelSnapshot(input, channelId, [NOSTR_KIND_GROUP_MEMBERS]);
		return (
			snapshot.ok &&
			!parseChannelRoster(snapshot.events, channelId).some(
				(member) => member.pubkey === targetPubkey
			)
		);
	};
	input.onConfirm?.(confirm);
	return settle(await publishEvent(input.relayUrl, unsigned, input.secretKey), confirm);
}

/** Self-join an open channel via kind 9021 (private joins are relay-rejected).
 *  Like addChannelMember, an empty roster snapshot is unconfirmed: the
 *  expected state is your own visible membership. */
export async function joinChannel(
	input: ChannelMutationInput,
	channelId: string
): Promise<ChannelMutationResult> {
	const pubkey = publicKeyOf(input.secretKey);
	const unsigned = await buildUnsignedEvent(
		NOSTR_KIND_CHANNEL_JOIN,
		'',
		[['h', channelId]],
		pubkey
	);
	const confirm = async () => {
		const snapshot = await queryChannelSnapshot(input, channelId, [NOSTR_KIND_GROUP_MEMBERS]);
		return (
			snapshot.ok &&
			parseChannelRoster(snapshot.events, channelId).some((member) => member.pubkey === pubkey)
		);
	};
	input.onConfirm?.(confirm);
	return settle(await publishEvent(input.relayUrl, unsigned, input.secretKey), confirm);
}

/** Leave a channel via kind 9022; confirmed when the fresh roster omits you.
 *  Same empty-snapshot rationale as removeChannelMember: the OK is sent only
 *  after awaited side effects, and a private self-leave makes the roster
 *  legitimately unreadable to you afterwards. */
export async function leaveChannel(
	input: ChannelMutationInput,
	channelId: string
): Promise<ChannelMutationResult> {
	const pubkey = publicKeyOf(input.secretKey);
	const unsigned = await buildUnsignedEvent(
		NOSTR_KIND_CHANNEL_LEAVE,
		'',
		[['h', channelId]],
		pubkey
	);
	const confirm = async () => {
		const snapshot = await queryChannelSnapshot(input, channelId, [NOSTR_KIND_GROUP_MEMBERS]);
		return (
			snapshot.ok &&
			!parseChannelRoster(snapshot.events, channelId).some((member) => member.pubkey === pubkey)
		);
	};
	input.onConfirm?.(confirm);
	return settle(await publishEvent(input.relayUrl, unsigned, input.secretKey), confirm);
}

export type ChannelDirectoryResult =
	| { ok: true; channels: ChannelDirectoryEntry[] }
	| { ok: false; reason: 'transport' | 'rejected' | 'invalid' };

/**
 * Directory of every channel visible to this identity, including empty ones.
 * A single kinds-[39000] historical REQ is the fork's group-discovery query;
 * the relay applies access control before returning, so private channels the
 * caller may not see never appear in the result (NOSTR.md "Group Discovery").
 * The REQ inherits queryRelayEvents' default limit of 1000 events (no
 * pagination), so a directory saturates at the newest 1000 metadata snapshots.
 */
export async function listVisibleChannels(
	input: ChannelMutationInput
): Promise<ChannelDirectoryResult> {
	const result = await queryRelayEvents(
		input.relayUrl,
		{ kinds: [NOSTR_KIND_GROUP_METADATA] },
		input.secretKey,
		input.relayPubkey
	);
	if (!result.ok) return { ok: false, reason: result.reason };
	return { ok: true, channels: parseChannelDirectory(result.events) };
}

/** name/about edits: owner/admin only (privileged 9002 tags in the fork). */
export function canEditChannelMetadata(role: ChatRole | null): boolean {
	return role === 'owner' || role === 'admin';
}

/**
 * Member adds per the fork's 9000 rules: private channels — owner/admin;
 * open channels — any full member (guests are read-only). Target always
 * joins as `member`; no guest or elevated assignment is offered.
 */
export function canAddMembers(role: ChatRole | null, visibility: 'open' | 'private'): boolean {
	if (role === 'guest' || role === null) return false;
	return visibility === 'open' || role === 'owner' || role === 'admin';
}

/** Removing someone else: owner/admin only (self-removal is the leave flow). */
export function canRemoveMembers(role: ChatRole | null): boolean {
	return role === 'owner' || role === 'admin';
}

/** Join is offered only where the relay accepts it: open, non-archived, not already a member. */
export function canJoinChannel(
	entry: { visibility: 'open' | 'private'; archived: boolean },
	memberRole: ChatRole | null
): boolean {
	return entry.visibility === 'open' && !entry.archived && memberRole === null;
}

/** True when leaving would orphan the channel (the fork rejects last-owner leave). */
export function isLastOwner(roster: ChannelRosterEntry[], pubkey: string): boolean {
	return (
		roster.some((member) => member.pubkey === pubkey && member.role === 'owner') &&
		roster.filter((member) => member.role === 'owner').length <= 1
	);
}

/** Channel ids are relay-assigned UUIDs; directory entries must carry one. */
export function isChannelId(value: string): boolean {
	return isUuid(value);
}
