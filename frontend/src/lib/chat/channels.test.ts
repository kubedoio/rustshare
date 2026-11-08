import { describe, expect, it } from 'vitest';
import type { NostrEvent } from './nostr';
import {
	canAddMembers,
	canEditChannelMetadata,
	canJoinChannel,
	canRemoveMembers,
	isLastOwner,
	parseChannelDirectory,
	parseChannelRoster
} from './channels';

const relayPubkey = 'b'.repeat(64);
const memberA = 'a'.repeat(64);
const memberB = 'c'.repeat(64);

function metadata(channelId: string, tags: string[][], created_at = 1): NostrEvent {
	return {
		id: `${channelId}:${created_at}`,
		kind: 39000,
		created_at,
		pubkey: relayPubkey,
		tags: [['d', channelId], ...tags],
		content: '',
		sig: ''
	};
}

describe('parseChannelDirectory', () => {
	it('includes channels with zero messages alongside active ones', () => {
		const events = [
			metadata('11111111-1111-4111-8111-111111111111', [['name', 'General']]),
			metadata('22222222-2222-4222-8222-222222222222', [
				['name', 'Empty'],
				['about', 'No messages yet']
			])
		];
		const directory = parseChannelDirectory(events);
		expect(directory.map((entry) => entry.name)).toEqual(['Empty', 'General']);
		expect(directory.find((entry) => entry.name === 'Empty')).toMatchObject({
			channelId: '22222222-2222-4222-8222-222222222222',
			description: 'No messages yet',
			visibility: 'open',
			archived: false
		});
	});

	it('marks private channels and omits descriptions the relay does not emit', () => {
		const events = [
			metadata('11111111-1111-4111-8111-111111111111', [['name', 'Secret'], ['private']])
		];
		const [entry] = parseChannelDirectory(events);
		expect(entry.visibility).toBe('private');
		expect(entry.description).toBe('');
	});

	it('excludes hidden DM channels and archived channels from the sidebar list', () => {
		const events = [
			metadata('11111111-1111-4111-8111-111111111111', [['name', 'Visible']]),
			metadata('22222222-2222-4222-8222-222222222222', [['name', 'Dm'], ['hidden']]),
			metadata('33333333-3333-4333-8333-333333333333', [
				['name', 'Old'],
				['archived', 'true']
			])
		];
		const directory = parseChannelDirectory(events);
		expect(directory.map((entry) => entry.name)).toEqual(['Visible']);
	});

	it('uses the newest metadata snapshot per channel', () => {
		const events = [
			metadata('11111111-1111-4111-8111-111111111111', [['name', 'Before']], 1),
			metadata('11111111-1111-4111-8111-111111111111', [['name', 'After']], 2)
		];
		const [entry] = parseChannelDirectory(events);
		expect(entry.name).toBe('After');
	});

	it('only surfaces channels the relay returned — non-disclosure is structural', () => {
		// A private channel the caller may not see never appears in the relay
		// result, so it can never reach the directory. The parser adds nothing
		// of its own: no cache, no inference from message history.
		const events = [metadata('11111111-1111-4111-8111-111111111111', [['name', 'Mine']])];
		const directory = parseChannelDirectory(events);
		expect(directory).toHaveLength(1);
		expect(directory.some((entry) => entry.name === 'Secret')).toBe(false);
	});
});

describe('parseChannelRoster', () => {
	const roster = (channelId: string, tags: string[][], created_at = 1): NostrEvent => ({
		id: `roster:${created_at}`,
		kind: 39002,
		created_at,
		pubkey: relayPubkey,
		tags: [['d', channelId], ...tags],
		content: '',
		sig: ''
	});
	const channelId = '11111111-1111-4111-8111-111111111111';

	it('reads actual roles from the fourth p-tag field, including guest', () => {
		const entries = parseChannelRoster(
			[
				roster(channelId, [
					['p', memberA, '', 'owner'],
					['p', memberB, '', 'guest'],
					['not-p', 'x'],
					['p', 'not-a-pubkey', '', 'member']
				])
			],
			channelId
		);
		expect(entries).toEqual([
			{ pubkey: memberA, role: 'owner' },
			{ pubkey: memberB, role: 'guest' }
		]);
	});

	it('returns an empty roster when the relay exposes none for this identity', () => {
		expect(parseChannelRoster([], channelId)).toEqual([]);
		expect(parseChannelRoster([roster(channelId, [['p', memberA, '', 'owner']])], 'other')).toEqual(
			[]
		);
	});

	it('uses the newest roster snapshot', () => {
		const entries = parseChannelRoster(
			[
				roster(channelId, [['p', memberA, '', 'owner']], 1),
				roster(channelId, [['p', memberA, '', 'guest']], 2)
			],
			channelId
		);
		expect(entries).toEqual([{ pubkey: memberA, role: 'guest' }]);
	});
});

describe('management gating', () => {
	it('limits metadata edits to owner/admin', () => {
		expect(canEditChannelMetadata('owner')).toBe(true);
		expect(canEditChannelMetadata('admin')).toBe(true);
		expect(canEditChannelMetadata('member')).toBe(false);
		expect(canEditChannelMetadata('guest')).toBe(false);
		expect(canEditChannelMetadata(null)).toBe(false);
	});

	it('allows member adds per the fork rules without ever offering guest', () => {
		expect(canAddMembers('owner', 'private')).toBe(true);
		expect(canAddMembers('admin', 'private')).toBe(true);
		expect(canAddMembers('member', 'private')).toBe(false);
		expect(canAddMembers('member', 'open')).toBe(true);
		expect(canAddMembers('guest', 'open')).toBe(false);
		expect(canAddMembers(null, 'open')).toBe(false);
	});

	it('limits removing others to owner/admin', () => {
		expect(canRemoveMembers('owner')).toBe(true);
		expect(canRemoveMembers('admin')).toBe(true);
		expect(canRemoveMembers('member')).toBe(false);
	});

	it('offers join only on open, non-archived, not-yet-member channels', () => {
		expect(canJoinChannel({ visibility: 'open', archived: false }, null)).toBe(true);
		expect(canJoinChannel({ visibility: 'private', archived: false }, null)).toBe(false);
		expect(canJoinChannel({ visibility: 'open', archived: true }, null)).toBe(false);
		expect(canJoinChannel({ visibility: 'open', archived: false }, 'member')).toBe(false);
	});

	it('flags the last owner so the UI can explain the relay leave rejection', () => {
		const roster = [
			{ pubkey: memberA, role: 'owner' as const },
			{ pubkey: memberB, role: 'admin' as const }
		];
		expect(isLastOwner(roster, memberA)).toBe(true);
		expect(isLastOwner(roster, memberB)).toBe(false);
		expect(
			isLastOwner([...roster, { pubkey: 'd'.repeat(64), role: 'owner' as const }], memberA)
		).toBe(false);
	});
});
