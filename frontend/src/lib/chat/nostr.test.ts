import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { schnorr } from '@noble/curves/secp256k1.js';
import { hexToBytes, bytesToHex } from '@noble/curves/utils.js';
import {
	generateSecretKey,
	publicKeyOf,
	signEvent,
	buildUnsignedEvent,
	publishEvent,
	NOSTR_KIND_TEXT
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
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_TEXT, 'hello', [], pk);
		const signed = await signEvent(unsigned, sk);
		expect(schnorr.verify(hexToBytes(signed.sig), hexToBytes(signed.id), hexToBytes(pk))).toBe(
			true
		);
	});

	it('is deterministic: same input gives the same event id', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_TEXT, 'hello', [], pk);
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

class FakeWebSocket {
	static instances: FakeWebSocket[] = [];
	static okValue = true;

	sent: unknown[][] = [];
	onopen: (() => void) | null = null;
	onmessage: ((raw: { data: unknown }) => void) | null = null;
	onerror: (() => void) | null = null;
	onclose: (() => void) | null = null;
	closed = false;

	constructor() {
		FakeWebSocket.instances.push(this);
		// The real socket fires onopen asynchronously, after the caller has
		// assigned its handlers.
		queueMicrotask(() => this.onopen?.());
	}

	send(data: string) {
		const frame = JSON.parse(data) as unknown[];
		this.sent.push(frame);
		if (frame[0] === 'REQ') {
			this.reply(['AUTH', 'challenge-1']);
		} else if (frame[0] === 'EVENT') {
			const event = frame[1] as { id: string };
			this.reply(['OK', event.id, FakeWebSocket.okValue, '']);
		}
	}

	reply(frame: unknown[]) {
		queueMicrotask(() => this.onmessage?.({ data: JSON.stringify(frame) }));
	}

	close() {
		this.closed = true;
	}
}

describe('publishEvent', () => {
	beforeEach(() => {
		FakeWebSocket.instances = [];
		FakeWebSocket.okValue = true;
		vi.stubGlobal('WebSocket', FakeWebSocket);
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it('sends the EVENT frame after AUTH and resolves true on OK', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_TEXT, 'hello relay', [], pk);
		const expected = await signEvent(unsigned, sk);

		const ok = await publishEvent('wss://relay.test', unsigned, sk);

		expect(ok).toBe(true);
		const ws = FakeWebSocket.instances[0];
		expect(ws.sent[0][0]).toBe('REQ');
		const eventFrame = ws.sent.find((frame) => frame[0] === 'EVENT');
		expect(eventFrame).toBeTruthy();
		expect((eventFrame![1] as { id: string }).id).toBe(expected.id);
	});

	it('resolves false when the relay rejects with OK false', async () => {
		FakeWebSocket.okValue = false;
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_TEXT, 'hello relay', [], pk);

		const ok = await publishEvent('wss://relay.test', unsigned, sk);

		expect(ok).toBe(false);
	});
});
