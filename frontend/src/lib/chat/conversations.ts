// Conversation threads, reactions and authorized deletion against the pinned
// Buzz fork's NIP-29/NIP-10 surface (NOSTR.md "What Works" + "Canonical
// Publish Tags" at c9a373b). Every mutation is signed in the browser with the
// bound Chat identity and confirmed against relay state — never the publish
// OK alone. All wire semantics below were verified live against the pinned
// image (ghcr.io/kubedoio/buzz-elembra@sha256:2b6c4b2a…) on 2026-11-09:
// canonical root+reply tagging, bare-reply rejection on threaded parents,
// duplicate-reaction rejection, NIP-09 reaction removal, self/admin delete
// authorization, in-channel tombstone system messages.
import {
	buildUnsignedEvent,
	publishEvent,
	publishSignedEvent,
	publicKeyOf,
	queryRelayEvents,
	NOSTR_KIND_STREAM_MESSAGE,
	type NostrEvent,
	type NostrTag,
	type PublishResult,
	type RelayQueryResult
} from './nostr';
import type { ChatRole } from './capabilities';

// NIP-25 reaction; NIP-09 event deletion (self-authored targets only);
// NIP-29 delete-event (channel owner/admin); relay-signed in-channel system
// message (the fork's `message_deleted` tombstone, side_effects.rs).
export const NOSTR_KIND_DELETION = 5;
export const NOSTR_KIND_REACTION = 7;
export const NOSTR_KIND_ADMIN_DELETE_EVENT = 9005;
export const NOSTR_KIND_CHANNEL_SYSTEM = 40099;

/** Palette offered by the message actions menu. */
export const REACTION_EMOJIS = ['👍', '🎉', '❤️', '🙏', '👀'] as const;

const HEX64 = /^[0-9a-f]{64}$/;
// Lowercase-only on purpose: every Nostr event id and pubkey this client
// produces or consumes is lowercase hex by construction. The pinned relay
// parses tag hex case-insensitively (is_ascii_hexdigit), so mixed-case tag
// values would still resolve there — but accepting them here would only
// widen the parser surface for ids the rest of the pipeline never emits.

// ---------------------------------------------------------------------------
// NIP-10 thread identity
// ---------------------------------------------------------------------------

export interface ThreadParentRef {
	eventId: string;
	/** Thread root of the parent; null/omitted when the parent is itself a root. */
	rootId?: string | null;
}

/**
 * Canonical reply tags per the fork's "Canonical Publish Tags": the relay
 * reads only the event id (position 1) and the marker (position 3); the relay
 * URL position is optional and omitted. The claimed root must match the
 * parent's stored ancestry, so the client always derives the root from the
 * parent's own tags (parent.rootId when present, else the parent itself) —
 * a bare `reply` tag is only valid for parents that have no thread ancestry.
 * Returns null when either id is not a lowercase 64-hex event id.
 */
export function buildReplyTags(parent: ThreadParentRef): NostrTag[] | null {
	if (!HEX64.test(parent.eventId)) return null;
	const root = parent.rootId ?? parent.eventId;
	if (!HEX64.test(root)) return null;
	return [
		['e', root, '', 'root'],
		['e', parent.eventId, '', 'reply']
	];
}

export interface ThreadRefs {
	rootId: string | null;
	parentId: string | null;
}

/**
 * Thread markers on a stored event: last root/reply marker wins, matching the
 * relay's resolve_nip10_thread_meta scan. Tags shorter than 4 parts or with
 * non-hex ids are ignored (the relay ignores them too).
 */
export function parseThreadRefs(event: NostrEvent): ThreadRefs {
	let rootId: string | null = null;
	let parentId: string | null = null;
	for (const tag of event.tags) {
		if (tag[0] !== 'e' || tag.length < 4 || !HEX64.test(tag[1])) continue;
		if (tag[3] === 'root') rootId = tag[1];
		else if (tag[3] === 'reply') parentId = tag[1];
	}
	return { rootId, parentId };
}

/**
 * The thread root a message belongs to for display grouping. Accepted events
 * always carry a root tag except bare replies (parent without ancestry), for
 * which the parent is the root of its own thread; channel-level messages
 * return null.
 */
export function threadRootOf(event: NostrEvent): string | null {
	const { rootId, parentId } = parseThreadRefs(event);
	return rootId ?? parentId;
}

/**
 * Replies belonging to the thread rooted at `rootId`, oldest first
 * (created_at, then id — the same deterministic tie-break the channel
 * parsers use). The root event itself is excluded.
 */
