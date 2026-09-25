// Relay -> Elembra observation bridge for the bundled Chat deployment.
//
// This process is transport only. Elembra verifies the webhook HMAC, the
// signed Buzz event, the workspace mapping, and the event's author binding.
// The bridge authenticates to Buzz as the dedicated service identity, uses
// Buzz's signed public registry/state APIs, and forwards the resulting events.
import { createHash, createHmac, randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const relayWs = process.env.BUZZ_RELAY_WS;
const webhookSecret = process.env.RUSTSHARE_CHAT_WEBHOOK_SECRET;
const serviceSk = process.env.BUZZ_SERVICE_SK;
const pinnedRelayPubkey = process.env.BUZZ_RELAY_PUBKEY;
const webhookUrl =
	process.env.ELEMBRA_WEBHOOK_URL || 'http://localhost:8080/api/v1/integrations/buzz/events';
const healthPort = Number(process.env.BUZZ_OBSERVER_HEALTH_PORT || 8091);
const pollMs = Math.max(5000, Number(process.env.BUZZ_CHANNEL_POLL_MS || 15000));
const maxBackoffS = Math.max(1, Number(process.env.BUZZ_MAX_RECONNECT_BACKOFF_S || 30));
const httpTimeoutMs = Math.max(1000, Number(process.env.BUZZ_HTTP_TIMEOUT_MS || 10000));
const since = process.env.BUZZ_SINCE ? Number(process.env.BUZZ_SINCE) : undefined;

const uuidRe = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const hex64Re = /^[0-9a-f]{64}$/;
const hex128Re = /^[0-9a-f]{128}$/;
const required = [
	['BUZZ_RELAY_WS', relayWs],
	['RUSTSHARE_CHAT_WEBHOOK_SECRET', webhookSecret],
	['BUZZ_SERVICE_SK', serviceSk],
	['BUZZ_RELAY_PUBKEY', pinnedRelayPubkey]
]
	.filter(([, value]) => !value)
	.map(([name]) => name);
if (required.length) {
	console.error(`buzz-observer: missing required configuration: ${required.join(', ')}`);
	process.exit(2);
}
if (!hex64Re.test(pinnedRelayPubkey)) {
	console.error('buzz-observer: BUZZ_RELAY_PUBKEY must be 64 lowercase hexadecimal characters');
	process.exit(2);
}
if (since !== undefined && !Number.isFinite(since)) {
	console.error('buzz-observer: BUZZ_SINCE must be a unix-seconds number');
	process.exit(2);
}

let servicePubkey;
try {
	const keyBytes = hexToBytes(serviceSk);
	if (keyBytes.length !== 32) throw new Error('invalid length');
	servicePubkey = bytesToHex(schnorr.getPublicKey(keyBytes));
} catch {
	console.error('buzz-observer: BUZZ_SERVICE_SK must be a valid 64-hex Schnorr scalar');
	process.exit(2);
}

const relayHttp = new URL(relayWs);
if (relayHttp.protocol === 'ws:') relayHttp.protocol = 'http:';
else if (relayHttp.protocol === 'wss:') relayHttp.protocol = 'https:';
else {
	console.error('buzz-observer: BUZZ_RELAY_WS must use ws:// or wss://');
	process.exit(2);
}
relayHttp.pathname = '/';
relayHttp.search = '';
relayHttp.hash = '';

const state = {
	connection: 'starting',
	auth: 'starting',
	community: null,
	registry: 'starting',
	lastRecoveryAt: null,
	lastEventAt: null,
	lastError: null,
	observation: 'starting',
	changedCommunity: false
};
let shuttingDown = false;
let socket = null;
let pollTimer = null;
let reconcileTimer = null;
let healthServer = null;
let reconcileRunning = false;
let reconnectAttempt = 0;
let forwardChain = Promise.resolve();
let discoveredChannels = new Set();
let lastReconcileSince = since;
const deliveredEventKeys = new Set();
const pendingEventKeys = new Set();
const MAX_DELIVERED_KEYS = 10000;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function logError(message, error) {
	state.lastError = message;
	console.error(`buzz-observer: ${message}${error ? `: ${error.message}` : ''}`);
}

function signedEvent(kind, tags, content = '') {
	const event = {
		pubkey: servicePubkey,
		created_at: Math.floor(Date.now() / 1000),
		kind,
		tags,
		content
	};
	const id = createHash('sha256')
		.update(
			JSON.stringify([0, event.pubkey, event.created_at, event.kind, event.tags, event.content])
		)
		.digest('hex');
	return {
		...event,
		id,
		sig: bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(serviceSk)))
	};
}

