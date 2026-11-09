import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { NostrEvent, RelayQueryResult } from './nostr';
import {
	CONVERSATION_OVERLAY_MAX_PAGES,
	NOSTR_KIND_ADMIN_DELETE_EVENT,
	NOSTR_KIND_CHANNEL_SYSTEM,
	NOSTR_KIND_DELETION,
	NOSTR_KIND_REACTION,
	buildReplyTags,
	canDeleteMessage,
	draftStorageKey,
	fetchConversationOverlay,
	parseDeletionTargets,
	parseReactions,
	parseThreadRefs,
	threadReplies,
	threadRootOf
} from './conversations';

// Only the overlay pagination talks to the transport; everything else under
// test here is pure parsing over in-memory events.
const relayMocks = vi.hoisted(() => ({ queryRelayEvents: vi.fn() }));

vi.mock('./nostr', async (importOriginal) => {
	const actual = await importOriginal<typeof import('./nostr')>();
	return { ...actual, queryRelayEvents: relayMocks.queryRelayEvents };
});

const relayPubkey = 'b'.repeat(64);
const alice = 'a'.repeat(64);
const bob = 'c'.repeat(64);
const rootId = '1'.repeat(64);
const parentId = '2'.repeat(64);
const replyId = '3'.repeat(64);

function event(
	id: string,
	kind: number,
	pubkey: string,
	tags: string[][],
	content = '',
	created_at = 1
): NostrEvent {
	return { id, kind, pubkey, tags, content, created_at, sig: '' };
}

describe('buildReplyTags', () => {
	it('emits the canonical root+reply markers, using the parent as root when it has none', () => {
		expect(buildReplyTags({ eventId: parentId })).toEqual([
			['e', parentId, '', 'root'],
			['e', parentId, '', 'reply']
		]);
	});

	it('threads nested replies under the stored root', () => {
		expect(buildReplyTags({ eventId: replyId, rootId })).toEqual([
			['e', rootId, '', 'root'],
			['e', replyId, '', 'reply']
		]);
	});

	it('rejects malformed event ids instead of publishing broken ancestry', () => {
		expect(buildReplyTags({ eventId: 'not-hex' })).toBeNull();
		expect(buildReplyTags({ eventId: parentId, rootId: 'BAD' })).toBeNull();
	});
});

describe('parseThreadRefs / threadRootOf', () => {
	it('reads root and reply markers with last-one-wins semantics', () => {
		const e = event(replyId, 9, alice, [
			['e', '9'.repeat(64), '', 'root'],
			['e', rootId, '', 'root'],
			['e', '8'.repeat(64), '', 'reply'],
			['e', parentId, '', 'reply']
		]);
		expect(parseThreadRefs(e)).toEqual({ rootId, parentId });
		expect(threadRootOf(e)).toBe(rootId);
	});

	it('treats a bare reply marker as its own thread root', () => {
		const e = event(replyId, 9, alice, [['e', parentId, '', 'reply']]);
		expect(parseThreadRefs(e)).toEqual({ rootId: null, parentId });
		expect(threadRootOf(e)).toBe(parentId);
	});

	it('ignores marker-less e tags, short tags, and non-hex ids like the relay does', () => {
		const e = event(replyId, 9, alice, [
			['e', rootId],
			['e', rootId, ''],
			['e', 'zz' + '9'.repeat(62), '', 'root'],
			['e', parentId, '', 'reply']
		]);
		expect(parseThreadRefs(e)).toEqual({ rootId: null, parentId });
	});

	it('returns null root for channel-level messages', () => {
		expect(
			threadRootOf(event(rootId, 9, alice, [['h', '11111111-1111-4111-8111-111111111111']]))
		).toBeNull();
	});
});

describe('threadReplies', () => {
	const root = event(rootId, 9, alice, [], 'root');
	const direct = event('4'.repeat(64), 9, bob, [['e', rootId, '', 'reply']], 'direct', 2);
	const nested = event(
		'5'.repeat(64),
		9,
		alice,
		[
			['e', rootId, '', 'root'],
			['e', direct.id, '', 'reply']
		],
		'nested',
		3
	);
	const otherThread = event(
		'6'.repeat(64),
		9,
		bob,
		[
			['e', '7'.repeat(64), '', 'root'],
			['e', '7'.repeat(64), '', 'reply']
		],
		'elsewhere',
		4
	);

	it('groups direct and nested replies under the root, excluding the root itself', () => {
		const replies = threadReplies([root, nested, otherThread, direct], rootId);
		expect(replies.map((e) => e.id)).toEqual([direct.id, nested.id]);
	});

	it('sorts deterministically by created_at with an id tie-break', () => {
		const a = event('a'.repeat(64), 9, alice, [['e', rootId, '', 'reply']], 'a', 5);
		const b = event('0'.repeat(64), 9, alice, [['e', rootId, '', 'reply']], 'b', 5);
		const replies = threadReplies([b, a], rootId);
		expect(replies.map((e) => e.id)).toEqual(['0'.repeat(64), 'a'.repeat(64)]);
	});
});