export function threadReplies(events: NostrEvent[], rootId: string): NostrEvent[] {
	return events
		.filter((event) => event.id !== rootId && threadRootOf(event) === rootId)
		.sort((a, b) => a.created_at - b.created_at || a.id.localeCompare(b.id));
}

// ---------------------------------------------------------------------------
// Reactions (kind 7)
// ---------------------------------------------------------------------------

export interface ReactionGroup {
	targetId: string;
	emoji: string;
	count: number;
	/** Unique author pubkeys, sorted for deterministic rendering. */
	authors: string[];
	/** The viewer's own kind-7 event id for this target+emoji, if reacted. */
	myEventId: string | null;
}

/** Display clamp for reaction content: user input rendered verbatim in the
 *  timeline, so cap it at a few grapheme clusters (relay-side limit is far
 *  higher) and fall back to '+' for unusable payloads. */
const REACTION_DISPLAY_GRAPHEMES = 4;

export function clampReactionEmoji(content: string): string {
	if (content === '') return '+';
	let graphemes: string[];
	if (typeof Intl?.Segmenter === 'function') {
		graphemes = [...new Intl.Segmenter('en', { granularity: 'grapheme' }).segment(content)].map(
			(part) => part.segment
		);
	} else {
		graphemes = Array.from(content);
	}
	const clamped = graphemes.slice(0, REACTION_DISPLAY_GRAPHEMES).join('');
	return clamped === '' ? '+' : clamped;
}

/**
 * Aggregate kind-7 events into per-target, per-emoji groups. Input events
 * must already have passed the transport's signature/kind validation; the
 * target is the LAST e tag (NIP-25, matching the relay's ingest). The relay
 * dedupes one reaction per (target, actor, emoji), but grouping stays correct
 * even if a relay returns duplicates.
 */
export function parseReactions(
	events: NostrEvent[],
	viewerPubkey: string | null
): Map<string, ReactionGroup[]> {
	const groups = new Map<string, ReactionGroup>();
	for (const event of events) {
		if (event.kind !== NOSTR_KIND_REACTION) continue;
		const targetId = [...event.tags].reverse().find((t) => t[0] === 'e' && HEX64.test(t[1]))?.[1];
		if (!targetId) continue;
		const emoji = clampReactionEmoji(event.content);
		const key = `${targetId}|${emoji}`;
		let group = groups.get(key);
		if (!group) {
			group = { targetId, emoji, count: 0, authors: [], myEventId: null };
			groups.set(key, group);
		}
		if (!group.authors.includes(event.pubkey)) {
			group.authors.push(event.pubkey);
			group.count += 1;
		}
		if (viewerPubkey !== null && event.pubkey === viewerPubkey && group.myEventId === null) {
			group.myEventId = event.id;
		}
	}
	const byTarget = new Map<string, ReactionGroup[]>();
	for (const group of groups.values()) {
		group.authors.sort();
		const list = byTarget.get(group.targetId) ?? [];
		list.push(group);
		byTarget.set(group.targetId, list);
	}
	for (const list of byTarget.values()) {
		list.sort((a, b) => a.emoji.localeCompare(b.emoji));
	}
	return byTarget;
}

// ---------------------------------------------------------------------------
// Deletion overlay (kinds 5, 9005, 40099)
// ---------------------------------------------------------------------------

export interface DeletionInfo {
	actor: string;
	via: 'nip09' | 'nip29' | 'system';
}

/**
 * Targets of applied deletions visible to this identity. User-signed kind 5
 * (self-authored targets) and kind 9005 (owner/admin targets) are authorized
 * at the relay's ingest, so their single e-tag target counts as deleted; the
 * relay-signed kind 40099 `message_deleted` tombstone is accepted ONLY from
 * the trusted relay signer — a user-signed lookalike must never drive UI
 * state (queryRelayEvents only enforces the trusted signer for 39000–39003).
 *
 * Defense in depth for kind 5: when `authorOf` is provided, a self-deletion
 * only counts if the deletion signer IS the target's author as known by the
 * timeline. The relay already rejects author-mismatched kind 5 at ingest, so
 * a mismatched entry means an untrustworthy relay response — drop it. Targets
 * whose author the timeline does not know yet are dropped as well (fail
 * closed for user-signed deletions; kind 9005 moderation and relay-signed
 * tombstones keep their relay-side authorization).
 */