async function nip98Header(method, url) {
	const auth = await signedEvent(27235, [
		['u', url],
		['method', method],
		['nonce', randomUUID()]
	]);
	return `Nostr ${Buffer.from(JSON.stringify(auth), 'utf8').toString('base64')}`;
}

function verifyRelayEnvelope(raw, requireFresh = true) {
	if (!raw || typeof raw !== 'object' || raw.kind !== 19030) {
		throw new Error('relay response is not a kind-19030 event');
	}
	if (raw.pubkey !== pinnedRelayPubkey || !hex64Re.test(raw.pubkey)) {
		throw new Error('relay response pubkey does not match the pinned relay identity');
	}
	const expectedId = createHash('sha256')
		.update(JSON.stringify([0, raw.pubkey, raw.created_at, raw.kind, raw.tags, raw.content]))
		.digest('hex');
	if (!hex64Re.test(raw.id) || raw.id !== expectedId)
		throw new Error('relay response event id verification failed');
	if (!hex128Re.test(raw.sig)) throw new Error('relay response signature is malformed');
	if (!schnorr.verify(hexToBytes(raw.sig), hexToBytes(raw.id), hexToBytes(raw.pubkey)))
		throw new Error('relay response signature verification failed');
	let content;
	try {
		content = JSON.parse(raw.content);
	} catch {
		throw new Error('relay response content is not JSON');
	}
	if (requireFresh) {
		const age = Math.floor(Date.now() / 1000) - Number(content.evaluated_at);
		if (!Number.isInteger(content.evaluated_at) || age < 0 || age > 60) {
			throw new Error(
				`relay response is stale or from the future (evaluated_at=${content.evaluated_at})`
			);
		}
	}
	return content;
}

async function relayGet(path, options = {}) {
	const url = new URL(path, relayHttp).toString();
	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), httpTimeoutMs);
	try {
		const response = await fetch(url, {
			headers: { Authorization: await nip98Header('GET', url) },
			signal: controller.signal
		});
		if (!response.ok) throw new Error(`relay HTTP ${response.status}`);
		return verifyRelayEnvelope(await response.json(), options.requireFresh !== false);
	} finally {
		clearTimeout(timer);
	}
}

async function discoverCommunity() {
	const identity = await relayGet('/api/v1/relay/community');
	if (!uuidRe.test(identity.community_id) || identity.relay_pubkey !== pinnedRelayPubkey) {
		throw new Error('signed community discovery does not match the configured relay identity');
	}
	if (state.community && state.community !== identity.community_id) {
		state.changedCommunity = true;
		logError(
			`relay community changed from ${state.community} to ${identity.community_id}; Elembra mapping must be reprovisioned`
		);
	}
	state.community = identity.community_id;
}

async function discoverChannels() {
	const query = `/api/v1/relay/channels?pubkey=${encodeURIComponent(servicePubkey)}`;
	const registry = await relayGet(query);
	if (registry.pubkey !== servicePubkey || !Array.isArray(registry.channels)) {
		throw new Error('signed channel registry is malformed');
	}
	const channels = new Set();
	for (const channel of registry.channels) {
		if (channel && typeof channel.channel_id === 'string' && uuidRe.test(channel.channel_id)) {
			channels.add(channel.channel_id);
		}
	}
	state.registry = 'ready';
	discoveredChannels = channels;
	applySubscriptions();
}