describe('parseReactions', () => {
	const react = (id: string, author: string, target: string, emoji: string): NostrEvent =>
		event(id, NOSTR_KIND_REACTION, author, [['e', target]], emoji);

	it('groups by target and emoji with unique authors and the viewer’s own event id', () => {
		const events = [
			react('1'.repeat(64), alice, rootId, '👍'),
			react('2'.repeat(64), bob, rootId, '👍'),
			react('3'.repeat(64), alice, rootId, '🎉'),
			react('4'.repeat(64), bob, parentId, '👍')
		];
		const byTarget = parseReactions(events, alice);
		expect(byTarget.get(rootId)).toEqual([
			{ targetId: rootId, emoji: '🎉', count: 1, authors: [alice], myEventId: '3'.repeat(64) },
			{ targetId: rootId, emoji: '👍', count: 2, authors: [alice, bob], myEventId: '1'.repeat(64) }
		]);
		expect(byTarget.get(parentId)).toEqual([
			{ targetId: parentId, emoji: '👍', count: 1, authors: [bob], myEventId: null }
		]);
	});

	it('uses the last e tag as the target (NIP-25) and + for empty content', () => {
		const e = event(
			'9'.repeat(64),
			NOSTR_KIND_REACTION,
			alice,
			[
				['e', parentId],
				['e', rootId]
			],
			''
		);
		const byTarget = parseReactions([e], alice);
		expect(byTarget.get(rootId)?.[0].emoji).toBe('+');
		expect(byTarget.has(parentId)).toBe(false);
	});

	it('clamps reaction content to a few graphemes for display', () => {
		const long = event(
			'8'.repeat(64),
			NOSTR_KIND_REACTION,
			alice,
			[['e', rootId]],
			'🎉'.repeat(20) + ' spam payload'
		);
		const skinTone = event('7'.repeat(64), NOSTR_KIND_REACTION, alice, [['e', rootId]], '👍🏽');
		const byTarget = parseReactions([long, skinTone], alice);
		const emojis = (byTarget.get(rootId) ?? []).map((group) => group.emoji);
		// The 20-party string clamps to 4 graphemes; 👍🏽 is one grapheme
		// (two code points) and stays intact.
		expect(emojis).toContain('🎉🎉🎉🎉');
		expect(emojis).toContain('👍🏽');
	});

	it('ignores non-reaction kinds and author-duplicates within a group', () => {
		const events = [
			react('1'.repeat(64), alice, rootId, '👍'),
			react('2'.repeat(64), alice, rootId, '👍'),
			event('3'.repeat(64), NOSTR_KIND_DELETION, alice, [['e', rootId]])
		];
		const byTarget = parseReactions(events, alice);
		expect(byTarget.get(rootId)).toEqual([
			{ targetId: rootId, emoji: '👍', count: 1, authors: [alice], myEventId: '1'.repeat(64) }
		]);
	});
});

