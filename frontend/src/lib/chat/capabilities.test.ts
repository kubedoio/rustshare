import { describe, expect, it } from 'vitest';
import type { NostrEvent } from './nostr';
import { canPostToChannel, getChatCapabilities, parseChannelState } from './capabilities';

const channel = '11111111-1111-4111-8111-111111111111';
const pubkey = 'a'.repeat(64);
const event = (kind: number, tags: string[][], created_at = 1): NostrEvent => ({
	kind,
	tags: [['d', channel], ...tags],
	created_at,
	id: String(created_at),
	pubkey: 'b'.repeat(64),
	content: '',
	sig: ''
});
const metadata = event(39000, [['name', 'Team'], ['about', 'Description'], ['private']]);

describe('supported browser-key Chat capabilities', () => {
	it('keeps managed and local profiles unavailable without borrowing fork support', () => {
		expect(getChatCapabilities('fork_client_held').createChannel.available).toBe(true);
		expect(getChatCapabilities('upstream_managed').createChannel.available).toBe(false);
		expect(getChatCapabilities('local_client_held').directory.available).toBe(false);
		expect(getChatCapabilities('fork_client_held').readOnlyGuests.available).toBe(false);
		expect(getChatCapabilities('fork_client_held').editMessage.available).toBe(false);
	});

	it('reads the pinned roster role from its fourth p-tag field', () => {
		const state = parseChannelState(
			[metadata, event(39002, [['p', pubkey, '', 'owner']])],
			channel,
			pubkey
		);
		expect(state).toMatchObject({
			name: 'Team',
			description: 'Description',
			visibility: 'private',
			role: 'owner'
		});
		expect(canPostToChannel(state)).toBe(true);
	});

	it('requires both matching snapshots and denies guest, unknown and nonmember posting', () => {
		expect(parseChannelState([metadata], channel, pubkey)).toBeNull();
		expect(parseChannelState([metadata, event(39002, [])], 'other-channel', pubkey)).toBeNull();
		for (const tags of [[['p', pubkey, '', 'guest']], [['p', pubkey, '', 'future-role']], []]) {
			expect(
				canPostToChannel(parseChannelState([metadata, event(39002, tags)], channel, pubkey))
			).toBe(false);
		}
	});

	it('uses the latest roster and denies archived channels', () => {
		const events = [
			metadata,
			event(39002, [['p', pubkey, '', 'member']]),
			event(39002, [['p', pubkey, '', 'guest']], 2)
		];
		expect(canPostToChannel(parseChannelState(events, channel, pubkey))).toBe(false);
		const archived = event(39000, [['archived', 'true']], 3);
		expect(
			canPostToChannel(
				parseChannelState([archived, event(39002, [['p', pubkey, '', 'owner']])], channel, pubkey)
			)
		).toBe(false);
	});
});