function contextForEvent(event, channelId, override = {}) {
	const tag = (name) =>
		Array.isArray(event.tags)
			? event.tags.find(
					(item) => Array.isArray(item) && item[0] === name && typeof item[1] === 'string'
				)
			: undefined;
	const h = event.kind === 9 || event.kind === 40002 ? tag('h') : undefined;
	return {
		community_id: state.community,
		channel_id: h?.[1] || tag('channel')?.[1] || channelId,
		channel_kind: 'workspace',
		thread_root_id: null,
		message_id: event.id,
		event_type: 'created',
		supersedes_event_id: null,
		...override
	};
}

function forwardEvent(event, context = contextForEvent(event)) {
	if (!event?.id || state.changedCommunity) return;
	const eventKey = `${event.id}:${context.event_type || 'created'}`;
	if (deliveredEventKeys.has(eventKey) || pendingEventKeys.has(eventKey)) return;
	pendingEventKeys.add(eventKey);
	forwardChain = forwardChain
		.then(async () => {
			const delivered = await deliver(event, context);
			pendingEventKeys.delete(eventKey);
			if (!delivered) return;
			deliveredEventKeys.add(eventKey);
			if (deliveredEventKeys.size > MAX_DELIVERED_KEYS)
				deliveredEventKeys.delete(deliveredEventKeys.values().next().value);
		})
		.catch((error) => {
			pendingEventKeys.delete(eventKey);
			state.observation = 'degraded';
			logError('forward chain failed', error);
		});
}

async function deliver(event, context) {
	const body = JSON.stringify({ event, context });
	for (let attempt = 1; attempt <= 10; attempt += 1) {
		const timestamp = Math.floor(Date.now() / 1000);
		const signature = createHmac('sha256', webhookSecret)
			.update(`${timestamp}.${Buffer.from(body, 'utf8').toString('hex')}`)
			.digest('hex');
		const controller = new AbortController();
		const timer = setTimeout(() => controller.abort(), httpTimeoutMs);
		try {
			const response = await fetch(webhookUrl, {
				method: 'POST',
				headers: {
					'content-type': 'application/json',
					'x-rustshare-signature': `t=${timestamp},v1=${signature}`
				},
				body,
				signal: controller.signal
			});
			if (response.ok) {
				state.observation = 'ready';
				state.lastError = null;
				state.lastEventAt = new Date().toISOString();
				return true;
			}
			if (response.status < 500) {
				state.observation = 'degraded';
				logError(`Elembra rejected event ${event.id.slice(0, 12)} with HTTP ${response.status}`);
				return false;
			}
			if (attempt === 10) {
				state.observation = 'degraded';
				logError(`Elembra stayed unavailable for event ${event.id.slice(0, 12)}`);
			}
		} catch (error) {
			if (attempt === 10) {
				state.observation = 'degraded';
				logError(`Elembra stayed unavailable for event ${event.id.slice(0, 12)}`, error);
			}
		} finally {
			clearTimeout(timer);
		}
		await sleep(Math.min(15000, 2 ** attempt * 1000));
	}
	return false;
}

