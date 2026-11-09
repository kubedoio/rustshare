// Direct-relay negative acceptance against the CANDIDATE Buzz relay image.
// =============================================================================
// Scenarios the Elembra acceptance stack cannot provision, run against a
// disposable two-community relay (docker-compose.chat-direct-negative.yml,
// service `relay` on 127.0.0.1:${CHAT_DIRECT_NEG_RELAY_PORT:-17547}):
//
//   guest-role write denial  — owner-signed kind 9000 with ['role','guest']
//     provisions a read-only guest; the guest's signed kind-9 MUST be rejected
//     by the candidate relay (the guest-write-denial hotfix), while guest reads still
//     work and a member write still succeeds (positive controls).
//   two-tenant isolation     — communities A and B live on ONE relay process,
//     keyed by Host header (buzz nip42_host_binding_live pattern). Tenant B
//     must not discover, read, or write tenant A's channels, and a NIP-42 AUTH
//     whose relay tag names the other tenant's host MUST be rejected.
//
// All keys are generated in-test (crypto.getRandomValues) and exist only for
// this run. Evidence JSON is written to CHAT_DIRECT_NEG_EVIDENCE (also on
// failure) and the process exits non-zero if any check FAILs.
// =============================================================================
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const PORT = process.env.CHAT_DIRECT_NEG_RELAY_PORT ?? '17547';
const HOST_A = `a.localhost:${PORT}`;
const HOST_B = `b.localhost:${PORT}`;
const URL_A = `ws://${HOST_A}`;
const URL_B = `ws://${HOST_B}`;
const EVIDENCE_PATH =
	process.env.CHAT_DIRECT_NEG_EVIDENCE ??
	'../docs/pilot/evidence/chat-direct-negative-acceptance-20261109.json';
const RUN_ID = bytesToHex(crypto.getRandomValues(new Uint8Array(6)));
const OP_TIMEOUT_MS = 20_000;

// ---------------------------------------------------------------------------
// keys + event signing (NIP-01 canonical serialization, BIP-340 schnorr)
// ---------------------------------------------------------------------------
function generateKey() {
	const secretKey = bytesToHex(crypto.getRandomValues(new Uint8Array(32)));
	return { secretKey, pubkey: bytesToHex(schnorr.getPublicKey(hexToBytes(secretKey))) };
}

async function sha256Hex(input) {
	const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

async function signEvent(unsigned, secretKey, pubkey) {
	const created_at = Math.floor(Date.now() / 1000);
	const id = await sha256Hex(
		JSON.stringify([0, pubkey, created_at, unsigned.kind, unsigned.tags, unsigned.content])
	);
	const sig = bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(secretKey)));
	return {
		id,
		sig,
		kind: unsigned.kind,
		tags: unsigned.tags,
		content: unsigned.content,
		pubkey,
		created_at
	};
}

