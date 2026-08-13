// frontend/scripts/buzz-observer.mjs
// Relay → Elembra observation bridge for the Alpha dogfooding stack.
//
// The upstream Buzz relay has no webhook delivery, so "push-only observation"
// (Elembra's `POST /api/v1/integrations/buzz/events`) needs a small runtime
// bridge in front of the relay: this script subscribes to the relay as a
// NIP-01 client (authenticating with NIP-42 as the bridge/owner identity),
// and forwards every signed kind-1 event to Elembra's observation webhook,
// carrying the shared webhook HMAC. Elembra's webhook endpoint stays the
// authoritative verifier: HMAC + replay window + Nostr id/Schnorr signature +
// community/author mapping. This bridge only relays; it never verifies on
// behalf of Elembra and never reads or stores private keys beyond the bridge
// service key it needs to authenticate to the relay.
//
// Usage (node 22+; run from anywhere, module resolution needs the workspace
// frontend node_modules for @noble/curves):
//   BUZZ_RELAY_WS=ws://localhost:7447 \
//   RUSTSHARE_CHAT_WEBHOOK_SECRET=<shared secret> \
//   BUZZ_SERVICE_SK=<64-hex bridge service secret key> \
//   BUZZ_COMMUNITY_ID=<community id> \
//   ELEMBRA_WEBHOOK_URL=http://localhost/api/v1/integrations/buzz/events \
//   node scripts/buzz-observer.mjs
//
// Env:
//   BUZZ_RELAY_WS                relay websocket URL (required)
//   RUSTSHARE_CHAT_WEBHOOK_SECRET  shared webhook HMAC secret (required)
//   BUZZ_SERVICE_SK              64-hex bridge/owner secret key for NIP-42 AUTH (required)
//   BUZZ_COMMUNITY_ID            community id forwarded in the context (required;
//                                must equal the Elembra workspace↔community mapping)
//   BUZZ_CHANNEL_ID              channel id forwarded in the context (default alpha-channel)
//   ELEMBRA_WEBHOOK_URL          default http://localhost/api/v1/integrations/buzz/events
//   BUZZ_SINCE                   optional unix seconds; only events after this are forwarded
//   BUZZ_MAX_RECONNECT_BACKOFF_S default 30
//   BUZZ_HTTP_TIMEOUT_MS         default 10000
import { createHmac, randomUUID } from 'node:crypto';
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const relayUrl = process.env.BUZZ_RELAY_WS;
const webhookSecret = process.env.RUSTSHARE_CHAT_WEBHOOK_SECRET;
const serviceSk = process.env.BUZZ_SERVICE_SK;
const communityId = process.env.BUZZ_COMMUNITY_ID;
const channelId = process.env.BUZZ_CHANNEL_ID || 'alpha-channel';
const webhookUrl =
	process.env.ELEMBRA_WEBHOOK_URL || 'http://localhost/api/v1/integrations/buzz/events';
const since = process.env.BUZZ_SINCE ? Number(process.env.BUZZ_SINCE) : undefined;
const maxBackoffS = Number(process.env.BUZZ_MAX_RECONNECT_BACKOFF_S || 30);
const httpTimeoutMs = Number(process.env.BUZZ_HTTP_TIMEOUT_MS || 10000);

let missing = [];
if (!relayUrl) missing.push('BUZZ_RELAY_WS');
if (!webhookSecret) missing.push('RUSTSHARE_CHAT_WEBHOOK_SECRET');
if (!serviceSk) missing.push('BUZZ_SERVICE_SK');
if (!communityId) missing.push('BUZZ_COMMUNITY_ID');
if (missing.length) {
	console.error(`buzz-observer: missing required env: ${missing.join(', ')}`);
	process.exit(2);
}
const servicePubkey = (() => {
	try {
		const keyBytes = hexToBytes(serviceSk);
		if (keyBytes.length !== 32) throw new Error('invalid key length');
		return bytesToHex(schnorr.getPublicKey(keyBytes));
	} catch {
		console.error('buzz-observer: BUZZ_SERVICE_SK must be 64 hex chars (scalar in [1, n-1])');
		process.exit(1);
	}
})();

