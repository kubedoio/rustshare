// frontend/scripts/alpha-validate-buzz-config.test.mjs
// Node.js built-in test runner for alpha-validate-buzz-config.mjs.
// Run with:
//   node --test frontend/scripts/alpha-validate-buzz-config.test.mjs
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { isHex64, deriveXOnlyPubkey, validateBuzzConfig } from './alpha-validate-buzz-config.mjs';

// Known secp256k1 test vector: secret key 1 -> generator x-coordinate.
const SERVICE_SK = '0000000000000000000000000000000000000000000000000000000000000001';
const SERVICE_PK = '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798';

const RELAY_SK = '0000000000000000000000000000000000000000000000000000000000000002';
const RELAY_PK = deriveXOnlyPubkey(RELAY_SK);

describe('isHex64', () => {
	it('accepts 64 lowercase hex characters', () => {
		assert.equal(isHex64('a'.repeat(64)), true);
		assert.equal(isHex64('0123456789abcdef'.repeat(4)), true);
	});

	it('rejects uppercase hex', () => {
		assert.equal(isHex64('A'.repeat(64)), false);
	});

	it('rejects wrong lengths', () => {
		assert.equal(isHex64('a'.repeat(63)), false);
		assert.equal(isHex64('a'.repeat(65)), false);
		assert.equal(isHex64(''), false);
	});

	it('rejects non-hex characters', () => {
		assert.equal(isHex64('g'.repeat(64)), false);
		assert.equal(isHex64(' '.repeat(64)), false);
	});
});

describe('deriveXOnlyPubkey', () => {
	it('derives the known public key for secret key 1', () => {
		assert.equal(deriveXOnlyPubkey(SERVICE_SK), SERVICE_PK);
	});
});

describe('validateBuzzConfig', () => {
	it('accepts a fully consistent alpha configuration', () => {
		const result = validateBuzzConfig({
			BUZZ_SERVICE_SK: SERVICE_SK,
			BUZZ_RELAY_OWNER_PUBKEY: SERVICE_PK,
			RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: SERVICE_SK,
			BUZZ_RELAY_PRIVATE_KEY: RELAY_SK,
			BUZZ_RELAY_PUBKEY: RELAY_PK
		});
		assert.equal(result.ok, true);
		assert.deepEqual(result.errors, []);
	});

	it('accepts a partial configuration with only service keys', () => {
		const result = validateBuzzConfig({
			BUZZ_SERVICE_SK: SERVICE_SK,
			BUZZ_RELAY_OWNER_PUBKEY: SERVICE_PK,
			RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: SERVICE_SK
		});
		assert.equal(result.ok, true);
		assert.deepEqual(result.errors, []);
	});

	it('skips validation for absent optional variables', () => {
		const result = validateBuzzConfig({
			BUZZ_SERVICE_SK: SERVICE_SK,
			BUZZ_RELAY_OWNER_PUBKEY: SERVICE_PK
		});
		assert.equal(result.ok, true);
	});

	it('rejects a mismatched owner public key', () => {
		const result = validateBuzzConfig({
			BUZZ_SERVICE_SK: SERVICE_SK,
			BUZZ_RELAY_OWNER_PUBKEY: 'a'.repeat(64),
			RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: SERVICE_SK
		});
		assert.equal(result.ok, false);
		assert.ok(
			result.errors.some((e) => e.includes('BUZZ_RELAY_OWNER_PUBKEY')),
			`expected owner-pubkey mismatch error, got: ${result.errors.join('; ')}`
		);
	});

	it('rejects a bridge secret that diverges from the service secret', () => {
		const result = validateBuzzConfig({
			BUZZ_SERVICE_SK: SERVICE_SK,
			BUZZ_RELAY_OWNER_PUBKEY: SERVICE_PK,
			RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: 'b'.repeat(64)
		});
		assert.equal(result.ok, false);
		assert.ok(
			result.errors.some((e) => e.includes('RUSTSHARE_CHAT_BRIDGE_SECRET_KEY')),
			`expected bridge-secret mismatch error, got: ${result.errors.join('; ')}`
		);
	});

	it('rejects invalid hex values', () => {
		const result = validateBuzzConfig({
			BUZZ_SERVICE_SK: 'not-hex',
			BUZZ_RELAY_OWNER_PUBKEY: SERVICE_PK,
			RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: SERVICE_SK,
			BUZZ_RELAY_PRIVATE_KEY: 'TOOSHORT',
			BUZZ_RELAY_PUBKEY: 'UPPERCASEUPPERCASEUPPERCASEUPPERCASEUPPERCASEUPPERCASEUPPERCASEUPP'
		});
		assert.equal(result.ok, false);
		assert.ok(result.errors.some((e) => e.includes('BUZZ_SERVICE_SK')));
		assert.ok(result.errors.some((e) => e.includes('BUZZ_RELAY_PRIVATE_KEY')));
		assert.ok(result.errors.some((e) => e.includes('BUZZ_RELAY_PUBKEY')));
	});

	it('rejects a relay public key that does not match the relay private key', () => {
		const result = validateBuzzConfig({
			BUZZ_RELAY_PRIVATE_KEY: RELAY_SK,
			BUZZ_RELAY_PUBKEY: 'c'.repeat(64)
		});
		assert.equal(result.ok, false);
		assert.ok(
			result.errors.some((e) => e.includes('BUZZ_RELAY_PUBKEY')),
			`expected relay-pubkey mismatch error, got: ${result.errors.join('; ')}`
		);
	});

	it('does not leak private secrets in error messages', () => {
		const result = validateBuzzConfig({
			BUZZ_SERVICE_SK: SERVICE_SK,
			BUZZ_RELAY_OWNER_PUBKEY: 'a'.repeat(64),
			RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: 'b'.repeat(64),
			BUZZ_RELAY_PRIVATE_KEY: RELAY_SK,
			BUZZ_RELAY_PUBKEY: 'c'.repeat(64)
		});
		assert.equal(result.ok, false);
		const allErrors = result.errors.join('\n');
		assert.ok(!allErrors.includes(SERVICE_SK), 'service secret leaked');
		assert.ok(!allErrors.includes(RELAY_SK), 'relay secret leaked');
		assert.ok(!allErrors.includes('b'.repeat(64)), 'bridge secret leaked');
	});
});