// ---------------------------------------------------------------------------
// NIP-42 authenticated session. The relay challenges LAZILY: a probe REQ goes
// out on open, the AUTH challenge arrives in response, and once the auth OK
// lands the socket is fully usable for publish()/query(). With a mismatched
// relay tag the auth OK is false and the session reports auth.accepted=false
// (the cross-host negative check relies on exactly that).
// ---------------------------------------------------------------------------
async function openSession(connectUrl, secretKey, { relayTag = connectUrl } = {}) {
	const pubkey = bytesToHex(schnorr.getPublicKey(hexToBytes(secretKey)));
	const ws = new WebSocket(connectUrl);
	const pending = new Map();
	let authResolve = null;
	let authId = null;
	let authSettled = false;
	let authResult = null;
	let subCounter = 0;
	const sessions = new Map();
	// Frame-type allowlist: every wire value is classified against these
	// fixed constants before it can influence any local state. Trust never
	// derives from the frame type alone — the handshake settles only when a
	// frame echoes OUR locally-computed auth event id (see authSettled).
	const FRAME = Object.freeze({
		AUTH: 'AUTH',
		OK: 'OK',
		EVENT: 'EVENT',
		EOSE: 'EOSE',
		CLOSED: 'CLOSED'
	});
	const frameType = (value) =>
		typeof value === 'string' && Object.values(FRAME).includes(value) ? value : null;
	ws.onmessage = (raw) => {
		void (async () => {
			let msg;
			try {
				msg = JSON.parse(String(raw.data));
			} catch {
				return;
			}
			if (!Array.isArray(msg)) return;
			const type = frameType(msg[0]);
			if (type === null) return;
			if (type === FRAME.AUTH && typeof msg[1] === 'string' && !authId) {
				const auth = await signEvent(
					{
						kind: 22242,
						tags: [
							['relay', relayTag],
							['challenge', msg[1]]
						],
						content: ''
					},
					secretKey,
					pubkey
				);
				authId = auth.id;
				ws.send(JSON.stringify(['AUTH', auth]));
				return;
			}
			// Locally-anchored guard: only a frame echoing the auth event id
			// WE signed may settle the handshake; authSettled is local state.
			if (type === FRAME.OK && !authSettled && authId !== null && msg[1] === authId) {
				authSettled = true;
				authResult = {
					settled: true,
					accepted: msg[2] === true,
					message: typeof msg[3] === 'string' ? msg[3] : ''
				};
				if (authResolve) authResolve(authResult);
				return;
			}
			if (msg[0] === 'EVENT') {
				const sub = sessions.get(msg[1]);
				if (sub) sub.events.push(msg[2]);
				return;
			}
			if (msg[0] === 'EOSE' || msg[0] === 'CLOSED') {
				const sub = sessions.get(msg[1]);
				if (sub) {
					sessions.delete(msg[1]);
					sub.resolve({
						ok: msg[0] === 'EOSE',
						detail: msg[0] === 'CLOSED' ? `closed: ${msg[2]}` : '',
						events: sub.events
					});
				}
				return;
			}
			if (msg[0] === 'OK') {
				const op = pending.get(msg[1]);
				if (op) {
					pending.delete(msg[1]);
					op.resolve({
						accepted: msg[2] === true,
						message: typeof msg[3] === 'string' ? msg[3] : ''
					});
				}
			}
		})();
	};
	// One onerror handler fans out to both consumers: pre-open failures
	// reject the connect promise, post-open failures reject the session
	// error promise. (Assigning ws.onerror twice would leave the first
	// promise's rejection path dead.)
	let opened = false;
	let reportError = null;
	const wsError = new Promise((_, reject) => {
		reportError = (what) => reject(new Error(`${what} on ${connectUrl}`));
	});
	ws.onerror = () => reportError?.(opened ? 'ws error' : 'ws connect failed');
	const open = new Promise((resolve) => {
		ws.onopen = () => {
			opened = true;
			resolve(undefined);
		};
	});
	await Promise.race([open, wsError]);
	const authPromise = new Promise((resolve, reject) => {
		authResolve = resolve;
		setTimeout(() => {
			if (!authResult) reject(new Error('AUTH challenge/response timeout'));
		}, OP_TIMEOUT_MS);
	});
	// Probe REQ triggers the lazy AUTH challenge; its (post-auth) response is
	// intentionally ignored — only the challenge matters here.
	ws.send(
		JSON.stringify(['REQ', `probe${(++subCounter).toString(36)}`, { kinds: [39000], limit: 1 }])
	);
	const auth = await Promise.race([authPromise, wsError]);
	// Usability gate on LOCAL state: the handshake must have settled (a
	// frame matched our own auth event id). The wire boolean only chooses
	// between a usable session and an explicitly rejected one.
	if (!auth || auth.settled !== true) {
		throw new Error('relay handshake did not settle against our auth event id');
	}
	if (!auth.accepted) {
		ws.close();
		return { auth, publish: unavailable, query: unavailable, close: () => ws.close() };
	}
	const withTimeout = (promise, what) =>
		Promise.race([
			promise,
			new Promise((_, reject) =>
				setTimeout(() => reject(new Error(`${what} timeout`)), OP_TIMEOUT_MS)
			)
		]);
	return {
		auth,
		publish: async (unsigned) => {
			const event = await signEvent(unsigned, secretKey, pubkey);
			const okPromise = new Promise((resolve, reject) =>
				pending.set(event.id, { resolve, reject })
			);
			ws.send(JSON.stringify(['EVENT', event]));
			const result = await withTimeout(Promise.race([okPromise, wsError]), 'publish');
			return { ...result, eventId: event.id };
		},
		query: async (filter) => {
			const sub = `dn${(++subCounter).toString(36)}${bytesToHex(crypto.getRandomValues(new Uint8Array(2)))}`;
			const resultPromise = new Promise((resolve, reject) =>
				sessions.set(sub, { resolve, reject, events: [] })
			);
			ws.send(JSON.stringify(['REQ', sub, filter]));
			return withTimeout(Promise.race([resultPromise, wsError]), 'query');
		},
		close: () => ws.close()
	};
}

