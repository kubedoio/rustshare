// frontend/scripts/chat-relay-probe.mjs
// Real-relay proof helper: NIP-42 auth + signed kind-1 publish against any
// Buzz relay (Node 22+ has global WebSocket). Mirrors ADR-0034's live proof.
// Usage:
//   node scripts/chat-relay-probe.mjs <wss://relay> <secret-key-hex> <text>
// Exit 0 when the relay accepted the event, 1 otherwise.
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const [, , relayUrl, secretKey, content] = process.argv;
if (!relayUrl || !secretKey || !content) {
	console.error('usage: chat-relay-probe.mjs <wss://relay> <secret-key-hex> <text>');
	process.exit(2);
}

// Derive the pubkey before touching the network so an unusable key fails
// cleanly instead of crashing (BIP-340 rejects the zero scalar).
let pubkey;
try {
	const keyBytes = hexToBytes(secretKey);
	if (keyBytes.length !== 32) throw new Error('invalid key length');
	pubkey = bytesToHex(schnorr.getPublicKey(keyBytes));
} catch {
	console.error('FAILED: invalid secret key (expect 64 hex chars, scalar in [1, n-1])');
	process.exit(1);
}
const sha256 = async (input) => {
	const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
};
const sign = async (kind, tags, text) => {
	const event = { pubkey, created_at: Math.floor(Date.now() / 1000), kind, tags, content: text };
	const id = await sha256(
		JSON.stringify([0, event.pubkey, event.created_at, event.kind, event.tags, event.content])
	);
	const sig = bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(secretKey)));
	return { ...event, id, sig };
};

const signed = await sign(1, [], content);
const accepted = await new Promise((resolve) => {
	const socket = new WebSocket(relayUrl);
	const timer = setTimeout(() => {
		socket.close();
		resolve(false);
	}, 10_000);
	socket.onopen = () => socket.send(JSON.stringify(['REQ', 'auth-probe', { limit: 0 }]));
	socket.onmessage = async (raw) => {
		const message = JSON.parse(String(raw.data));
		if (!Array.isArray(message)) return;
		if (message[0] === 'AUTH' && typeof message[1] === 'string') {
			const auth = await sign(22242, [['relay', relayUrl], ['challenge', message[1]]], '');
			socket.send(JSON.stringify(['AUTH', auth]));
			socket.send(JSON.stringify(['EVENT', signed]));
		}
		if (message[0] === 'OK' && message[1] === signed.id) {
			clearTimeout(timer);
			socket.close();
			resolve(message[2] === true);
		}
	};
	socket.onerror = () => {
		clearTimeout(timer);
		resolve(false);
	};
	socket.onclose = () => {
		clearTimeout(timer);
		resolve(false);
	};
});

console.log(accepted ? `OK published ${signed.id}` : 'FAILED: relay rejected or unreachable');
process.exit(accepted ? 0 : 1);
