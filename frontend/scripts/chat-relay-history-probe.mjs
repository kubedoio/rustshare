import { createHash } from 'node:crypto';
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const relayUrl = process.env.BUZZ_RELAY_WS;
const channelId = process.env.BUZZ_RECOVERY_CHANNEL_ID;
const secretKey = process.env.BUZZ_SERVICE_SK;
if (!relayUrl || !channelId || !secretKey) {
	throw new Error('BUZZ_RELAY_WS, BUZZ_RECOVERY_CHANNEL_ID and BUZZ_SERVICE_SK are required');
}

const pubkey = bytesToHex(schnorr.getPublicKey(hexToBytes(secretKey)));
const sub = `g0${Math.random().toString(36).slice(2, 10)}`;
const filter = { '#h': [channelId], limit: 5000 };
const events = [];
const ws = new WebSocket(relayUrl);
let authId = null;
let authenticated = false;
const result = await new Promise((resolve, reject) => {
	const timer = setTimeout(() => {
		ws.close();
		reject(new Error('relay history query timed out'));
	}, 20_000);
	ws.onopen = () => ws.send(JSON.stringify(['REQ', sub, filter]));
	ws.onerror = () => reject(new Error('relay history query transport failed'));
	ws.onmessage = async (raw) => {
		let message;
		try {
			message = JSON.parse(String(raw.data));
		} catch {
			return;
		}
		if (!Array.isArray(message)) return;
		if (message[0] === 'AUTH' && typeof message[1] === 'string' && !authId) {
			const unsigned = {
				pubkey,
				created_at: Math.floor(Date.now() / 1000),
				kind: 22242,
				tags: [
					['relay', relayUrl],
					['challenge', message[1]],
				],
				content: '',
			};
			const id = createHash('sha256')
				.update(
					JSON.stringify([
						0,
						unsigned.pubkey,
						unsigned.created_at,
						unsigned.kind,
						unsigned.tags,
						'',
					]),
				)
				.digest('hex');
			authId = id;
			ws.send(
				JSON.stringify([
					'AUTH',
					{
						...unsigned,
						id,
						sig: bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(secretKey))),
					},
				]),
			);
			return;
		}
		if (message[0] === 'OK' && authId && message[1] === authId) {
			if (message[2] !== true) {
				clearTimeout(timer);
				return reject(new Error(`relay NIP-42 auth rejected: ${message[3]}`));
			}
			authenticated = true;
			ws.send(JSON.stringify(['REQ', sub, filter]));
			return;
		}
		if (message[0] === 'EVENT' && message[1] === sub) events.push(message[2]);
		if (message[0] === 'EOSE' && message[1] === sub) {
			clearTimeout(timer);
			resolve(events);
		}
		if (message[0] === 'CLOSED' && message[1] === sub) {
			const detail = String(message[2] ?? '');
			if (!authenticated && /auth/i.test(detail)) return;
			clearTimeout(timer);
			reject(new Error(`relay closed history query: ${detail}`));
		}
	};
});
ws.close();

if (result.length === 0) throw new Error(`no events returned for recovery channel ${channelId}`);
const canonical = result
	.map((event) => [
		event.id,
		event.pubkey,
		event.sig,
		event.created_at,
		event.kind,
		event.tags,
		event.content,
	])
	.sort((a, b) => String(a[0]).localeCompare(String(b[0])));
for (const event of result) {
	const calculatedId = createHash('sha256')
		.update(
			JSON.stringify([0, event.pubkey, event.created_at, event.kind, event.tags, event.content]),
		)
		.digest('hex');
	if (
		calculatedId !== event.id ||
		!schnorr.verify(hexToBytes(event.sig), hexToBytes(event.id), hexToBytes(event.pubkey))
	) {
		throw new Error(`relay returned an invalid signed event ${event.id}`);
	}
}
console.log(
	JSON.stringify({
		channelId,
		eventCount: canonical.length,
		eventIds: canonical.map(([id]) => id),
		eventAuthors: canonical.map(([id, author]) => [id, author]),
		eventSetSha256: createHash('sha256').update(JSON.stringify(canonical)).digest('hex'),
	}),
);