async function reconcile() {
	if (reconcileRunning || !state.community || state.changedCommunity) return;
	reconcileRunning = true;
	try {
		let cursor;
		let pages = 0;
		let newest = lastReconcileSince;
		do {
			const query = new URL('/api/v1/relay/state/events', relayHttp);
			if (lastReconcileSince !== undefined)
				query.searchParams.set('since', String(lastReconcileSince));
			query.searchParams.set('limit', '500');
			if (cursor) query.searchParams.set('cursor', cursor);
			const page = await relayGet(`${query.pathname}${query.search}`, {
				requireFresh: false
			});
			if (!Array.isArray(page.entries)) throw new Error('signed state page is malformed');
			for (const entry of page.entries) {
				if (!entry?.event || !entry.context || entry.context.community_id !== state.community)
					continue;
				forwardEvent(entry.event, entry.context);
				const created = Number(entry.event.created_at);
				if (Number.isFinite(created)) newest = Math.max(newest ?? created, created);
			}
			cursor = page.complete ? undefined : page.cursor;
			if (!page.complete && !cursor) throw new Error('incomplete state page has no cursor');
			pages += 1;
		} while (cursor && pages < 100);
		if (cursor) throw new Error('state reconciliation exceeded the 100-page limit');
		// Readiness means the bounded replay has reached Elembra, not merely that
		// its events have been placed behind a potentially slow delivery chain.
		await forwardChain;
		lastReconcileSince = newest;
		state.lastRecoveryAt = new Date().toISOString();
		if (state.observation !== 'degraded') {
			state.observation = 'ready';
			state.lastError = null;
		}
	} catch (error) {
		logError('Buzz state reconciliation failed', error);
	} finally {
		reconcileRunning = false;
	}
}

function applySubscriptions() {
	if (!socket || socket.readyState !== WebSocket.OPEN || !socket.authenticated) return;
	const wanted = new Set(discoveredChannels);
	for (const [channelId, subscriptionId] of socket.subscriptions) {
		if (!wanted.has(channelId)) {
			try {
				socket.send(JSON.stringify(['CLOSE', subscriptionId]));
			} catch {
				/* reconnect handles it */
			}
			socket.subscriptions.delete(channelId);
		}
	}
	for (const channelId of wanted) {
		if (socket.subscriptions.has(channelId)) continue;
		const subscriptionId = `${socket.requestId}-${channelId.slice(0, 8)}`;
		socket.subscriptions.set(channelId, subscriptionId);
		const filter = { kinds: [9, 40002], '#h': [channelId] };
		if (since !== undefined) filter.since = since;
		try {
			socket.send(JSON.stringify(['REQ', subscriptionId, filter]));
		} catch (error) {
			socket.subscriptions.delete(channelId);
			logError('channel subscription failed', error);
		}
	}
}

