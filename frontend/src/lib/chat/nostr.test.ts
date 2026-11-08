import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { schnorr } from '@noble/curves/secp256k1.js';
import { hexToBytes, bytesToHex } from '@noble/curves/utils.js';
import {
	type NostrEvent,
	generateSecretKey,
	publicKeyOf,
	signEvent,
	buildUnsignedEvent,
	publishEvent,
	publishSignedEvent,
	queryRelayEvents,
	verifyEvent,
	isUuid,
	NOSTR_KIND_STREAM_MESSAGE
} from './nostr';

describe('nostr signing', () => {
	it('generates a 32-byte key and its x-only pubkey', () => {
		const sk = generateSecretKey();
		expect(hexToBytes(sk)).toHaveLength(32);
		const pk = publicKeyOf(sk);
		expect(hexToBytes(pk)).toHaveLength(32);
	});

	it('produces a verifiable BIP-340 signature over the event id', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello', [], pk);
		const signed = await signEvent(unsigned, sk);
		expect(schnorr.verify(hexToBytes(signed.sig), hexToBytes(signed.id), hexToBytes(pk))).toBe(
			true
		);
	});

	it('is deterministic: same input gives the same event id', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello', [], pk);
		const a = await signEvent(unsigned, sk);
		const b = await signEvent(unsigned, sk);
		expect(a.id).toBe(b.id);
		// BIP-340 signing uses a random auxiliary nonce, so the signature
		// itself differs per signing; both must verify against the same id.
		expect(schnorr.verify(hexToBytes(a.sig), hexToBytes(a.id), hexToBytes(pk))).toBe(true);
		expect(schnorr.verify(hexToBytes(b.sig), hexToBytes(b.id), hexToBytes(pk))).toBe(true);
		expect(bytesToHex(schnorr.getPublicKey(hexToBytes(sk)))).toBe(pk);
	});
});

describe('isUuid', () => {
	it('accepts canonical UUIDs and rejects names and malformed forms', () => {
		expect(isUuid('11111111-2222-4333-8444-555555555555')).toBe(true);
		expect(isUuid('AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE')).toBe(true); // uppercase
		expect(isUuid('general')).toBe(false);
		expect(isUuid('alpha-channel')).toBe(false);
		expect(isUuid('')).toBe(false);
		expect(isUuid('11111111-2222-4333-8444-55555555555')).toBe(false); // 11 hex in last group
		expect(isUuid('11111111222243338444555555555555')).toBe(false); // hyphenless
		expect(isUuid('g1111111-2222-4333-8444-555555555555')).toBe(false); // non-hex
	});
});

class FakeWebSocket {
	static instances: FakeWebSocket[] = [];
	static okValue = true;
	static okMessage = '';
	static challengeOnEvent = true;
	// Set false to script every OK frame manually from the test.
	static autoOkOnEvent = true;
	static latePreAuthRejection = false;
	static autoOkDelayMs = 0;

	sent: unknown[][] = [];
	onopen: (() => void) | null = null;
	onmessage: ((raw: { data: unknown }) => void) | null = null;
	onerror: (() => void) | null = null;
	onclose: (() => void) | null = null;
	closed = false;
	authenticated = false;
	initialEventId: string | null = null;

	constructor() {
		FakeWebSocket.instances.push(this);
		// The real socket fires onopen asynchronously, after the caller has
		// assigned its handlers.
		queueMicrotask(() => this.onopen?.());
	}

	send(data: string) {
		const frame = JSON.parse(data) as unknown[];
		this.sent.push(frame);
		if (frame[0] === 'AUTH') {
			this.authenticated = true;
			this.reply(['OK', (frame[1] as { id: string }).id, true, '']);
			if (FakeWebSocket.latePreAuthRejection && this.initialEventId) {
				const eventId = this.initialEventId;
				setTimeout(() => this.reply(['OK', eventId, false, 'auth-required: not authenticated']), 0);
			}
		} else if (frame[0] === 'EVENT' && FakeWebSocket.challengeOnEvent && !this.authenticated) {
			// Buzz relay behavior: an unauthenticated EVENT provokes the NIP-42
			// AUTH challenge instead of an immediate OK.
			this.initialEventId = (frame[1] as { id: string }).id;
			this.reply(['AUTH', 'challenge-1']);
		} else if (frame[0] === 'EVENT' && FakeWebSocket.autoOkOnEvent) {
			const event = frame[1] as { id: string };
			setTimeout(
				() => this.reply(['OK', event.id, FakeWebSocket.okValue, FakeWebSocket.okMessage]),
				FakeWebSocket.autoOkDelayMs
			);
		}
	}

