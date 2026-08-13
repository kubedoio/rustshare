// frontend/scripts/alpha-buzz-ops.mjs
// Wire-level Buzz relay operations for the Elembra Alpha dogfood E2E driver.
// Uses the same NIP-42 / NIP-43 / NIP-01 wire contracts as the Buzz bridge
// (backend/server/src/buzz_bridge.rs) and the browser client, so the driver
// exercises the real relay behavior without a browser.
//
// Commands:
//   keygen                                    generate a fresh keypair (SK, PK)
//   bind-proof <relay-url> <challenge> <sk>   sign a kind-22242 NIP-42 proof
//   admit <relay-url> <owner-sk> <pk>         kind-9030 add member (owner authority)
//   revoke <relay-url> <owner-sk> <pk>        kind-9031 remove member (owner authority)
//   publish <relay-url> <sk> <content> [channel] [elembra-ref]
//                                             signed kind-1 publish; prints event id
//
// Every command prints JSON to stdout; exit 0 on success, 1 on relay
// rejection, 2 on usage errors.
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const [, , command, ...args] = process.argv;

const sha256Hex = async (input) => {
	const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
};

const pubkeyOf = (skHex) => {
	const keyBytes = hexToBytes(skHex);
	if (keyBytes.length !== 32) throw new Error('invalid key length');
	return bytesToHex(schnorr.getPublicKey(keyBytes));
};

const mkEvent = async (kind, tags, content, skHex) => {
	const pubkey = pubkeyOf(skHex);
	const event = { pubkey, created_at: Math.floor(Date.now() / 1000), kind, tags, content };
	const id = await sha256Hex(
		JSON.stringify([0, event.pubkey, event.created_at, event.kind, event.tags, event.content])
	);
	return { ...event, id, sig: bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(skHex))) };
};

// Send one command over NIP-42 to the relay; resolves with the final OK frame
// for `eventId` (true/false + reason), retrying the send once after AUTH.
const sendCommand = (relayUrl, event, authSk) =>
	new Promise((resolve) => {
		const ws = new WebSocket(relayUrl);
		const timer = setTimeout(() => {
			ws.close();
			resolve({ accepted: false, reason: 'timeout' });
		}, 15000);
		ws.onopen = () => ws.send(JSON.stringify(['EVENT', event]));
		ws.onmessage = (raw) => {
			let message;
			try {
				message = JSON.parse(String(raw.data));
			} catch {
				return;
			}
			if (!Array.isArray(message)) return;
			if (message[0] === 'AUTH' && typeof message[1] === 'string') {
				mkEvent(
					22242,
					[
						['relay', relayUrl],
						['challenge', message[1]]
					],
					'',
					authSk
				).then((auth) => {
					ws.send(JSON.stringify(['AUTH', auth]));
					ws.send(JSON.stringify(['EVENT', event]));
				});
			}
			if (message[0] === 'OK' && message[1] === event.id && message[2] === true) {
				clearTimeout(timer);
				ws.close();
				resolve({ accepted: true, reason: '' });
			} else if (message[0] === 'OK' && message[1] === event.id && message[2] === false) {
				const reason = message[3] || 'rejected';
				// The first rejection for an unauthenticated send is the relay
				// demanding NIP-42 auth; the AUTH + resend happens above and the
				// outcome arrives in a later OK for the same event id.
				if (reason.toLowerCase().includes('auth')) return;
				clearTimeout(timer);
				ws.close();
				resolve({ accepted: false, reason });
			}
		};
		ws.onerror = () => resolve({ accepted: false, reason: 'transport' });
	});

if (command === 'keygen') {
	const sk = bytesToHex(crypto.getRandomValues(new Uint8Array(32)));
	console.log(JSON.stringify({ secretKey: sk, pubkey: pubkeyOf(sk) }));
	process.exit(0);
}

if (command === 'pubkey') {
	const [skHex] = args;
	if (!skHex) {
		console.error('usage: alpha-buzz-ops.mjs pubkey <sk>');
		process.exit(2);
	}
	console.log(pubkeyOf(skHex));
	process.exit(0);
}

if (command === 'bind-proof') {
	const [relayUrl, challenge, skHex] = args;
	if (!relayUrl || !challenge || !skHex) {
		console.error('usage: alpha-buzz-ops.mjs bind-proof <relay-url> <challenge> <sk>');
		process.exit(2);
	}
	const proof = await mkEvent(
		22242,
		[
			['relay', relayUrl],
			['challenge', challenge]
		],
		'',
		skHex
	);
	console.log(JSON.stringify(proof));
	process.exit(0);
}

if (command === 'admit' || command === 'revoke') {
	const [relayUrl, ownerSk, targetPk] = args;
	if (!relayUrl || !ownerSk || !targetPk) {
		console.error(`usage: alpha-buzz-ops.mjs ${command} <relay-url> <owner-sk> <pk>`);
		process.exit(2);
	}
	const kind = command === 'admit' ? 9030 : 9031;
	const event = await mkEvent(
		kind,
		[['p', targetPk]],
		`${command} ${targetPk.slice(0, 8)}`,
		ownerSk
	);
	const result = await sendCommand(relayUrl, event, ownerSk);
	console.log(JSON.stringify({ ...result, kind, target: targetPk.slice(0, 8) }));
	process.exit(result.accepted ? 0 : 1);
}

if (command === 'publish') {
	const [relayUrl, skHex, content, channel, ref] = args;
	if (!relayUrl || !skHex || content === undefined) {
		console.error(
			'usage: alpha-buzz-ops.mjs publish <relay-url> <sk> <content> [channel] [elembra-ref]'
		);
		process.exit(2);
	}
	const tags = [];
	if (channel) tags.push(['channel', channel]);
	if (ref) tags.push(['elembra-ref', ref]);
	const event = await mkEvent(1, tags, content, skHex);
	const result = await sendCommand(relayUrl, event, skHex);
	console.log(JSON.stringify({ ...result, eventId: event.id, kind: 1, channel: channel || null }));
	process.exit(result.accepted ? 0 : 1);
}

console.error(`unknown command: ${command}`);
process.exit(2);
