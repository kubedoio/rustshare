import { describe, expect, it } from 'vitest';
import type { NostrEvent } from './nostr';
import {
	NOSTR_KIND_ADMIN_DELETE_EVENT,
	NOSTR_KIND_CHANNEL_SYSTEM,
	NOSTR_KIND_DELETION,
	NOSTR_KIND_REACTION,
	buildReplyTags,
	canDeleteMessage,
	draftStorageKey,
	parseDeletionTargets,
	parseReactions,
	parseThreadRefs,
	threadReplies,
	threadRootOf
} from './conversations';

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