describe('parseDeletionTargets', () => {
	it('collects kind 5 and 9005 e-tag targets with their authorization path', () => {
		const events = [
			event('1'.repeat(64), NOSTR_KIND_DELETION, alice, [['e', rootId]], 'undo'),
			event('2'.repeat(64), NOSTR_KIND_ADMIN_DELETE_EVENT, alice, [['e', parentId]]),
			event('3'.repeat(64), NOSTR_KIND_ADMIN_DELETE_EVENT, alice, [['e', 'not-hex']])
		];
		const targets = parseDeletionTargets(events, relayPubkey);
		expect(targets.get(rootId)).toEqual({ actor: alice, via: 'nip09' });
		expect(targets.get(parentId)).toEqual({ actor: alice, via: 'nip29' });
		expect(targets.size).toBe(2);
	});

	it('accepts relay-signed 40099 tombstones and ignores user-signed lookalikes', () => {
		const content = JSON.stringify({
			type: 'message_deleted',
			actor: alice,
			target_event_id: rootId
		});
		const events = [
			event('1'.repeat(64), NOSTR_KIND_CHANNEL_SYSTEM, relayPubkey, [['h', 'x']], content),
			event('2'.repeat(64), NOSTR_KIND_CHANNEL_SYSTEM, alice, [['h', 'x']], content),
			event('3'.repeat(64), NOSTR_KIND_CHANNEL_SYSTEM, relayPubkey, [['h', 'x']], 'not json'),
			event(
				'4'.repeat(64),
				NOSTR_KIND_CHANNEL_SYSTEM,
				relayPubkey,
				[],
				JSON.stringify({ type: 'channel_created' })
			)
		];
		const targets = parseDeletionTargets(events, relayPubkey);
		expect(targets.get(rootId)).toEqual({ actor: alice, via: 'system' });
		expect(targets.size).toBe(1);
	});

	it('drops kind 5 entries whose signer is not the target author when the timeline knows the author', () => {
		const events = [
			// Signer is NOT the author (the relay should have rejected this at
			// ingest, so a mismatched entry means an untrustworthy response).
			event('1'.repeat(64), NOSTR_KIND_DELETION, bob, [['e', rootId]], 'vandalism'),
			// Signer IS the author.
			event('2'.repeat(64), NOSTR_KIND_DELETION, alice, [['e', parentId]], 'undo'),
			// Author unknown to the timeline: fail closed for user-signed deletions.
			event('3'.repeat(64), NOSTR_KIND_DELETION, alice, [['e', replyId]], 'orphan')
		];
		const authorOf = (id: string) => (id === rootId || id === parentId ? alice : null);
		const targets = parseDeletionTargets(events, relayPubkey, authorOf);
		expect(targets.has(rootId)).toBe(false);
		expect(targets.get(parentId)).toEqual({ actor: alice, via: 'nip09' });
		expect(targets.has(replyId)).toBe(false);
	});

	it('keeps kind 9005 and relay-signed tombstones regardless of the author lookup', () => {
		const content = JSON.stringify({
			type: 'message_deleted',
			actor: alice,
			target_event_id: rootId
		});
		const events = [
			event('1'.repeat(64), NOSTR_KIND_ADMIN_DELETE_EVENT, bob, [['e', parentId]]),
			event('2'.repeat(64), NOSTR_KIND_CHANNEL_SYSTEM, relayPubkey, [['h', 'x']], content)
		];
		const targets = parseDeletionTargets(events, relayPubkey, () => null);
		expect(targets.get(parentId)).toEqual({ actor: bob, via: 'nip29' });
		expect(targets.get(rootId)).toEqual({ actor: alice, via: 'system' });
	});
});

describe('canDeleteMessage', () => {
	it('authors delete their own messages, owner/admin delete others’, the rest cannot', () => {
		expect(canDeleteMessage('member', alice, alice)).toBe('self');
		expect(canDeleteMessage('owner', bob, alice)).toBe('admin');
		expect(canDeleteMessage('admin', bob, alice)).toBe('admin');
		expect(canDeleteMessage('member', bob, alice)).toBeNull();
		expect(canDeleteMessage(null, bob, alice)).toBeNull();
	});
});

describe('draftStorageKey', () => {
	it('scopes drafts per account, channel, and thread root', () => {
		const base = draftStorageKey('ws://relay', alice, '11111111-1111-4111-8111-111111111111', null);
		expect(base).toBe(
			`elembra.chat.draft:${JSON.stringify(['ws://relay', alice, '11111111-1111-4111-8111-111111111111', null])}`
		);
		const thread = draftStorageKey(
			'ws://relay',
			alice,
			'11111111-1111-4111-8111-111111111111',
			rootId
		);
		expect(thread).not.toBe(base);
		expect(
			draftStorageKey('ws://relay', bob, '11111111-1111-4111-8111-111111111111', null)
		).not.toBe(base);
	});
});

