import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { NostrEvent } from './nostr';

const mocks = vi.hoisted(() => ({
	publishEvent: vi.fn(),
	publishSignedEvent: vi.fn(),
	queryRelayEvents: vi.fn(),
	publicKeyOf: vi.fn((secretKey: string) => 'a'.repeat(64))
}));

vi.mock('./nostr', () => ({
	buildUnsignedEvent: async (kind: number, content: string, tags: string[][], pubkey: string) => ({
		pubkey,
		created_at: 0,
		kind,
		tags,
		content
	}),
	publishEvent: mocks.publishEvent,
	publishSignedEvent: mocks.publishSignedEvent,
	publicKeyOf: mocks.publicKeyOf,
	queryRelayEvents: mocks.queryRelayEvents,
	isUuid: (value: string) =>
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value)
}));

import {
	addChannelMember,
	createChannel,
	editChannel,
	joinChannel,
	leaveChannel,
	listVisibleChannels,
	removeChannelMember,
	retryChannelMutation
} from './channels';

const relayPubkey = 'b'.repeat(64);
const target = 'c'.repeat(64);
const input = { relayUrl: 'wss://relay.test', secretKey: 'sk', relayPubkey };

function metadataEvent(channelId: string, tags: string[][], created_at = 5): NostrEvent {
	return {
		id: `m:${channelId}:${created_at}`,
		kind: 39000,
		created_at,
		pubkey: relayPubkey,
		tags: [['d', channelId], ...tags],
		content: '',
		sig: ''
	};
}

function rosterEvent(channelId: string, tags: string[][], created_at = 5): NostrEvent {
	return {
		id: `r:${channelId}:${created_at}`,
		kind: 39002,
		created_at,
		pubkey: relayPubkey,
		tags: [['d', channelId], ...tags],
		content: '',
		sig: ''
	};
}

beforeEach(() => {
	mocks.publishEvent.mockReset();
	mocks.publishSignedEvent.mockReset();
	mocks.queryRelayEvents.mockReset();
});

describe('createChannel', () => {
	it('confirms creation against relay-signed metadata instead of trusting the OK', async () => {
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e1' });
		mocks.queryRelayEvents.mockImplementation(async (_url: string, filter: { '#d'?: string[] }) => {
			const channelId = filter['#d']?.[0];
			return {
				ok: true,
				events: [
					metadataEvent(channelId!, [
						['name', 'General'],
						['about', 'Talk']
					])
				]
			};
		});
		const { channelId, result } = await createChannel(input, {
			name: 'General',
			description: 'Talk',
			visibility: 'open'
		});
		expect(result).toEqual({ status: 'confirmed' });
		expect(channelId).toMatch(/^[0-9a-f-]{36}$/);
		// The publish carried the client-chosen UUID, name, visibility and kind 9007.
		const [, unsigned] = mocks.publishEvent.mock.calls[0];
		expect(unsigned.kind).toBe(9007);
		expect(unsigned.tags).toContainEqual(['h', channelId]);
		expect(unsigned.tags).toContainEqual(['name', 'General']);
		expect(unsigned.tags).toContainEqual(['about', 'Talk']);
		expect(unsigned.tags).toContainEqual(['visibility', 'open']);
	});

	it('reports unconfirmed when the relay never exposes the new metadata', async () => {
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e1' });
		mocks.queryRelayEvents.mockResolvedValue({ ok: true, events: [] });
		const { result } = await createChannel(input, { name: 'Ghost', visibility: 'private' });
		expect(result.status).toBe('unconfirmed');
	});

	it('surfaces relay rejections such as a channel-id collision', async () => {
		mocks.publishEvent.mockResolvedValue({
			ok: false,
			reason: 'rejected',
			detail: 'duplicate: channel already exists'
		});
		const { result } = await createChannel(input, { name: 'Again', visibility: 'open' });
		expect(result).toMatchObject({
			status: 'rejected',
			detail: 'duplicate: channel already exists'
		});
		expect(mocks.queryRelayEvents).not.toHaveBeenCalled();
	});

	it('keeps the exact signed event on uncertainty so a retry never re-signs', async () => {
		const signed = {
			id: 'e-uncertain',
			kind: 9007,
			created_at: 1,
			pubkey: 'pk:sk',
			tags: [],
			content: '',
			sig: 'sig'
		};
		mocks.publishEvent.mockResolvedValue({
			ok: false,
			reason: 'uncertain',
			event_id: signed.id,
			event: signed
		});
		const { channelId, result } = await createChannel(input, { name: 'Retry', visibility: 'open' });
		expect(result).toMatchObject({ status: 'uncertain', event: signed });

		// Retry replays the identical bytes; the relay dedupes by event id, so
		// the original operation is confirmed — never applied twice.
		mocks.publishSignedEvent.mockResolvedValue({ ok: true, event_id: signed.id });
		mocks.queryRelayEvents.mockResolvedValue({
			ok: true,
			events: [metadataEvent(channelId, [['name', 'Retry']])]
		});
		const retry = await retryChannelMutation(input, signed, async () => {
			const snapshot = await mocks.queryRelayEvents();
			return snapshot.ok;
		});
		expect(retry).toEqual({ status: 'confirmed' });
		expect(mocks.publishSignedEvent).toHaveBeenCalledWith('wss://relay.test', signed, 'sk');
		expect(mocks.publishEvent).toHaveBeenCalledTimes(1);
	});
});