const sha256Hex = async (input) => {
	const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
};

const signEvent = async (kind, tags, content = '') => {
	const event = {
		pubkey: servicePubkey,
		created_at: Math.floor(Date.now() / 1000),
		kind,
		tags,
		content
	};
	const id = await sha256Hex(
		JSON.stringify([0, event.pubkey, event.created_at, event.kind, event.tags, event.content])
	);
	return { ...event, id, sig: bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(serviceSk))) };
};

// Serializes forwards so event order is preserved; failures are retried with
// backoff and logged (Elembra deduplicates by event id, so retries are safe).
let forwardChain = Promise.resolve();
const forwardEvent = (event) => {
	forwardChain = forwardChain.then(() => deliver(event));
	return forwardChain;
};

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function deliver(event) {
	// Channel attribution is bridge-side (the client publishes no channel tag —
	// spec §10). When the publisher carries a `channel` tag (as the E2E driver
	// and future bridge do), it wins over the configured default.
	const channelTag = Array.isArray(event.tags)
		? event.tags.find(
				(tag) => Array.isArray(tag) && tag[0] === 'channel' && typeof tag[1] === 'string'
			)
		: undefined;
	const context = {
		community_id: communityId,
		channel_id: channelTag ? channelTag[1] : channelId,
		channel_kind: 'workspace',
		thread_root_id: null,
		message_id: event.id,
		event_type: 'created',
		supersedes_event_id: null
	};
	const body = JSON.stringify({ event, context });
	const timestamp = Math.floor(Date.now() / 1000);
	// WebhookSigner::sign_with_timestamp: HMAC-SHA256 over `<ts>.<hex(body)>`
	const signed = `${timestamp}.${Buffer.from(body, 'utf8').toString('hex')}`;
	const signature = createHmac('sha256', webhookSecret).update(signed).digest('hex');

	for (let attempt = 1; ; attempt++) {
		let controller;
		try {
			controller = new AbortController();
			const timeout = setTimeout(() => controller.abort(), httpTimeoutMs);
			const response = await fetch(webhookUrl, {
				method: 'POST',
				headers: {
					'content-type': 'application/json',
					'x-rustshare-signature': `t=${timestamp},v1=${signature}`
				},
				body,
				signal: controller.signal
			});
			clearTimeout(timeout);
			const text = await response.text();
			if (response.ok) {
				console.log(
					`forwarded ${event.id.slice(0, 12)} kind=${event.kind} channel=${context.channel_id} -> ${response.status} ${text.trim()}`
				);
				return;
			}
			// 4xx are permanent (bad config, unknown community, unbound author);
			// retrying would only churn the logs. 5xx are transient.
			if (response.status < 500) {
				console.error(
					`forward failed ${event.id.slice(0, 12)} (permanent ${response.status}): ${text.trim()}`
				);
				return;
			}
			console.error(
				`forward failed ${event.id.slice(0, 12)} (transient ${response.status}), retry ${attempt}`
			);
		} catch (error) {
			console.error(
				`forward failed ${event.id.slice(0, 12)} (transport: ${error.message}), retry ${attempt}`
			);
		}
		const backoff = Math.min(2 ** attempt, 15) * 1000;
		await sleep(backoff);
	}
}

let reconnectAttempt = 0;

// Registered once at module scope so reconnects do not accumulate listeners.
let shuttingDown = false;
const shutdown = () => {
	shuttingDown = true;
	console.log('shutting down');
	process.exit(0);
};
process.once('SIGINT', shutdown);
process.once('SIGTERM', shutdown);

// A bridge process must never die silently: log and continue. Node >= 15
// terminates on unhandled rejections by default; the async relay handler must
// not take the observation bridge down with it.
process.on('unhandledRejection', (reason) => {
	console.error(
		`unhandled rejection (continuing): ${reason instanceof Error ? reason.stack : reason}`
	);
});
process.on('uncaughtException', (error) => {
	console.error(`uncaught exception (continuing): ${error.stack}`);
});