export function parseDeletionTargets(
	events: NostrEvent[],
	relayPubkey: string,
	authorOf?: (eventId: string) => string | null
): Map<string, DeletionInfo> {
	const targets = new Map<string, DeletionInfo>();
	for (const event of events) {
		if (event.kind === NOSTR_KIND_DELETION || event.kind === NOSTR_KIND_ADMIN_DELETE_EVENT) {
			const target = event.tags.find((t) => t[0] === 'e' && HEX64.test(t[1]))?.[1];
			if (!target || targets.has(target)) continue;
			const via = event.kind === NOSTR_KIND_DELETION ? 'nip09' : 'nip29';
			if (via === 'nip09' && authorOf) {
				const author = authorOf(target);
				if (author === null || author !== event.pubkey) continue;
			}
			targets.set(target, { actor: event.pubkey, via });
			continue;
		}
		if (event.kind === NOSTR_KIND_CHANNEL_SYSTEM && event.pubkey === relayPubkey) {
			let payload: { type?: unknown; actor?: unknown; target_event_id?: unknown };
			try {
				payload = JSON.parse(event.content);
			} catch {
				continue;
			}
			if (
				payload.type === 'message_deleted' &&
				typeof payload.target_event_id === 'string' &&
				HEX64.test(payload.target_event_id) &&
				!targets.has(payload.target_event_id)
			) {
				targets.set(payload.target_event_id, {
					actor: typeof payload.actor === 'string' ? payload.actor : event.pubkey,
					via: 'system'
				});
			}
		}
	}
	return targets;
}

/**
 * Which delete mode the relay accepts for a message: the author always uses
 * NIP-09 kind 5 (mirrored agent-owner deletion is relay-side, not offered in
 * the UI); channel owner/admin use NIP-29 kind 9005 for others' messages.
 */
export function canDeleteMessage(
	role: ChatRole | null,
	authorPubkey: string,
	viewerPubkey: string
): 'self' | 'admin' | null {
	if (authorPubkey === viewerPubkey) return 'self';
	if (role === 'owner' || role === 'admin') return 'admin';
	return null;
}

// ---------------------------------------------------------------------------
// Relay reads
// ---------------------------------------------------------------------------

export interface ConversationReadInput {
	relayUrl: string;
	secretKey: string;
	relayPubkey: string;
}

/**
 * The conversation overlay for a channel: reactions plus every deletion
 * signal (user-signed requests and relay-signed tombstones). One REQ covers
 * all four kinds; the relay applies channel access control before returning,
 * so a lost membership fails closed here exactly like the roster query.
 */
export type ConversationOverlayResult =
	| { ok: true; events: NostrEvent[]; capped: boolean }
	| { ok: false; reason: 'transport' | 'rejected' | 'invalid'; detail?: string };

/** One page is one transport read (1000-event/1 MiB bounds are the #479
 *  per-connection security cap, unchanged). The aggregate hard bound across
 *  pages stays deliberate: CONVERSATION_OVERLAY_MAX_PAGES × page size. */
export const CONVERSATION_OVERLAY_MAX_PAGES = 5;
const OVERLAY_PAGE_SIZE = 1000;
const OVERLAY_KINDS = [
	NOSTR_KIND_DELETION,
	NOSTR_KIND_REACTION,
	NOSTR_KIND_ADMIN_DELETE_EVENT,
	NOSTR_KIND_CHANNEL_SYSTEM
];

/**
 * The conversation overlay for a channel: reactions plus every deletion
 * signal (user-signed requests and relay-signed tombstones), paginated
 * older-than-oldest (`until` windowing) so channels with more overlay events
 * than a single read's cap still surface everything up to the aggregate
 * bound. The relay applies channel access control before returning, so a
 * lost membership fails closed here exactly like the roster query.
 *
 * `capped` reports a PARTIAL overlay: true when the aggregate page bound was
 * exhausted with more results possibly remaining, when the window stopped
 * making progress (a relay that ignores `until` would loop forever
 * otherwise), or when a later page failed after page one succeeded. Callers
 * must surface the partial state instead of silently dropping it. A failed
 * FIRST page means there is no overlay at all and is propagated as-is.
 *
 * Note: `until` is inclusive per NIP-01, so the next window steps strictly
 * below the page's oldest event; same-`created_at` events beyond a page cut
 * can be skipped — standard Nostr pagination imprecision, guarded by dedupe.
 */
