// frontend/scripts/alpha-validate-buzz-config.mjs
// Validates alpha/dogfood Buzz key consistency so the backend's bridge identity
// cannot silently diverge from the relay's trusted-service allowlist.
//
// Checks:
//   - BUZZ_SERVICE_SK, BUZZ_RELAY_OWNER_PUBKEY, RUSTSHARE_CHAT_BRIDGE_SECRET_KEY,
//     BUZZ_RELAY_PRIVATE_KEY, and optional BUZZ_RELAY_PUBKEY are 64 lowercase hex.
//   - The x-only public key derived from BUZZ_SERVICE_SK equals
//     BUZZ_RELAY_OWNER_PUBKEY (the relay's owner / trusted service key).
//   - RUSTSHARE_CHAT_BRIDGE_SECRET_KEY equals BUZZ_SERVICE_SK in alpha.
//   - The optional BUZZ_RELAY_PUBKEY matches the key derived from
//     BUZZ_RELAY_PRIVATE_KEY.
//
// Private keys are never printed. Exits 0 on success, 1 on failure with
// actionable diagnostics.
import { fileURLToPath } from 'node:url';
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const HEX64_RE = /^[0-9a-f]{64}$/;

// Variables that must be 64 lowercase hex characters when present.
const HEX64_NAMES = [
	'BUZZ_SERVICE_SK',
	'BUZZ_RELAY_OWNER_PUBKEY',
	'RUSTSHARE_CHAT_BRIDGE_SECRET_KEY',
	'BUZZ_RELAY_PRIVATE_KEY'
];

/**
 * @param {string | undefined} value
 * @returns {boolean}
 */
export function isHex64(value) {
	return typeof value === 'string' && HEX64_RE.test(value);
}

/**
 * Derive the x-only Schnorr public key for a 32-byte secret key.
 * @param {string} skHex 64-character hex secret key
 * @returns {string} 64-character hex public key
 */
export function deriveXOnlyPubkey(skHex) {
	return bytesToHex(schnorr.getPublicKey(hexToBytes(skHex)));
}

/**
 * Validate Buzz key consistency from an environment-like object.
 * @param {Record<string, string | undefined>} env
 * @returns {{ ok: boolean; errors: string[] }}
 */
export function validateBuzzConfig(env) {
	const errors = [];

	for (const name of HEX64_NAMES) {
		const value = env[name];
		if (value === undefined || value === '') {
			continue;
		}
		if (!isHex64(value)) {
			errors.push(`${name} must be 64 lowercase hex characters`);
		}
	}

	const relayPubkey = env.BUZZ_RELAY_PUBKEY;
	if (relayPubkey !== undefined && relayPubkey !== '' && !isHex64(relayPubkey)) {
		errors.push('BUZZ_RELAY_PUBKEY must be 64 lowercase hex characters');
	}

	const serviceSk = env.BUZZ_SERVICE_SK;
	const ownerPubkey = env.BUZZ_RELAY_OWNER_PUBKEY;

	if (serviceSk && ownerPubkey && isHex64(serviceSk) && isHex64(ownerPubkey)) {
		try {
			const derivedOwnerPubkey = deriveXOnlyPubkey(serviceSk);
			if (derivedOwnerPubkey !== ownerPubkey) {
				errors.push(
					`BUZZ_RELAY_OWNER_PUBKEY does not match the public key derived from BUZZ_SERVICE_SK ` +
						`(expected ${derivedOwnerPubkey.slice(0, 12)}…, got ${ownerPubkey.slice(0, 12)}…)`
				);
			}
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err);
			errors.push(`Failed to derive public key from BUZZ_SERVICE_SK: ${message}`);
		}
	}

	const bridgeSk = env.RUSTSHARE_CHAT_BRIDGE_SECRET_KEY;
	if (
		bridgeSk !== undefined &&
		bridgeSk !== '' &&
		serviceSk !== undefined &&
		serviceSk !== '' &&
		bridgeSk !== serviceSk
	) {
		errors.push(
			'RUSTSHARE_CHAT_BRIDGE_SECRET_KEY must equal BUZZ_SERVICE_SK in the alpha deployment'
		);
	}

	const relaySk = env.BUZZ_RELAY_PRIVATE_KEY;
	if (relaySk && isHex64(relaySk)) {
		try {
			const derivedRelayPubkey = deriveXOnlyPubkey(relaySk);
			if (relayPubkey && isHex64(relayPubkey) && derivedRelayPubkey !== relayPubkey) {
				errors.push(
					`BUZZ_RELAY_PUBKEY does not match the public key derived from BUZZ_RELAY_PRIVATE_KEY ` +
						`(expected ${derivedRelayPubkey.slice(0, 12)}…, got ${relayPubkey.slice(0, 12)}…)`
				);
			}
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err);
			errors.push(`Failed to derive public key from BUZZ_RELAY_PRIVATE_KEY: ${message}`);
		}
	}

	return { ok: errors.length === 0, errors };
}

function main() {
	const { ok, errors } = validateBuzzConfig(process.env);
	if (!ok) {
		console.error('Buzz configuration validation failed:');
		for (const err of errors) {
			console.error(`  - ${err}`);
		}
		console.error('\nFix the variables above and re-run, or generate a fresh set of keys with:');
		console.error('  node frontend/scripts/alpha-gen-buzz-keys.mjs');
		process.exit(1);
	}
	console.log('Buzz configuration is consistent.');
	process.exit(0);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	main();
}
