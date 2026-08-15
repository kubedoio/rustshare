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
//   create-channel <relay-url> <channel-uuid> <name> <visibility> <channel_type>
//                                             kind-9007 create-group (owner
//                                             authority, BUZZ_SERVICE_SK); a
//                                             re-run of an existing channel is
//                                             reported as accepted:false
//                                             "duplicate: channel already exists"
//                                             (idempotent provisioning)
//   publish <relay-url> <sk> <content> [channel] [elembra-ref]
//                                             signed kind-9 stream-message publish
//                                             scoped by ["h", <channel-uuid>]
//                                             (kind-1 legacy note otherwise);
//                                             prints event id
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
		ws.onerror = () => {
			clearTimeout(timer);
			resolve({ accepted: false, reason: 'transport' });
		};
		ws.onclose = () => {
			clearTimeout(timer);
			// No OK arrived before the connection closed; resolve so the caller
			// never hangs (idempotent if an OK already resolved).
			resolve({ accepted: false, reason: 'connection closed' });
		};
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
	// The owner/bridge key is a durable service secret: prefer the environment
	// (BUZZ_SERVICE_SK — same key as RUSTSHARE_CHAT_BRIDGE_SECRET_KEY) so it
	// does not appear on argv, in `ps`, or in shell history. Two forms are
	// accepted: <relay-url> <pk> (owner-sk from BUZZ_SERVICE_SK) or the fully
	// explicit <relay-url> <owner-sk> <pk>.
	const [relayUrl, a, b] = args;
	const ownerSk = args.length === 3 ? a : process.env.BUZZ_SERVICE_SK;
	const targetPk = args.length === 3 ? b : a;
	if (!relayUrl || !ownerSk || !targetPk) {
		console.error(
			`usage: alpha-buzz-ops.mjs ${command} <relay-url> [owner-sk] <pk>  (owner-sk defaults to BUZZ_SERVICE_SK)`
		);
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

if (command === 'create-channel') {
	// kind-9007 (NIP-29 create-group), owner authority — same key handling as
	// admit/revoke (owner-sk from BUZZ_SERVICE_SK). The relay creates the
	// channel under the client-chosen UUID from the `h` tag; a re-run of an
	// existing channel is answered accepted:false "duplicate: channel already
	// exists", which callers treat as success (idempotent provisioning).
	const [relayUrl, channelUuid, name, visibility, channelType] = args;
	const ownerSk = process.env.BUZZ_SERVICE_SK;
	if (!relayUrl || !channelUuid || !name || !visibility || !channelType || !ownerSk) {
		console.error(
			'usage: alpha-buzz-ops.mjs create-channel <relay-url> <channel-uuid> <name> <visibility> <channel_type>  (owner-sk from BUZZ_SERVICE_SK)'
		);
		process.exit(2);
	}
	const event = await mkEvent(
		9007,
		[
			['h', channelUuid],
			['name', name],
			['visibility', visibility],
			['channel_type', channelType]
		],
		`create-channel ${name}`,
		ownerSk
	);
	const result = await sendCommand(relayUrl, event, ownerSk);
	console.log(JSON.stringify({ ...result, kind: 9007, channel: channelUuid }));
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
	// Canonical chat wire format (spec: "Canonical publish tags and kinds"):
	// kind 9 with the NIP-29 `h` tag scoping the message to the channel. The
	// relay parses `h` strictly as a UUID, so a name-based channel falls back
	// to a legacy kind-1 note carrying the `channel` attribution tag (served
	// by the observation path until the authoritative registry supplies
	// channel UUIDs).
	const isUuid = (value) =>
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
	const streamScoped = !!channel && isUuid(channel);
	const tags = [];
	if (streamScoped) tags.push(['h', channel]);
	if (!streamScoped && channel) tags.push(['channel', channel]);
	if (ref) tags.push(['elembra-ref', ref]);
	const event = await mkEvent(streamScoped ? 9 : 1, tags, content, skHex);
	const result = await sendCommand(relayUrl, event, skHex);
	console.log(
		JSON.stringify({ ...result, eventId: event.id, kind: event.kind, channel: channel || null })
	);
	process.exit(result.accepted ? 0 : 1);
}

console.error(`unknown command: ${command}`);
process.exit(2);