function unavailable() {
	throw new Error('session not authenticated');
}

// ---------------------------------------------------------------------------
// check ledger
// ---------------------------------------------------------------------------
const checks = [];
function record(id, title, status, evidence) {
	checks.push({ id, title, status, evidence });
	console.log(`[direct-neg ${id}] ${status} — ${JSON.stringify(evidence)}`);
}

let failed = false;
function check(id, title, condition, evidence) {
	const status = condition ? 'PASS' : 'FAIL';
	if (!condition) failed = true;
	record(id, title, status, evidence);
	return condition;
}

async function poll(desc, fn, { timeoutMs = 60_000, intervalMs = 2_000 } = {}) {
	const started = Date.now();
	let last;
	for (;;) {
		try {
			last = await fn();
			if (last.ok) return last;
		} catch (error) {
			last = { ok: false, detail: String(error) };
		}
		if (Date.now() - started > timeoutMs) return { ...last, ok: false, timedOut: true, desc };
		await new Promise((resolve) => setTimeout(resolve, intervalMs));
	}
}

// ---------------------------------------------------------------------------
// matrix
// ---------------------------------------------------------------------------
async function main() {
	const alice = generateKey();
	const member = generateKey();
	const guest = generateKey();
	const bob = generateKey();
	const channelA = crypto.randomUUID();
	const channelB = crypto.randomUUID();
	const channelNameA = `tenant-a-${RUN_ID}`;
	const channelNameB = `tenant-b-${RUN_ID}`;
	const aliceMessage = `tenant A message ${RUN_ID}`;

	// --- Community A ------------------------------------------------------------
	const aliceSession = await openSession(URL_A, alice.secretKey);
	check('a_auth', 'Alice NIP-42 AUTH on community A accepted', aliceSession.auth.accepted, {
		relayTag: URL_A,
		message: aliceSession.auth.message
	});

	const created = await aliceSession.publish({
		kind: 9007,
		tags: [
			['h', channelA],
			['name', channelNameA],
			['visibility', 'private'],
			['channel_type', 'stream']
		],
		content: ''
	});
	check('a_create_channel', 'Alice creates a private channel on community A', created.accepted, {
		channelId: channelA,
		eventId: created.eventId,
		message: created.message
	});

	const snapshot = await poll('39000 snapshot', () =>
		aliceSession.query({ kinds: [39000], '#d': [channelA] })
	);
	const snapshotEvent = (snapshot.events ?? []).find((e) => e.id);
	check(
		'a_snapshot',
		'Relay-signed 39000 metadata snapshot for the channel exists on A',
		snapshot.ok && !!snapshotEvent,
		{ ok: snapshot.ok, detail: snapshot.detail ?? '', snapshotEventId: snapshotEvent?.id ?? null }
	);

	const posted = await aliceSession.publish({
		kind: 9,
		tags: [['h', channelA]],
		content: aliceMessage
	});
	check('a_post', 'Alice posts a kind-9 message on community A', posted.accepted, {
		eventId: posted.eventId,
		message: posted.message
	});

	const addedMember = await aliceSession.publish({
		kind: 9000,
		tags: [
			['h', channelA],
			['p', member.pubkey]
		],
		content: ''
	});
	const memberSession = await openSession(URL_A, member.secretKey);
	const memberPost = await memberSession.publish({
		kind: 9,
		tags: [['h', channelA]],
		content: `member write ${RUN_ID}`
	});
	check(
		'a_member_write',
		'Member added and member kind-9 accepted (positive control)',
		addedMember.accepted && memberPost.accepted,
		{
			addMemberEventId: addedMember.eventId,
			memberPostEventId: memberPost.eventId,
			memberPostMessage: memberPost.message
		}
	);

	// Guest provisioning relay-direct (the UI deliberately cannot do this).
	const guestAssignedAt = Date.now();
	const addedGuest = await aliceSession.publish({
		kind: 9000,
		tags: [
			['h', channelA],
			['p', guest.pubkey],
			['role', 'guest']
		],
		content: ''
	});
	const roster = await poll('39002 roster', () =>
		aliceSession.query({ kinds: [39002], '#d': [channelA] })
	);
	const rosterEvents = roster.events ?? [];
	const guestEntry = rosterEvents
		.flatMap((e) => e.tags ?? [])
		.filter((t) => t[0] === 'p' && t[1] === guest.pubkey)
		.map((t) => t[3] ?? 'member')[0];
	const rosterEvent = rosterEvents.find((e) =>
		(e.tags ?? []).some((t) => t[0] === 'p' && t[1] === guest.pubkey)
	);
	check(
		'a_guest_role',
		'Owner-signed 9000 with role tag provisions the guest role (relay-direct)',
		addedGuest.accepted && guestEntry === 'guest',
		{
			addGuestEventId: addedGuest.eventId,
			rosterShowsGuestRole: guestEntry === 'guest',
			rosterSnapshotsScanned: rosterEvents.length,
			rosterEventId: rosterEvent?.id ?? null
		}
	);

	const guestSession = await openSession(URL_A, guest.secretKey);
	const guestRead = await guestSession.query({ kinds: [9], '#h': [channelA] });
	check(
		'a_guest_read',
		'Guest read still works: guest sees the owner message',
		guestRead.ok && guestRead.events.some((e) => e.id === posted.eventId),
		{ ok: guestRead.ok, detail: guestRead.detail ?? '', events: guestRead.events.length }
	);

	const guestWrite = await guestSession.publish({
		kind: 9,
		tags: [['h', channelA]],
		content: `guest write ${RUN_ID}`
	});
	const denialAnchored = /guest/.test(guestWrite.message);
	check(
		'a_guest_write_denied',
		'CORE: guest-role signed kind-9 is REJECTED by the candidate relay',
		!guestWrite.accepted && denialAnchored,
		{
			eventId: guestWrite.eventId,
			accepted: guestWrite.accepted,
			denialAnchoredToGuestRole: denialAnchored,
			denialMessage: guestWrite.message,
			msAfterRoleAssignment: Date.now() - guestAssignedAt
		}
	);

	// --- Community B --------------------------------------------------------------
	const bobSession = await openSession(URL_B, bob.secretKey);
	check(
		'b_auth',
		'Bob NIP-42 AUTH on community B accepted (tenant B operable)',
		bobSession.auth.accepted,
		{ relayTag: URL_B, message: bobSession.auth.message }
	);

	const bobDiscovery = await bobSession.query({ kinds: [39000] });
	const bobSeesChannelA = (bobDiscovery.events ?? []).some((e) =>
		(e.tags ?? []).some((t) => t[0] === 'd' && t[1] === channelA)
	);
	check(
		'b_discovery_isolated',
		'Tenant B discovery does not list tenant A channels',
		bobDiscovery.ok && !bobSeesChannelA,
		{
			ok: bobDiscovery.ok,
			detail: bobDiscovery.detail ?? '',
			channelACount: bobSeesChannelA ? 1 : 0
		}
	);

	const bobRead = await bobSession.query({ kinds: [9], '#h': [channelA] });
	const bobLeaked = (bobRead.events ?? []).filter((e) => e.id === posted.eventId).length;
	check(
		'b_read_isolated',
		'Tenant B cannot read tenant A channel content (even by guessing the id)',
		bobLeaked === 0,
		{
			ok: bobRead.ok,
			detail: bobRead.detail ?? '',
			events: bobRead.events.length,
			leakedEventIds: bobLeaked
		}
	);

	const bobWrite = await bobSession.publish({
		kind: 9,
		tags: [['h', channelA]],
		content: `cross-tenant write ${RUN_ID}`
	});
	check(
		'b_write_denied',
		'Tenant B signed kind-9 to a tenant A channel is REJECTED',
		!bobWrite.accepted,
		{ eventId: bobWrite.eventId, accepted: bobWrite.accepted, denialMessage: bobWrite.message }
	);

	// Cross-host NIP-42: relay tag names tenant A's host on a tenant B connection.
	const crossHost = await openSession(URL_B, bob.secretKey, { relayTag: URL_A });
	check(
		'b_cross_host_auth_denied',
		'CORE: cross-host NIP-42 AUTH (relay tag A on B connection) is REJECTED',
		!crossHost.auth.accepted,
		{ accepted: crossHost.auth.accepted, denialMessage: crossHost.auth.message }
	);

	const bobCreated = await bobSession.publish({
		kind: 9007,
		tags: [
			['h', channelB],
			['name', channelNameB],
			['visibility', 'private'],
			['channel_type', 'stream']
		],
		content: ''
	});
	const bobPost = await bobSession.publish({
		kind: 9,
		tags: [['h', channelB]],
		content: `tenant B message ${RUN_ID}`
	});
	check(
		'b_positive',
		'Tenant B channel creation and posting work (positive control)',
		bobCreated.accepted && bobPost.accepted,
		{ channelId: channelB, createEventId: bobCreated.eventId, postEventId: bobPost.eventId }
	);

	// Reverse direction: tenant A cannot see tenant B.
	const aliceDiscovery = await aliceSession.query({ kinds: [39000] });
	const aliceSeesChannelB = (aliceDiscovery.events ?? []).some((e) =>
		(e.tags ?? []).some((t) => t[0] === 'd' && t[1] === channelB)
	);
	const aliceRead = await aliceSession.query({ kinds: [9], '#h': [channelB] });
	const aliceLeaked = (aliceRead.events ?? []).filter((e) => e.id === bobPost.eventId).length;
	check(
		'a_reverse_isolation',
		'Reverse isolation: tenant A cannot discover or read tenant B content',
		!aliceSeesChannelB && aliceLeaked === 0,
		{
			aliceSeesChannelB,
			aliceReadOk: aliceRead.ok,
			aliceReadDetail: aliceRead.detail ?? '',
			aliceReadEvents: aliceRead.events.length
		}
	);

	for (const session of [aliceSession, memberSession, guestSession, bobSession]) {
		try {
			session.close();
		} catch {
			/* already closed */
		}
	}

	return {
		identities: {
			alice: alice.pubkey,
			member: member.pubkey,
			guest: guest.pubkey,
			bob: bob.pubkey
		},
		channels: { a: channelA, b: channelB }
	};
}

