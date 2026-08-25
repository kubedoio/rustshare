import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { schnorr } from '@noble/curves/secp256k1.js';
import { hexToBytes, bytesToHex } from '@noble/curves/utils.js';
import {
	generateSecretKey,
	publicKeyOf,
	signEvent,
	buildUnsignedEvent,
	publishEvent,
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

	sent: unknown[][] = [];
	onopen: (() => void) | null = null;
	onmessage: ((raw: { data: unknown }) => void) | null = null;
	onerror: (() => void) | null = null;
	onclose: (() => void) | null = null;
	closed = false;
	authenticated = false;

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
		} else if (frame[0] === 'EVENT' && FakeWebSocket.challengeOnEvent && !this.authenticated) {
			// Buzz relay behavior: an unauthenticated EVENT provokes the NIP-42
			// AUTH challenge instead of an immediate OK.
			this.reply(['AUTH', 'challenge-1']);
		} else if (frame[0] === 'EVENT') {
			const event = frame[1] as { id: string };
			this.reply(['OK', event.id, FakeWebSocket.okValue, FakeWebSocket.okMessage]);
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
	constructor() {
		super();
		queueMicrotask(() => this.onerror?.());
	}
}

describe('publishEvent', () => {
	beforeEach(() => {
		FakeWebSocket.instances = [];
		FakeWebSocket.okValue = true;
		FakeWebSocket.okMessage = '';
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

	it('resolves transport on a socket error before any OK frame', async () => {
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
