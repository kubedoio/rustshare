import { describe, expect, it } from 'vitest';
import { schnorr } from '@noble/curves/secp256k1.js';
import { hexToBytes, bytesToHex } from '@noble/curves/utils.js';
import {
	generateSecretKey,
	publicKeyOf,
	signEvent,
	buildUnsignedEvent,
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
		expect(
			schnorr.verify(hexToBytes(signed.sig), hexToBytes(signed.id), hexToBytes(pk))
		).toBe(true);
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