const evidence = {
	generatedAt: new Date().toISOString(),
	runId: RUN_ID,
	relay: {
		image: process.env.CHAT_DIRECT_NEG_IMAGE ?? 'buzz-elembra-candidate:f68a4c59',
		imageId: process.env.CHAT_DIRECT_NEG_IMAGE_ID ?? null,
		builtFrom:
			'f68a4c59 (final qualified candidate; supported baseline c9a373b + guest-write-denial hotfix)',
		wsCommunityA: URL_A,
		wsCommunityB: URL_B
	},
	topology: {
		layout:
			'one relay process, two communities keyed by Host header (buzz nip42_host_binding_live pattern)',
		communities: [
			{
				id:
					process.env.CHAT_DIRECT_NEG_COMMUNITY_A_ID?.trim() ||
					'auto-ensured by relay at startup (RELAY_URL host)',
				host: HOST_A,
				note: 'relay auto-provisions its RELAY_URL host community at startup'
			},
			{ id: '22222222-2222-4222-8222-222222222222', host: HOST_B }
		],
		requireRelayMembership: false,
		rationale:
			'Membership gate disabled so any NIP-42-authed key can publish; the checks isolate channel-role (guest write denial) and community boundaries (two-tenant isolation). The Elembra acceptance stack covers the community-membership gate separately.'
	},
	provenance: {
		gitSha: process.env.ACCEPTANCE_GIT_SHA ?? null,
		stack: process.env.ACCEPTANCE_STACK ?? null
	},
	checks: []
};

let matrix = null;
try {
	matrix = await main();
	evidence.identities = matrix.identities;
	evidence.channels = matrix.channels;
} catch (error) {
	failed = true;
	record('harness', 'direct-negative matrix aborted', 'FAIL', { error: String(error) });
} finally {
	evidence.checks = checks;
	evidence.summary = {
		total: checks.length,
		passed: checks.filter((c) => c.status === 'PASS').length,
		failed: checks.filter((c) => c.status === 'FAIL').length
	};
	mkdirSync(dirname(EVIDENCE_PATH), { recursive: true });
	writeFileSync(EVIDENCE_PATH, `${JSON.stringify(evidence, null, 2)}\n`);
	console.log(`evidence written to ${EVIDENCE_PATH}`);
}

process.exit(failed ? 1 : 0);