describe('fetchConversationOverlay pagination', () => {
	const input = { relayUrl: 'ws://relay', secretKey: 'sk', relayPubkey: 'b'.repeat(64) };
	const CHANNEL = '11111111-1111-4111-8111-111111111111';

	/** `count` kind-7 events with unique ids and strictly descending
	 *  created_at starting at `base`, newest-first like a relay page. */
	function pageEvents(count: number, base: number, idPrefix: string): NostrEvent[] {
		return Array.from({ length: count }, (_, i) =>
			event(
				`${idPrefix}${String(i).padStart(4, '0')}${'0'.repeat(60)}`.slice(0, 64),
				7,
				alice,
				[['e', rootId]],
				'+',
				base - i
			)
		);
	}

	function okPage(events: NostrEvent[]): RelayQueryResult {
		return { ok: true, events };
	}

	beforeEach(() => {
		relayMocks.queryRelayEvents.mockReset();
	});

	it('merges multiple pages and windows the next page below the oldest event', async () => {
		const first = pageEvents(1000, 2000, 'aa');
		const second = pageEvents(3, 1000, 'bb');
		relayMocks.queryRelayEvents
			.mockResolvedValueOnce(okPage(first))
			.mockResolvedValueOnce(okPage(second));
		const result = await fetchConversationOverlay(input, CHANNEL);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.capped).toBe(false);
		expect(result.events).toHaveLength(1003);
		expect(relayMocks.queryRelayEvents).toHaveBeenCalledTimes(2);
		// Page one: no window, page size at the transport cap.
		expect(relayMocks.queryRelayEvents.mock.calls[0][1]).toMatchObject({
			kinds: [
				NOSTR_KIND_DELETION,
				NOSTR_KIND_REACTION,
				NOSTR_KIND_ADMIN_DELETE_EVENT,
				NOSTR_KIND_CHANNEL_SYSTEM
			],
			'#h': [CHANNEL],
			limit: 1000
		});
		expect(relayMocks.queryRelayEvents.mock.calls[0][1].until).toBeUndefined();
		// Page two: until is inclusive, so it steps strictly below the page's
		// oldest event (2000 - 999 = 1001 → until 1000).
		expect(relayMocks.queryRelayEvents.mock.calls[1][1].until).toBe(1000);
	});

	it('dedupes events that appear on both sides of a window boundary', async () => {
		const first = pageEvents(1000, 2000, 'aa');
		relayMocks.queryRelayEvents
			.mockResolvedValueOnce(okPage(first))
			.mockResolvedValueOnce(okPage([first[first.length - 1]]));
		const result = await fetchConversationOverlay(input, CHANNEL);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.capped).toBe(false);
		expect(result.events).toHaveLength(1000);
	});

	it('stops at the aggregate page bound with a full last page and reports capped', async () => {
		const pages = Array.from({ length: CONVERSATION_OVERLAY_MAX_PAGES }, (_, p) =>
			pageEvents(1000, 1_000_000 - p * 1000, `p${p}`)
		);
		relayMocks.queryRelayEvents.mockImplementation(async () => okPage(pages.shift() ?? []));
		const result = await fetchConversationOverlay(input, CHANNEL);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.capped).toBe(true);
		expect(result.events).toHaveLength(CONVERSATION_OVERLAY_MAX_PAGES * 1000);
		expect(relayMocks.queryRelayEvents).toHaveBeenCalledTimes(CONVERSATION_OVERLAY_MAX_PAGES);
	});

	it('stops when the window stops moving (relay ignoring until) instead of looping', async () => {
		const same = pageEvents(1000, 2000, 'aa');
		relayMocks.queryRelayEvents.mockResolvedValue(okPage(same));
		const result = await fetchConversationOverlay(input, CHANNEL);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.capped).toBe(true);
		expect(result.events).toHaveLength(1000);
		expect(relayMocks.queryRelayEvents.mock.calls.length).toBeLessThan(
			CONVERSATION_OVERLAY_MAX_PAGES
		);
	});

	it('stops at the created_at floor without issuing an until < 0 query', async () => {
		relayMocks.queryRelayEvents.mockResolvedValueOnce(okPage(pageEvents(1000, 999, 'aa')));
		const result = await fetchConversationOverlay(input, CHANNEL);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.capped).toBe(true);
		expect(relayMocks.queryRelayEvents).toHaveBeenCalledTimes(1);
	});

	it('propagates a failed first page (no overlay at all)', async () => {
		relayMocks.queryRelayEvents.mockResolvedValue({ ok: false, reason: 'transport' });
		const result = await fetchConversationOverlay(input, CHANNEL);
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.reason).toBe('transport');
	});

	it('returns the pages already fetched when a later page fails', async () => {
		relayMocks.queryRelayEvents
			.mockResolvedValueOnce(okPage(pageEvents(1000, 2000, 'aa')))
			.mockResolvedValueOnce({ ok: false, reason: 'rejected' });
		const result = await fetchConversationOverlay(input, CHANNEL);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.capped).toBe(true);
		expect(result.events).toHaveLength(1000);
	});
});