export async function fetchConversationOverlay(
	input: ConversationReadInput,
	channelId: string
): Promise<ConversationOverlayResult> {
	const merged = new Map<string, NostrEvent>();
	let until: number | undefined;
	let oldest: number | null = null;
	for (let page = 0; page < CONVERSATION_OVERLAY_MAX_PAGES; page += 1) {
		const result = await queryRelayEvents(
			input.relayUrl,
			{ kinds: [...OVERLAY_KINDS], '#h': [channelId], limit: OVERLAY_PAGE_SIZE, until },
			input.secretKey,
			input.relayPubkey
		);
		if (!result.ok) {
			return page === 0 ? result : { ok: true, events: [...merged.values()], capped: true };
		}
		for (const event of result.events) {
			if (!merged.has(event.id)) merged.set(event.id, event);
		}
		if (result.events.length < OVERLAY_PAGE_SIZE) {
			return { ok: true, events: [...merged.values()], capped: false };
		}
		const pageOldest = Math.min(...result.events.map((event) => event.created_at));
		// created_at 0 is the floor of the window; there is no older page.
		if (pageOldest <= 0 || (oldest !== null && pageOldest >= oldest)) {
			return { ok: true, events: [...merged.values()], capped: true };
		}
		oldest = pageOldest;
		until = pageOldest - 1;
	}
	// Every page came back full at the aggregate bound: more may exist.
	return { ok: true, events: [...merged.values()], capped: true };
}

/**
 * Thread history rooted at `rootId`: every kind-9 event carrying an e tag of
 * the root (root markers for nested replies, reply markers for direct ones).
 * Deleted events are already filtered out by the relay's query path.
 */
export function fetchThreadHistory(
	input: ConversationReadInput,
	rootId: string
): Promise<RelayQueryResult> {
	return queryRelayEvents(
		input.relayUrl,
		{ kinds: [NOSTR_KIND_STREAM_MESSAGE], '#e': [rootId] },
		input.secretKey,
		input.relayPubkey
	);
}

// ---------------------------------------------------------------------------
// Mutations (confirm-via-relay-state, same contract as channels.ts)
// ---------------------------------------------------------------------------

export type ConversationMutationResult =
	| { status: 'confirmed' }
	| { status: 'unconfirmed'; detail?: string }
	| { status: 'uncertain'; event: NostrEvent }
	| { status: 'rejected'; detail?: string }
	| { status: 'transport' };

export interface ConversationMutationInput extends ConversationReadInput {
	/** Receives the relay-state confirmation closure so an uncertain retry can
	 *  re-verify the same outcome instead of trusting the retried OK frame. */
	onConfirm?: (confirm: () => Promise<boolean>) => void;
}

async function settle(
	published: PublishResult,
	confirm: () => Promise<boolean>
): Promise<ConversationMutationResult> {
	if (published.ok) return (await confirm()) ? { status: 'confirmed' } : { status: 'unconfirmed' };
	if (published.reason === 'uncertain') return { status: 'uncertain', event: published.event };
	if (published.reason === 'rejected') return { status: 'rejected', detail: published.detail };
	return { status: 'transport' };
}

/**
 * Retry an uncertain conversation mutation with the exact signed bytes the
 * first attempt produced — the relay deduplicates by event id, so the retry
 * confirms the original outcome and can never apply it twice.
 */
export async function retryConversationMutation(
	input: ConversationMutationInput,
	signed: NostrEvent,
	confirm: () => Promise<boolean>
): Promise<ConversationMutationResult> {
	return settle(await publishSignedEvent(input.relayUrl, signed, input.secretKey), confirm);
}

/**
 * Reply to a message: kind 9 with the channel `h` tag and canonical NIP-10
 * root/reply tags (buildReplyTags). Confirmed when a fresh `#e` history query
 * on the thread root contains the exact new event id.
 */
export async function sendReply(
	input: ConversationMutationInput,
	channelId: string,
	parent: ThreadParentRef,
	content: string
): Promise<ConversationMutationResult> {
	const threadTags = buildReplyTags(parent);
	if (!threadTags) return { status: 'rejected', detail: 'Invalid thread parent' };
	const unsigned = await buildUnsignedEvent(
		NOSTR_KIND_STREAM_MESSAGE,
		content,
		[['h', channelId], ...threadTags],
		publicKeyOf(input.secretKey)
	);
	let publishedId: string | null = null;
	const confirm = async () => {
		if (!publishedId) return false;
		const history = await fetchThreadHistory(input, parent.rootId ?? parent.eventId);
		return history.ok && history.events.some((event) => event.id === publishedId);
	};
	input.onConfirm?.(confirm);
	return settle(
		await publishEvent(input.relayUrl, unsigned, input.secretKey, (event) => {
			publishedId = event.id;
		}),
		confirm
	);
}

/**
 * React to a message: kind 7 with `h` (recommended; the relay derives the
 * channel from the target) and a single `e` target tag. The relay dedupes one
 * reaction per (target, actor, emoji) and rejects duplicates — toggling off
 * goes through removeReaction. Confirmed when a fresh `#e` reaction query
 * contains the exact new kind-7 event id.
 */