describe('editChannel', () => {
	const channelId = '11111111-1111-4111-8111-111111111111';

	it('confirms when a fresh snapshot matches the requested values', async () => {
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e2' });
		mocks.queryRelayEvents.mockResolvedValue({
			ok: true,
			events: [metadataEvent(channelId, [['name', 'Renamed']])]
		});
		const result = await editChannel(input, channelId, { name: 'Renamed' });
		expect(result).toEqual({ status: 'confirmed' });
		const [, unsigned] = mocks.publishEvent.mock.calls[0];
		expect(unsigned.kind).toBe(9002);
		expect(unsigned.tags).toContainEqual(['h', channelId]);
		expect(unsigned.tags).toContainEqual(['name', 'Renamed']);
	});

	it('reports unconfirmed when the snapshot still shows old values', async () => {
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e2' });
		mocks.queryRelayEvents.mockResolvedValue({
			ok: true,
			events: [metadataEvent(channelId, [['name', 'Old']])]
		});
		const result = await editChannel(input, channelId, { name: 'Renamed' });
		expect(result.status).toBe('unconfirmed');
	});

	it('treats a cleared description as confirmed when about is absent', async () => {
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e2' });
		// The fork omits `about` for empty descriptions.
		mocks.queryRelayEvents.mockResolvedValue({
			ok: true,
			events: [metadataEvent(channelId, [['name', 'Same']])]
		});
		const result = await editChannel(input, channelId, {
			name: 'Same',
			description: ''
		});
		expect(result).toEqual({ status: 'confirmed' });
	});
});

describe('membership operations', () => {
	const channelId = '11111111-1111-4111-8111-111111111111';

	it('adds a member and confirms against the fresh relay-signed roster', async () => {
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e3' });
		mocks.queryRelayEvents.mockResolvedValue({
			ok: true,
			events: [rosterEvent(channelId, [['p', target, '', 'member']])]
		});
		const result = await addChannelMember(input, channelId, target);
		expect(result).toEqual({ status: 'confirmed' });
		const [, unsigned] = mocks.publishEvent.mock.calls[0];
		expect(unsigned.kind).toBe(9000);
		// No role tag: the fork defaults new members to `member`; guest and
		// elevated grants are never requested.
		expect(unsigned.tags).toContainEqual(['h', channelId]);
		expect(unsigned.tags).toContainEqual(['p', target]);
		expect(unsigned.tags.some((tag: string[]) => tag[0] === 'role')).toBe(false);
	});

	it('surfaces the relay denial when the role is insufficient', async () => {
		mocks.publishEvent.mockResolvedValue({
			ok: false,
			reason: 'rejected',
			detail: 'only owners/admins may add private-channel members'
		});
		const result = await addChannelMember(input, channelId, target);
		expect(result).toMatchObject({
			status: 'rejected',
			detail: 'only owners/admins may add private-channel members'
		});
		expect(mocks.queryRelayEvents).not.toHaveBeenCalled();
	});

	it('rejects malformed pubkeys locally without publishing', async () => {
		const result = await addChannelMember(input, channelId, 'not-a-pubkey');
		expect(result.status).toBe('rejected');
		expect(mocks.publishEvent).not.toHaveBeenCalled();
	});

	it('confirms removal when the fresh roster omits the target', async () => {
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e4' });
		mocks.queryRelayEvents.mockResolvedValue({ ok: true, events: [rosterEvent(channelId, [])] });
		const result = await removeChannelMember(input, channelId, target);
		expect(result).toEqual({ status: 'confirmed' });
		expect(mocks.publishEvent.mock.calls[0][1].kind).toBe(9001);
	});

	it('confirms join and leave against roster membership of the caller', async () => {
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e5' });
		mocks.queryRelayEvents.mockResolvedValue({
			ok: true,
			events: [rosterEvent(channelId, [['p', 'a'.repeat(64), '', 'member']])]
		});
		expect(await joinChannel(input, channelId)).toEqual({ status: 'confirmed' });
		expect(mocks.publishEvent.mock.calls[0][1].kind).toBe(9021);

		mocks.queryRelayEvents.mockResolvedValue({ ok: true, events: [rosterEvent(channelId, [])] });
		expect(await leaveChannel(input, channelId)).toEqual({ status: 'confirmed' });
		expect(mocks.publishEvent.mock.calls[1][1].kind).toBe(9022);
	});

	it('reports unconfirmed when the post-OK roster snapshot is empty (add/join)', async () => {
		// Inverse of the removal semantics: for an add/join the expected state
		// is the target's VISIBLE membership, so an empty snapshot can never
		// mean success.
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e6' });
		mocks.queryRelayEvents.mockResolvedValue({ ok: true, events: [] });
		expect(await addChannelMember(input, channelId, target)).toEqual({ status: 'unconfirmed' });
		expect(await joinChannel(input, channelId)).toEqual({ status: 'unconfirmed' });
	});
});

describe('listVisibleChannels', () => {
	it('maps relay-visible metadata, including empty channels', async () => {
		mocks.queryRelayEvents.mockResolvedValue({
			ok: true,
			events: [
				metadataEvent('11111111-1111-4111-8111-111111111111', [['name', 'General']]),
				metadataEvent('22222222-2222-4222-8222-222222222222', [['name', 'Empty'], ['private']])
			]
		});
		const result = await listVisibleChannels(input);
		expect(result.ok).toBe(true);
		if (result.ok) {
			expect(result.channels.map((entry) => entry.name)).toEqual(['Empty', 'General']);
			expect(result.channels.find((entry) => entry.name === 'Empty')?.visibility).toBe('private');
		}
		expect(mocks.queryRelayEvents).toHaveBeenCalledWith(
			'wss://relay.test',
			{ kinds: [39000] },
			'sk',
			relayPubkey
		);
	});

	it('passes transport failures through so the UI can say it is offline', async () => {
		mocks.queryRelayEvents.mockResolvedValue({ ok: false, reason: 'transport' });
		const result = await listVisibleChannels(input);
		expect(result).toEqual({ ok: false, reason: 'transport' });
	});
});
