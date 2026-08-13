// frontend/scripts/alpha-gen-buzz-keys.mjs
// Generates the two keypairs a clean Elembra Alpha deployment needs:
//   1. the relay owner / Elembra bridge service key (RELAY_OWNER_PUBKEY on
//      the relay side, RUSTSHARE_CHAT_BRIDGE_SECRET_KEY + BUZZ_SERVICE_SK on
//      the Elembra/observer side);
//   2. the relay's own identity key (BUZZ_RELAY_PRIVATE_KEY).
// Usage: node scripts/alpha-gen-buzz-keys.mjs
// Print the values once and place them in .env (see .env.example). No state
// is written anywhere; the keys live only in .env and are never logged by
// the services.
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex } from '@noble/curves/utils.js';

const newKeypair = () => {
	const sk = crypto.getRandomValues(new Uint8Array(32));
	const skHex = bytesToHex(sk);
	return { sk: skHex, pk: bytesToHex(schnorr.getPublicKey(sk)) };
};

const owner = newKeypair();
const relay = newKeypair();

console.log('== Elembra Alpha Buzz keys (add to .env; keep private) ==');
console.log(`BUZZ_RELAY_OWNER_PUBKEY=${owner.pk}`);
console.log(`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY=${owner.sk}`);
console.log(`BUZZ_SERVICE_SK=${owner.sk}`);
console.log(`BUZZ_RELAY_PRIVATE_KEY=${relay.sk}`);
console.log(`BUZZ_RELAY_PUBKEY=${relay.pk}   # informational only`);