export async function sendReaction(
	input: ConversationMutationInput,
	channelId: string,
	targetId: string,
	emoji: string
): Promise<ConversationMutationResult> {
	if (!HEX64.test(targetId)) return { status: 'rejected', detail: 'Invalid reaction target' };
	const unsigned = await buildUnsignedEvent(
		NOSTR_KIND_REACTION,
		emoji,
		[
			['h', channelId],
			['e', targetId]
		],
		publicKeyOf(input.secretKey)
	);
	let publishedId: string | null = null;
	const confirm = async () => {
		if (!publishedId) return false;
		const reactions = await queryRelayEvents(
			input.relayUrl,
			{ kinds: [NOSTR_KIND_REACTION], '#e': [targetId] },
			input.secretKey,
			input.relayPubkey
		);
		return reactions.ok && reactions.events.some((e) => e.id === publishedId);
	};
	input.onConfirm?.(confirm);
	return settle(
		await publishEvent(input.relayUrl, unsigned, input.secretKey, (event) => {
			publishedId = event.id;
		}),
		confirm
	);
}

/**
 * Remove a reaction: NIP-09 kind 5 targeting the reaction's own kind-7 event
 * (deletions must reference exactly one target). The relay soft-deletes the
 * reaction event and removes its reaction row. Confirmed when the reaction
 * event id no longer appears in the target's `#e` reaction query.
 */
export async function removeReaction(
	input: ConversationMutationInput,
	channelId: string,
	reaction: { eventId: string; targetId: string }
): Promise<ConversationMutationResult> {
	if (!HEX64.test(reaction.eventId) || !HEX64.test(reaction.targetId)) {
		return { status: 'rejected', detail: 'Invalid reaction reference' };
	}
	const unsigned = await buildUnsignedEvent(
		NOSTR_KIND_DELETION,
		'',
		[
			['h', channelId],
			['e', reaction.eventId]
		],
		publicKeyOf(input.secretKey)
	);
	const confirm = async () => {
		const reactions = await queryRelayEvents(
			input.relayUrl,
			{ kinds: [NOSTR_KIND_REACTION], '#e': [reaction.targetId] },
			input.secretKey,
			input.relayPubkey
		);
		return reactions.ok && !reactions.events.some((e) => e.id === reaction.eventId);
	};
	input.onConfirm?.(confirm);
	return settle(await publishEvent(input.relayUrl, unsigned, input.secretKey), confirm);
}

/**
 * Delete a message. `self` publishes NIP-09 kind 5 (relay-enforced
 * self-author rule); `admin` publishes NIP-29 kind 9005 (relay-enforced
 * owner/admin rule, same channel only). Deletion events must reference
 * exactly one target. Confirmed when an `ids` query for the target returns
 * no live event — the relay's query path filters soft-deleted rows.
 */
export async function deleteMessage(
	input: ConversationMutationInput,
	channelId: string,
	targetEventId: string,
	mode: 'self' | 'admin'
): Promise<ConversationMutationResult> {
	if (!HEX64.test(targetEventId)) return { status: 'rejected', detail: 'Invalid delete target' };
	const unsigned = await buildUnsignedEvent(
		mode === 'self' ? NOSTR_KIND_DELETION : NOSTR_KIND_ADMIN_DELETE_EVENT,
		'',
		[
			['h', channelId],
			['e', targetEventId]
		],
		publicKeyOf(input.secretKey)
	);
	const confirm = async () => {
		const probe = await queryRelayEvents(
			input.relayUrl,
			{ kinds: [NOSTR_KIND_STREAM_MESSAGE], ids: [targetEventId] },
			input.secretKey,
			input.relayPubkey
		);
		return probe.ok && !probe.events.some((e) => e.id === targetEventId);
	};
	input.onConfirm?.(confirm);
	return settle(await publishEvent(input.relayUrl, unsigned, input.secretKey), confirm);
}

// ---------------------------------------------------------------------------
// Draft scoping
// ---------------------------------------------------------------------------

/**
 * Draft storage key, scoped per account (bound pubkey), channel, and thread
 * root (null = channel-level draft). Same shape the composer has always used,
 * with the thread dimension appended; legacy three-element keys are read as
 * channel-level drafts by the composer for upgrade compatibility.
 */
export function draftStorageKey(
	relayUrl: string,
	boundPubkey: string,
	channelId: string,
	threadRootId: string | null
): string {
	return `elembra.chat.draft:${JSON.stringify([relayUrl, boundPubkey, channelId, threadRootId])}`;
}