function connect() {
	if (shuttingDown) return;
	const ws = new WebSocket(relayWs);
	ws.requestId = `buzz-observer-${randomUUID().slice(0, 8)}`;
	ws.subscriptions = new Map();
	ws.authenticated = false;
	ws.authEventId = null;
	socket = ws;
	state.connection = 'connecting';
	state.auth = 'starting';
	let eoseSeen = false;
	let reconnectScheduled = false;
	const scheduleReconnect = () => {
		if (reconnectScheduled) return;
		reconnectScheduled = true;
		clearTimeout(watchdog);
		if (shuttingDown) return;
		state.connection = state.auth === 'failed' ? 'auth_failed' : 'reconnecting';
		reconnectAttempt += 1;
		if (reconnectAttempt >= 3) {
			logError('relay unavailable after three attempts; exiting for Compose restart');
			process.exit(1);
		}
		const backoff = Math.min(2 ** reconnectAttempt, maxBackoffS) * 1000;
		setTimeout(connect, backoff);
	};
	const watchdog = setTimeout(() => {
		if (ws.readyState !== WebSocket.OPEN) {
			logError('relay connection watchdog expired');
			try {
				ws.close();
			} catch {
				/* already closed */
			}
			setTimeout(scheduleReconnect, 1000);
		}
	}, 15000);
	ws.onopen = () => {
		clearTimeout(watchdog);
		reconnectAttempt = 0;
		state.connection = 'connected';
		try {
			ws.send(JSON.stringify(['REQ', ws.requestId, { kinds: [9], limit: 1 }]));
		} catch (error) {
			logError('relay authentication request failed', error);
		}
		applySubscriptions();
		void reconcile();
	};
	ws.onmessage = async (raw) => {
		try {
			const message = JSON.parse(String(raw.data));
			if (!Array.isArray(message)) return;
			if (message[0] === 'AUTH' && typeof message[1] === 'string') {
				const auth = await signedEvent(22242, [
					['relay', relayWs],
					['challenge', message[1]]
				]);
				ws.send(JSON.stringify(['AUTH', auth]));
				ws.authEventId = auth.id;
				state.auth = 'pending';
				return;
			}
			if (message[0] === 'OK' && message[1] === ws.authEventId) {
				if (message[2] !== true) {
					state.auth = 'failed';
					state.connection = 'auth_failed';
					logError('relay rejected observer authentication');
					ws.close();
					return;
				}
				ws.authenticated = true;
				state.auth = 'authenticated';
				if (state.observation !== 'degraded') state.lastError = null;
				applySubscriptions();
				void reconcile();
				return;
			}
			if (message[0] === 'EVENT') {
				const event = message[2];
				const subscriptionId = message[1];
				const liveSubscription = [...ws.subscriptions.values()].includes(subscriptionId);
				if (liveSubscription && event && (event.kind === 9 || event.kind === 40002))
					forwardEvent(event);
				return;
			}
			if (message[0] === 'EOSE') {
				eoseSeen = true;
				return;
			}
			if (message[0] === 'CLOSED' && eoseSeen) {
				logError('relay closed a live subscription; reconnecting');
				try {
					ws.close();
				} catch {
					/* already closed */
				}
			}
		} catch (error) {
			logError('relay frame handler failed', error);
		}
	};
	ws.onerror = () => {
		try {
			ws.close();
		} catch {
			/* already closed */
		}
	};
	ws.onclose = () => {
		scheduleReconnect();
	};
}

function startHealthServer() {
	healthServer = createServer((request, response) => {
		if (request.url !== '/health' && request.url !== '/ready') {
			response.writeHead(404).end();
			return;
		}
		const ready =
			state.connection === 'connected' &&
			state.auth === 'authenticated' &&
			state.registry === 'ready' &&
			!!state.community &&
			state.observation !== 'degraded' &&
			!state.changedCommunity;
		const body = JSON.stringify({
			status: ready ? 'ready' : 'degraded',
			connection: state.connection,
			auth: state.auth,
			registry: state.registry,
			community_discovered: !!state.community,
			last_recovery_at: state.lastRecoveryAt,
			last_event_at: state.lastEventAt,
			last_error: state.lastError,
			observation: state.observation,
			community_changed: state.changedCommunity
		});
		response.writeHead(request.url === '/ready' && !ready ? 503 : 200, {
			'content-type': 'application/json',
			'cache-control': 'no-store'
		});
		response.end(body);
	});
	healthServer.listen(healthPort, '0.0.0.0');
}

async function refresh() {
	try {
		await discoverCommunity();
		await discoverChannels();
		await reconcile();
	} catch (error) {
		state.registry = 'degraded';
		logError('Buzz discovery failed', error);
	}
}

async function shutdown() {
	if (shuttingDown) return;
	shuttingDown = true;
	clearInterval(pollTimer);
	clearInterval(reconcileTimer);
	try {
		socket?.close();
	} catch {
		/* already closed */
	}
	if (healthServer) await new Promise((resolve) => healthServer.close(resolve));
}
process.once('SIGINT', () => void shutdown().finally(() => process.exit(0)));
process.once('SIGTERM', () => void shutdown().finally(() => process.exit(0)));
process.on('uncaughtException', (error) => {
	logError('uncaught observer exception', error);
	process.exit(1);
});
process.on('unhandledRejection', (error) => logError('unhandled observer rejection', error));

startHealthServer();
void (async () => {
	await refresh();
	connect();
	pollTimer = setInterval(() => void refresh(), pollMs);
	reconcileTimer = setInterval(() => void reconcile(), pollMs);
})();