	reply(frame: unknown[]) {
		queueMicrotask(() => this.onmessage?.({ data: JSON.stringify(frame) }));
	}

	close() {
		this.closed = true;
	}
}

/** A socket that errors before it can answer, like a refused connection. */
class ErroringSocket extends FakeWebSocket {
	send() {
		this.onerror?.();
	}
}

describe('publishEvent', () => {
	beforeEach(() => {
		FakeWebSocket.instances = [];
		FakeWebSocket.okValue = true;
		FakeWebSocket.okMessage = '';
		FakeWebSocket.challengeOnEvent = true;
		FakeWebSocket.autoOkOnEvent = true;
		FakeWebSocket.latePreAuthRejection = false;
		FakeWebSocket.autoOkDelayMs = 0;
		vi.stubGlobal('WebSocket', FakeWebSocket);
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it('sends the EVENT frame first, authenticates on the AUTH challenge, and re-sends it', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello relay', [], pk);
		const expected = await signEvent(unsigned, sk);

		const result = await publishEvent('wss://relay.test', unsigned, sk);

		expect(result).toMatchObject({ ok: true, event_id: expect.any(String) });
		const ws = FakeWebSocket.instances[0];
		// Proven Buzz flow: EVENT first (no REQ probe), then AUTH, then EVENT again.
		expect(ws.sent[0][0]).toBe('EVENT');
		const authFrame = ws.sent.find((frame) => frame[0] === 'AUTH');
		expect(authFrame).toBeTruthy();
		expect((authFrame![1] as { tags: string[][] }).tags[0]).toEqual(['relay', 'wss://relay.test']);
		const eventFrame = ws.sent.find((frame) => frame[0] === 'EVENT');
		expect(eventFrame).toBeTruthy();
		expect((eventFrame![1] as { id: string }).id).toBe(expected.id);
	});

	it('retries after an auth-required rejection and resolves ok on the re-send', async () => {
		// Relay answers the first (unauthenticated) EVENT with OK false
		// "auth required" before/after the challenge; the client must not
		// treat that as the final rejection and must resolve from the
		// post-AUTH OK instead.
		FakeWebSocket.challengeOnEvent = true;
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello relay', [], pk);

		const result = await publishEvent('wss://relay.test', unsigned, sk);

		expect(result).toMatchObject({ ok: true, event_id: expect.any(String) });
		const ws = FakeWebSocket.instances[0];
		expect(ws.sent.filter((frame) => frame[0] === 'EVENT')).toHaveLength(2);
		expect(ws.sent.filter((frame) => frame[0] === 'AUTH')).toHaveLength(1);
	});

	it('waits for a delayed successful resend after a late pre-auth rejection', async () => {
		FakeWebSocket.latePreAuthRejection = true;
		FakeWebSocket.autoOkDelayMs = 300;
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello relay', [], pk);

		const result = await publishEvent('wss://relay.test', unsigned, sk);

		expect(result).toMatchObject({ ok: true, event_id: expect.any(String) });
	});

	it('surfaces a persistent auth-flavored rejection instead of swallowing it forever', async () => {
		// Relay demands auth (AUTH challenge + auth-flavored OK false for the
		// pre-auth publish), then rejects the re-sent event with another
		// auth-flavored OK false — e.g. it refused our AUTH. Only the first
		// rejection is the auth demand; the second must resolve as 'rejected'.
		FakeWebSocket.okValue = false;
		FakeWebSocket.okMessage = 'auth failed: rejected by relay';
		FakeWebSocket.challengeOnEvent = false;
		FakeWebSocket.autoOkOnEvent = false;
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello relay', [], pk);

		const promise = publishEvent('wss://relay.test', unsigned, sk);
		await vi.waitFor(() => expect(FakeWebSocket.instances).toHaveLength(1));
		const ws = FakeWebSocket.instances[0];
		await vi.waitFor(() => expect(ws.sent.filter((f) => f[0] === 'EVENT')).toHaveLength(1));
		const eventId = (ws.sent[0][1] as { id: string }).id;
		// Demand for auth: challenge plus an auth-flavored rejection of the
		// pre-auth publish. The first such rejection is swallowed as the demand.
		ws.reply(['AUTH', 'challenge-1']);
		ws.reply(['OK', eventId, false, 'auth required: authentication needed']);
		await vi.waitFor(() => expect(ws.sent.filter((f) => f[0] === 'EVENT')).toHaveLength(2));
		// The relay also rejects the re-sent (post-AUTH) event with an
		// auth-flavored reason: that second rejection is genuine and must surface.
		ws.reply(['OK', eventId, false, 'auth failed: rejected by relay']);

		// Fail fast if a regression re-introduces the hang instead of waiting
		// out the 10s transport timer inside publishEvent.
		const result = await Promise.race([
			promise,
			new Promise<never>((_, reject) =>
				setTimeout(() => reject(new Error('publishEvent hung on a post-AUTH rejection')), 2000)
			)
		]);

		expect(result).toMatchObject({
			ok: false,
			reason: 'rejected',
			detail: 'auth failed: rejected by relay'
		});
	});

	it('surfaces a rejection when the relay challenges before sending an OK', async () => {
		// Some relays send AUTH immediately and only answer the resent EVENT.
		// That rejection must not be mistaken for the initial auth demand.
		FakeWebSocket.autoOkOnEvent = false;
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello relay', [], pk);

		const promise = publishEvent('wss://relay.test', unsigned, sk);
		await vi.waitFor(() => expect(FakeWebSocket.instances).toHaveLength(1));
		const ws = FakeWebSocket.instances[0];
		await vi.waitFor(() => expect(ws.sent.filter((frame) => frame[0] === 'EVENT')).toHaveLength(2));
		const eventId = (ws.sent[0][1] as { id: string }).id;
		ws.reply(['OK', eventId, false, 'auth failed: rejected by relay']);

		await expect(promise).resolves.toMatchObject({
			ok: false,
			reason: 'rejected',
			detail: 'auth failed: rejected by relay'
		});
	});

	it('resolves rejected with the relay message when the relay answers OK false', async () => {
		FakeWebSocket.okValue = false;
		FakeWebSocket.okMessage = 'blocked: not admitted';
		FakeWebSocket.challengeOnEvent = false; // no auth dance for this rejection
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello relay', [], pk);
		const result = await publishEvent('wss://relay.test', unsigned, sk);

		expect(result).toEqual({ ok: false, reason: 'rejected', detail: 'blocked: not admitted' });
	});

	it('resolves transport when the socket fails before EVENT transmission', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello relay', [], pk);

		vi.stubGlobal('WebSocket', ErroringSocket);
		const result = await publishEvent('wss://relay.test', unsigned, sk);

		expect(result).toEqual({ ok: false, reason: 'transport' });
	});
});