function connect() {
	const socket = new WebSocket(relayUrl);
	let subscribed = false;
	let closing = shuttingDown;
	const reqId = `buzz-observer-${randomUUID().slice(0, 8)}`;

	const subscribe = () => {
		if (subscribed) return;
		subscribed = true;
		const filter = { kinds: [1] };
		if (since !== undefined) filter.since = since;
		try {
			socket.send(JSON.stringify(['REQ', reqId, filter]));
			console.log(`subscribed ${reqId} kinds=[1] since=${since ?? 'all'}`);
		} catch (error) {
			subscribed = false;
			console.error(`REQ send failed: ${error.message}`);
		}
	};

	socket.onmessage = async (raw) => {
		try {
			let message;
			try {
				message = JSON.parse(String(raw.data));
			} catch {
				return;
			}
			if (!Array.isArray(message)) return;
			const [kind] = message;
			if (kind === 'AUTH' && typeof message[1] === 'string') {
				const auth = await signEvent(
					22242,
					[
						['relay', relayUrl],
						['challenge', message[1]]
					],
					''
				);
				try {
					socket.send(JSON.stringify(['AUTH', auth]));
					// The pre-auth REQ was rejected (NOTICE + CLOSED for the
					// subscription); re-issue it now that this connection is
					// authenticated (same pattern as the Buzz bridge re-sending
					// its command after AUTH).
					subscribed = false;
					subscribe();
				} catch (error) {
					console.error(`AUTH send failed: ${error.message}`);
				}
				console.log('authenticated with relay challenge (NIP-42)');
			} else if (kind === 'EVENT') {
				// NIP-01 frame shape: ["EVENT", <subscription-id>, <event>]
				const event = message[2];
				if (event && typeof event === 'object' && event.kind === 1) {
					// Forward without awaiting: ordering is preserved by the promise chain.
					forwardEvent(event).catch((error) => {
						console.error(`unexpected forward error: ${error.message}`);
					});
				}
			} else if (kind === 'NOTICE' || kind === 'OK' || kind === 'EOSE' || kind === 'CLOSED') {
				console.log(`relay ${kind}: ${JSON.stringify(message).slice(0, 300)}`);
			}
		} catch (error) {
			console.error(`relay frame handler error: ${error.message}`);
		}
	};

	socket.onerror = (event) => {
		console.error(`websocket error: ${event.message || 'unknown'}`);
		// A rejected upgrade or network failure may not be followed by an
		// onclose event; force the close so the reconnect loop always runs.
		try {
			socket.close();
		} catch {
			// already closed
		}
	};

	// Watchdog: if the connection neither opens nor closes within the window
	// (half-open socket), force a close so the reconnect loop progresses.
	const connectWatchdog = setTimeout(() => {
		if (!closing && socket.readyState !== WebSocket.OPEN) {
			console.error('websocket connect watchdog: forcing close');
			try {
				socket.close();
			} catch {
				// already closed
			}
		}
	}, 15_000);

	socket.onopen = () => {
		clearTimeout(connectWatchdog);
		reconnectAttempt = 0;
		console.log(`connected to ${relayUrl}`);
		// Some relays deliver the AUTH challenge only after a first REQ.
		subscribe();
	};

	socket.onclose = () => {
		clearTimeout(connectWatchdog);
		if (closing || shuttingDown) return;
		reconnectAttempt++;
		const backoff = Math.min(2 ** reconnectAttempt, maxBackoffS) * 1000;
		console.log(
			`websocket closed; reconnecting in ${backoff / 1000}s (attempt ${reconnectAttempt})`
		);
		setTimeout(connect, backoff);
	};
}

console.log(
	`buzz-observer starting: relay=${relayUrl} community=${communityId} channel=${channelId} webhook=${webhookUrl}`
);
connect();