class NoChallengeSocket extends FakeWebSocket {
	send(data: string) {
		const frame = JSON.parse(data) as unknown[];
		this.sent.push(frame);
		if (frame[0] === 'EVENT') {
			const event = frame[1] as { id: string };
			this.reply(['OK', event.id, true, '']);
		}
	}
}

describe('publishEvent without an AUTH challenge', () => {
	beforeEach(() => {
		FakeWebSocket.instances = [];
		vi.stubGlobal('WebSocket', NoChallengeSocket);
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it('publishes immediately when the relay answers OK without challenging', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_STREAM_MESSAGE, 'hello relay', [], pk);

		const result = await publishEvent('wss://relay.test', unsigned, sk);

		expect(result).toMatchObject({ ok: true, event_id: expect.any(String) });
		const ws = FakeWebSocket.instances[0];
		expect(ws.sent.some((frame) => frame[0] === 'EVENT')).toBe(true);
		// No auth dance on a challenge-less relay.
		expect(ws.sent.some((frame) => frame[0] === 'AUTH')).toBe(false);
	});
});

describe('publish ambiguity and exact retries', () => {
	beforeEach(() => {
		FakeWebSocket.instances = [];
		FakeWebSocket.challengeOnEvent = false;
		FakeWebSocket.autoOkOnEvent = false;
		vi.stubGlobal('WebSocket', FakeWebSocket);
	});
	afterEach(() => vi.unstubAllGlobals());

	it('retains the exact signed event on close after transmission and retries it unchanged', async () => {
		const sk = generateSecretKey();
		const unsigned = await buildUnsignedEvent(9, 'draft', [['h', 'channel']], publicKeyOf(sk));
		const pending = publishEvent('wss://relay.test', unsigned, sk);
		await vi.waitFor(() => expect(FakeWebSocket.instances[0]?.sent).toHaveLength(1));
		const firstSocket = FakeWebSocket.instances[0];
		firstSocket.onclose?.();
		const result = await pending;
		expect(result).toMatchObject({ ok: false, reason: 'uncertain' });
		if (result.ok || result.reason !== 'uncertain') throw new Error('expected uncertain');
		expect(result.event).toEqual(firstSocket.sent[0][1]);
		const retry = publishSignedEvent('wss://relay.test', result.event, sk);
		await vi.waitFor(() => expect(FakeWebSocket.instances).toHaveLength(2));
		const retrySocket = FakeWebSocket.instances[1];
		await vi.waitFor(() => expect(retrySocket.sent).toHaveLength(1));
		expect(retrySocket.sent[0]).toEqual(firstSocket.sent[0]);
		retrySocket.reply(['OK', result.event_id, true, 'duplicate: already have this event']);
		await expect(retry).resolves.toEqual({
			ok: true,
			event_id: result.event_id,
			detail: 'duplicate: already have this event'
		});
	});

	it('does not connect when durable signed-event retention fails', async () => {
		const sk = generateSecretKey();
		const unsigned = await buildUnsignedEvent(9, 'draft', [], publicKeyOf(sk));
		await expect(
			publishEvent('wss://relay.test', unsigned, sk, () => {
				throw new Error('storage unavailable');
			})
		).rejects.toThrow('storage unavailable');
		expect(FakeWebSocket.instances).toHaveLength(0);
	});

	it('rejects a retry with a different signing identity before connecting', async () => {
		const sk = generateSecretKey();
		const event = await signEvent(await buildUnsignedEvent(9, '', [], publicKeyOf(sk)), sk);
		await expect(
			publishSignedEvent('wss://relay.test', event, generateSecretKey())
		).resolves.toMatchObject({ ok: false, reason: 'rejected' });
		expect(FakeWebSocket.instances).toHaveLength(0);
	});

	it('does not resend EVENT until authentication is acknowledged', async () => {
		const sk = generateSecretKey();
		const pending = publishEvent(
			'wss://relay.test',
			await buildUnsignedEvent(9, '', [], publicKeyOf(sk)),
			sk
		);
		await vi.waitFor(() => expect(FakeWebSocket.instances[0]?.sent).toHaveLength(1));
		const ws = FakeWebSocket.instances[0];
		const originalSend = ws.send.bind(ws);
		ws.send = (data) => {
			const frame = JSON.parse(data);
			if (frame[0] === 'AUTH') ws.sent.push(frame);
			else originalSend(data);
		};
		ws.reply(['AUTH', 'challenge']);
		await vi.waitFor(() => expect(ws.sent).toHaveLength(2));
		expect(ws.sent.filter((f) => f[0] === 'EVENT')).toHaveLength(1);
		ws.reply(['OK', (ws.sent[1][1] as { id: string }).id, false, 'restricted: not a member']);
		await expect(pending).resolves.toMatchObject({
			ok: false,
			reason: 'rejected',
			detail: 'restricted: not a member'
		});
	});
});

class QuerySocket extends FakeWebSocket {
	constructor() {
		super();
		queueMicrotask(() => this.reply(['AUTH', 'query-challenge']));
	}
	send(data: string) {
		const frame = JSON.parse(data) as unknown[];
		this.sent.push(frame);
		if (frame[0] === 'AUTH') this.reply(['OK', (frame[1] as { id: string }).id, true, '']);
	}
}

describe('subject-authenticated relay reads', () => {
	beforeEach(() => {
		FakeWebSocket.instances = [];
		vi.stubGlobal('WebSocket', QuerySocket);
	});
	afterEach(() => vi.unstubAllGlobals());

	async function query(snapshot: NostrEvent, channel = 'channel') {
		const sk = generateSecretKey();
		const pending = queryRelayEvents(
			'wss://relay.test',
			{ kinds: [39002], '#d': [channel] },
			sk,
			snapshot.pubkey
		);
		await vi.waitFor(() =>
			expect(FakeWebSocket.instances[0]?.sent.filter((f) => f[0] === 'REQ')).toHaveLength(2)
		);
		const ws = FakeWebSocket.instances[0];
		const req = ws.sent.find((f) => f[0] === 'REQ')!;
		return { pending, ws, subscription: req[1] };
	}

	it('accepts only matching signed snapshots after subject AUTH and verifies before EOSE', async () => {
		const relaySk = generateSecretKey();
		const snapshot = await signEvent(
			await buildUnsignedEvent(39002, '', [['d', 'channel']], publicKeyOf(relaySk)),
			relaySk
		);
		const { pending, ws, subscription } = await query(snapshot);
		ws.reply(['EVENT', subscription, snapshot]);
		ws.reply(['EOSE', subscription]);
		await expect(pending).resolves.toEqual({ ok: true, events: [snapshot] });
	});

	it('fails closed on a valid snapshot for the wrong channel', async () => {
		const relaySk = generateSecretKey();
		const snapshot = await signEvent(
			await buildUnsignedEvent(39002, '', [['d', 'elsewhere']], publicKeyOf(relaySk)),
			relaySk
		);
		const { pending, ws, subscription } = await query(snapshot);
		ws.reply(['EVENT', subscription, snapshot]);
		ws.reply(['EOSE', subscription]);
		await expect(pending).resolves.toMatchObject({ ok: false, reason: 'invalid' });
	});

	it('fails closed on a signed snapshot from an untrusted relay key', async () => {
		const trusted = generateSecretKey();
		const other = generateSecretKey();
		const snapshot = await signEvent(
			await buildUnsignedEvent(39002, '', [['d', 'channel']], publicKeyOf(other)),
			other
		);
		const { pending, ws, subscription } = await query({
			...snapshot,
			pubkey: publicKeyOf(trusted)
		});
		ws.reply(['EVENT', subscription, snapshot]);
		await expect(pending).resolves.toMatchObject({ ok: false, reason: 'invalid' });
	});

	it('returns denied rather than a partial event list on CLOSED', async () => {
		const relaySk = generateSecretKey();
		const snapshot = await signEvent(
			await buildUnsignedEvent(39002, '', [['d', 'channel']], publicKeyOf(relaySk)),
			relaySk
		);
		const { pending, ws, subscription } = await query(snapshot);
		ws.reply(['EVENT', subscription, snapshot]);
		ws.reply(['CLOSED', subscription, 'restricted: private channel']);
		await expect(pending).resolves.toEqual({
			ok: false,
			reason: 'rejected',
			detail: 'restricted: private channel'
		});
	});

	it('rejects malformed limits before connecting', async () => {
		const sk = generateSecretKey();
		await expect(
			queryRelayEvents('wss://relay.test', { kinds: [39000], limit: NaN }, sk, publicKeyOf(sk))
		).resolves.toEqual({ ok: false, reason: 'invalid' });
		expect(FakeWebSocket.instances).toHaveLength(0);
	});

	it('fails closed when more events arrive than requested', async () => {
		const sk = generateSecretKey();
		const event = await signEvent(
			await buildUnsignedEvent(39002, '', [['d', 'channel']], publicKeyOf(sk)),
			sk
		);
		const pending = queryRelayEvents(
			'wss://relay.test',
			{ kinds: [39002], '#d': ['channel'], limit: 1 },
			sk,
			publicKeyOf(sk)
		);
		await vi.waitFor(() =>
			expect(FakeWebSocket.instances[0]?.sent.filter((f) => f[0] === 'REQ')).toHaveLength(2)
		);
		const ws = FakeWebSocket.instances[0];
		const subscription = ws.sent.find((f) => f[0] === 'REQ')![1];
		ws.reply(['EVENT', subscription, event]);
		ws.reply(['EVENT', subscription, event]);
		ws.reply(['EOSE', subscription]);
		await expect(pending).resolves.toEqual({ ok: false, reason: 'invalid' });
	});

	it('binds thread queries to the requested parent event', async () => {
		const sk = generateSecretKey();
		const snapshot = await signEvent(
			await buildUnsignedEvent(
				9,
				'',
				[
					['h', 'channel'],
					['e', 'other-parent']
				],
				publicKeyOf(sk)
			),
			sk
		);
		const pending = queryRelayEvents(
			'wss://relay.test',
			{ kinds: [9], '#h': ['channel'], '#e': ['requested-parent'] },
			sk,
			publicKeyOf(sk)
		);
		await vi.waitFor(() =>
			expect(FakeWebSocket.instances[0]?.sent.filter((f) => f[0] === 'REQ')).toHaveLength(2)
		);
		const ws = FakeWebSocket.instances[0];
		const subscription = ws.sent.find((f) => f[0] === 'REQ')![1];
		ws.reply(['EVENT', subscription, snapshot]);
		await expect(pending).resolves.toEqual({ ok: false, reason: 'invalid' });
	});

	it('rejects content mutation even when the signature still matches the old ID', async () => {
		const sk = generateSecretKey();
		const snapshot = await signEvent(
			await buildUnsignedEvent(39000, '', [['d', 'channel']], publicKeyOf(sk)),
			sk
		);
		expect(await verifyEvent(snapshot)).toBe(true);
		expect(await verifyEvent({ ...snapshot, content: 'tampered' })).toBe(false);
	});
});
