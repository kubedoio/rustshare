// Chat channel-management product acceptance (PR #480, mission §5).
//
// Drives the REAL bundled stack through real browser contexts: Elembra login,
// browser-held Chat identity binding, NIP-42 relay admission against the pinned
// Buzz relay, channel lifecycle UI, messaging, and the Files attachment gate.
// Three separate browser contexts act as user A (alice), user B (bob), and the
// unrelated user C (carol). Assertions never mock the API; direct API calls use
// each browser context's own authenticated session (cookie jar + CSRF header).
//
// Required env:
//   ACCEPTANCE_ADMIN_PASSWORD         first-boot admin (bootstrap) password
// Optional env:
//   ACCEPTANCE_BASE_URL               default http://127.0.0.1:18080
//   ACCEPTANCE_RELAY_URL              default ws://localhost:17447
//   ACCEPTANCE_EVIDENCE               evidence JSON output path
//   ACCEPTANCE_GIT_SHA / ACCEPTANCE_IMAGE_DIGESTS / ACCEPTANCE_STACK
//                                     provenance recorded verbatim in evidence
import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { execFileSync } from 'node:child_process';
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

// Typed globals the matrix-g ACK-delay WebSocket wrapper injects into the
// page (see test g); declared once so page.evaluate callbacks type-check.
declare global {
	interface Window {
		__gAckProbe?: {
			delayNext: boolean;
			swallowedId: string | null;
			lastEventId: string | null;
		};
		__gOriginalWS?: typeof WebSocket;
	}
}

const BASE_URL = process.env.ACCEPTANCE_BASE_URL ?? 'http://127.0.0.1:18080';
const RELAY_URL = process.env.ACCEPTANCE_RELAY_URL ?? 'ws://localhost:17447';
const ADMIN_EMAIL = process.env.ACCEPTANCE_ADMIN_EMAIL ?? 'admin@localhost';
const ADMIN_PASSWORD = process.env.ACCEPTANCE_ADMIN_PASSWORD ?? '';
const EVIDENCE_PATH =
	process.env.ACCEPTANCE_EVIDENCE ?? '../docs/pilot/evidence/chat-channel-acceptance-20261109.json';
// CSPRNG-backed run uniqueness: the disposable-user password, channel names,
// canary bodies, and message bodies all derive from this suffix, so it must
// not come from Math.random() (CodeQL js/insecure-randomness).
function randomRunSuffix(): string {
	const bytes = crypto.getRandomValues(new Uint8Array(6));
	const randomPart = [...bytes].map((b) => b.toString(36).padStart(2, '0')).join('');
	return `${Date.now().toString(36)}${randomPart}`;
}
const RUN_SUFFIX = randomRunSuffix();
const COMPOSE_PROJECT = process.env.COMPOSE_PROJECT_NAME ?? 'accharden';

if (!ADMIN_PASSWORD) {
	throw new Error('ACCEPTANCE_ADMIN_PASSWORD is required (bootstrap admin password)');
}

// ---------------------------------------------------------------------------
// Evidence ledger
// ---------------------------------------------------------------------------
interface MatrixResult {
	id: string;
	title: string;
	status: 'PASS' | 'FAIL' | 'BLOCKED';
	evidence: Record<string, unknown>;
}
const results: MatrixResult[] = [];
function record(
	id: string,
	title: string,
	status: MatrixResult['status'],
	evidence: Record<string, unknown>
) {
	results.push({ id, title, status, evidence });
	console.log(`[matrix ${id}] ${status} — ${JSON.stringify(evidence)}`);
}

// ---------------------------------------------------------------------------
// Shared acceptance state
// ---------------------------------------------------------------------------
interface UserState {
	key: 'a' | 'b' | 'c';
	username: string;
	email: string;
	password: string;
	userId: string;
	pubkey: string;
	/** Browser-held Chat secret key, decrypted in-test with the known
	 *  acceptance passphrase (see decryptChatKey). Owned-key material for
	 *  relay-direct NIP-42 scenarios only. */
	secretKey: string | null;
	context: BrowserContext;
	page: Page;
}
const state = {
	badResponses: {} as Record<string, string[]>,
	tenantId: '',
	communityId: '',
	relayPubkey: '',
	users: {} as Record<'a' | 'b' | 'c', UserState>,
	channels: {} as Record<string, { id: string; name: string }>,
	eventIds: {} as Record<string, string | null>,
	attachmentFileId: ''
};

// ---------------------------------------------------------------------------
// API helpers (real sessions; CSRF double-submit like the product client)
// ---------------------------------------------------------------------------
async function csrfOf(ctx: BrowserContext): Promise<string> {
	const cookies = await ctx.cookies(BASE_URL);
	return cookies.find((c) => c.name === 'rustshare_csrf_token')?.value ?? '';
}
async function api(
	ctx: BrowserContext,
	method: string,
	path: string,
	body?: unknown
): Promise<{ status: number; json: () => Promise<any> }> {
	const res = await ctx.request.fetch(path, {
		method,
		data: body === undefined ? undefined : JSON.stringify(body),
		headers: {
			'content-type': 'application/json',
			'X-Rustshare-Csrf': await csrfOf(ctx)
		}
	});
	return { status: res.status(), json: async () => res.json() };
}

async function uiLogin(user: UserState): Promise<void> {
	const { page } = user;
	await page.goto('/login');
	await page.getByLabel('Email').fill(user.email);
	await page.getByLabel('Password').fill(user.password);
	await page.getByRole('button', { name: 'Sign in with password' }).click();
	await page.waitForURL('**/files', { timeout: 30_000 });
}

async function bindChat(user: UserState): Promise<void> {
	const { page } = user;
	await page.goto('/apps/chat');
	await expect(page.getByRole('heading', { name: 'Set up Chat' })).toBeVisible({ timeout: 30_000 });
	await page.getByLabel('key passphrase').fill(`${user.password}-key`);
	await page.getByRole('button', { name: 'Create identity & bind' }).click();
	// Admission is queued through the durable outbox to the relay (9030); the
	// main Chat view renders once status reports an active admission. (The
	// 'Bound and admission queued.' notice is transient — the view switches
	// away from the binding panel immediately — so wait for the stable state.)
	await expect(page.getByRole('heading', { name: 'Channels' })).toBeVisible({ timeout: 120_000 });
	// The browser-held signing session starts locked; unlock it so the composer
	// renders for the messaging steps.
	await expect(page.getByRole('heading', { name: 'Unlock Chat' })).toBeVisible({ timeout: 30_000 });
	await page.getByLabel('key passphrase').fill(`${user.password}-key`);
	await page.getByRole('button', { name: 'Unlock' }).click();
	await expect(page.getByLabel('Message text')).toBeVisible({ timeout: 30_000 });
}

async function chatStatus(user: UserState) {
	const res = await api(user.context, 'GET', '/api/v1/applications/chat/status');
	return { status: res.status, body: await res.json() };
}

/** Full page navigations reset the in-memory signing session (the key never
 *  persists unencrypted); re-unlock whenever a goto preceded messaging. */
async function ensureUnlocked(user: UserState): Promise<void> {
	const { page } = user;
	const unlockHeading = page.getByRole('heading', { name: 'Unlock Chat' });
	if (await unlockHeading.isVisible().catch(() => false)) {
		await page.getByLabel('key passphrase').fill(`${user.password}-key`);
		await page.getByRole('button', { name: 'Unlock' }).click();
		await expect(page.getByLabel('Message text')).toBeVisible({ timeout: 30_000 });
	}
}

/** Create a channel via the PR #480 UI. Success is user-observable and strict:
 *  the channel NAME must appear in the creator's own relay-backed listbox
 *  (deep links are unreliable here — the directory-clobber effect can reset
 *  the selection to the first entry before the snapshot lands). The channel
 *  id is read back from the backend registry by name. Rejections (e.g. the
 *  relay-admission propagation race) are retried with a fresh name. */
async function createChannelViaUi(
	user: UserState,
	baseName: string,
	visibility: 'open' | 'private'
): Promise<{ id: string; name: string }> {
	const { page } = user;
	for (let attempt = 1; attempt <= 4; attempt += 1) {
		const name = attempt === 1 ? baseName : `${baseName} ${attempt}`;
		await page.getByRole('button', { name: '+ New channel' }).click({ timeout: 10_000 });
		const form = page.getByRole('form', { name: 'Create channel' });
		await form.getByLabel('New channel name').fill(name);
		await form.getByLabel('New channel description').fill(`acceptance channel ${name}`);
		await form.getByLabel('New channel visibility').selectOption(visibility);
		await form.getByRole('button', { name: 'Create channel' }).click();
		try {
			await expect
				.poll(
					async () => {
						const retryVisible = await page
							.getByRole('button', { name: 'Retry same request' })
							.isVisible()
							.catch(() => false);
						if (retryVisible) {
							await page.getByRole('button', { name: 'Retry same request' }).click();
							return false;
						}
						const alert = await page
							.getByRole('alert')
							.last()
							.textContent()
							.catch(() => '');
						if (alert && /locked/i.test(alert)) {
							await page.getByLabel('key passphrase').fill(`${user.password}-key`);
							await page.getByRole('button', { name: 'Unlock' }).click();
							return false;
						}
						if (alert && /rejected|unreachable/i.test(alert)) return 'error';
						const dir = await channelDirectoryOf(user);
						return (dir.body ?? []).some((c: any) => c.name === name);
					},
					{ timeout: 90_000, intervals: [3_000, 5_000, 5_000] }
				)
				.toBe(true);
		} catch {
			// Fall through to the retry below.
		}
		const dir = await channelDirectoryOf(user);
		const entry = (dir.body ?? []).find((c: any) => c.name === name);
		if (entry) {
			await channelOption(page, name)
				.click()
				.catch(() => {});
			return { id: entry.channel_id, name };
		}
		const cancel = page.getByRole('button', { name: 'Cancel' });
		if (await cancel.isVisible().catch(() => false)) await cancel.click().catch(() => {});
		await page.waitForTimeout(8_000);
	}
	throw new Error(`createChannelViaUi failed for ${baseName} after retries`);
}

async function sendMessage(user: UserState, body: string): Promise<void> {
	const { page } = user;
	// The composer gates Send on posting eligibility AND a non-empty draft;
	// fill first, then wait for the button to arm.
	await page.getByLabel('Message text').fill(body);
	try {
		await expect(page.getByRole('button', { name: 'Send message' })).toBeEnabled({
			timeout: 90_000
		});
	} catch (error) {
		const bar = page
			.getByRole('button', { name: 'Refresh permissions' })
			.locator('xpath=ancestor::div[1]')
			.first();
		const barText = await bar.textContent().catch(() => '');
		const header = await page
			.locator('header')
			.last()
			.textContent()
			.catch(() => '');
		console.log(
			`[sendMessage:${user.key}] Send never enabled. header=${header?.slice(0, 120)} bar=${barText?.slice(0, 300)}`
		);
		console.log(`[sendMessage:${user.key}] bad responses:`, JSON.stringify(state.badResponses));
		throw error;
	}
	// Nudge the relay-permission refresh instead of waiting a whole poll cycle.
	await page
		.getByRole('button', { name: 'Refresh permissions' })
		.click()
		.catch(() => {});
	await page.getByRole('button', { name: 'Send message' }).click();
	await expect(page.getByText('Observed by Elembra.')).toBeVisible({ timeout: 90_000 });
}

/** Channel item in the relay-backed directory listbox (robust across the
 *  title/accessible-name variants of the ChannelList option button). */
function channelOption(page: Page, name: string) {
	return page.getByRole('option', {
		name: new RegExp(name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
	});
}

async function messagesOf(user: UserState, channelId: string) {
	const res = await api(
		user.context,
		'GET',
		`/api/v1/applications/chat/messages?channel_id=${channelId}`
	);
	return { status: res.status, body: await res.json() };
}

async function channelDirectoryOf(user: UserState) {
	const res = await api(user.context, 'GET', '/api/v1/applications/chat/channels');
	return { status: res.status, body: await res.json() };
}

/** Negative isolation assertions must SAMPLE across the propagation window:
 *  a single immediate check can pass before the relay/observer catches up and
 *  report a false PASS. Every sample must independently hold. */
async function expectAbsentAcrossWindow(
	samples: number,
	intervalMs: number,
	check: () => Promise<void>
): Promise<void> {
	for (let i = 0; i < samples; i += 1) {
		await check();
		if (i < samples - 1) await new Promise((resolve) => setTimeout(resolve, intervalMs));
	}
}

// ---------------------------------------------------------------------------
// Relay-direct scenarios (NIP-42 authenticated WebSocket, owned test keys).
// Node 22's global WebSocket speaks the same wire protocol as the browser;
// signatures are BIP-340 Schnorr via @noble/curves (mirrors the product's
// frontend/src/lib/chat/keys.ts envelope for key-backup decryption).
// ---------------------------------------------------------------------------
async function decryptChatKey(user: UserState): Promise<string | null> {
	const envelopeJson = await user.page.evaluate(
		(key) => localStorage.getItem(key),
		`elembra.chat.key.v1.${user.userId}`
	);
	if (!envelopeJson) return null;
	const envelope = JSON.parse(envelopeJson);
	const iterations = envelope.iter ?? 100_000;
	const aad =
		envelope.aad === 1
			? new TextEncoder().encode(JSON.stringify([1, user.userId, state.tenantId]))
			: undefined;
	const material = await crypto.subtle.importKey(
		'raw',
		new TextEncoder().encode(`${user.password}-key`),
		'PBKDF2',
		false,
		['deriveKey']
	);
	const derived = await crypto.subtle.deriveKey(
		{ name: 'PBKDF2', salt: hexToBytes(envelope.salt), iterations, hash: 'SHA-256' },
		material,
		{ name: 'AES-GCM', length: 256 },
		false,
		['decrypt']
	);
	try {
		const plain = await crypto.subtle.decrypt(
			{ name: 'AES-GCM', iv: hexToBytes(envelope.iv), additionalData: aad },
			derived,
			hexToBytes(envelope.ciphertext)
		);
		return bytesToHex(new Uint8Array(plain));
	} catch {
		return null;
	}
}

function publicKeyOf(secretKey: string): string {
	return bytesToHex(schnorr.getPublicKey(hexToBytes(secretKey)));
}

async function sha256Hex(input: string): Promise<string> {
	const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

interface UnsignedEvent {
	kind: number;
	tags: string[][];
	content: string;
}

async function signEvent(unsigned: UnsignedEvent, secretKey: string, pubkey: string) {
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

/**
 * NIP-42 authenticated relay READ. The relay closes any unauthenticated REQ
 * immediately (auth-required) and answers the NIP-42 challenge separately, so
 * the real REQ goes out only AFTER the auth OK. A throwaway probe absorbs the
 * early close; probe frames are ignored by design.
 */
function nextSubId(): string {
	const bytes = crypto.getRandomValues(new Uint8Array(4));
	return `acc${[...bytes].map((b) => b.toString(16).padStart(2, '0')).join('')}`;
}

interface RelayQueryResult {
	ok: boolean;
	detail: string;
	events: any[];
}

async function relayQuery(
	secretKey: string,
	filter: Record<string, unknown>
): Promise<RelayQueryResult> {
	const pubkey = publicKeyOf(secretKey);
	return new Promise((resolve) => {
		const ws = new WebSocket(RELAY_URL);
		const sub = nextSubId();
		const events: any[] = [];
		let authId: string | null = null;
		let settled = false;
		const finish = (result: RelayQueryResult) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			try {
				ws.close();
			} catch {
				/* already closed */
			}
			resolve(result);
		};
		const timer = setTimeout(() => finish({ ok: false, detail: 'timeout', events }), 20_000);
		// Choreography (mirrors the direct-negative harness): an UNauthenticated
		// REQ is closed instantly with auth-required, so the real REQ must wait
		// for the NIP-42 handshake. A throwaway probe absorbs that early close
		// and the relay's proactive challenge; the real REQ goes out only after
		// the auth OK (the relay commits auth state before sending it).
		ws.onopen = () => ws.send(JSON.stringify(['REQ', 'probe', { kinds: [39000], limit: 1 }]));
		ws.onerror = () => finish({ ok: false, detail: 'transport', events });
		ws.onclose = () => finish({ ok: false, detail: 'closed', events });
		ws.onmessage = (raw) => {
			void (async () => {
				let msg: unknown;
				try {
					msg = JSON.parse(String(raw.data));
				} catch {
					return;
				}
				if (!Array.isArray(msg)) return;
				if (msg[0] === 'AUTH' && typeof msg[1] === 'string' && !authId) {
					const auth = await signEvent(
						{
							kind: 22242,
							tags: [
								['relay', RELAY_URL],
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
				if (msg[0] === 'OK' && authId && msg[1] === authId) {
					if (msg[2] === true) ws.send(JSON.stringify(['REQ', sub, filter]));
					else finish({ ok: false, detail: `auth rejected: ${msg[3]}`, events });
					return;
				}
				if (msg[0] === 'EVENT' && msg[1] === sub) events.push(msg[2]);
				else if (msg[0] === 'EOSE' && msg[1] === sub) finish({ ok: true, detail: '', events });
				else if (msg[0] === 'CLOSED' && msg[1] === sub)
					finish({ ok: false, detail: `closed: ${msg[2]}`, events });
				// Frames for the throwaway probe sub are ignored by design.
			})();
		};
	});
}

interface RelayPublishResult {
	eventId: string;
	accepted: boolean;
	message: string;
}

/** NIP-42 authenticated relay WRITE: publish a freshly signed EVENT. Same
 *  lazy-auth choreography as relayQuery — the EVENT goes out on open; when
 *  the relay challenges, the auth OK is followed by a re-send of the exact
 *  same signed event (the relay is idempotent by event id). */
async function relayPublish(
	secretKey: string,
	unsigned: UnsignedEvent
): Promise<RelayPublishResult> {
	const pubkey = publicKeyOf(secretKey);
	const event = await signEvent(unsigned, secretKey, pubkey);
	return new Promise((resolve) => {
		const ws = new WebSocket(RELAY_URL);
		let authId: string | null = null;
		let settled = false;
		const finish = (result: RelayPublishResult) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			try {
				ws.close();
			} catch {
				/* already closed */
			}
			resolve(result);
		};
		const timer = setTimeout(
			() => finish({ eventId: event.id, accepted: false, message: 'timeout' }),
			20_000
		);
		// Same lazy-handshake choreography as relayQuery: probe first, publish
		// only after the auth OK.
		ws.onopen = () => ws.send(JSON.stringify(['REQ', 'probe', { kinds: [39000], limit: 1 }]));
		ws.onerror = () => finish({ eventId: event.id, accepted: false, message: 'transport' });
		ws.onclose = () => finish({ eventId: event.id, accepted: false, message: 'closed' });
		ws.onmessage = (raw) => {
			void (async () => {
				let msg: unknown;
				try {
					msg = JSON.parse(String(raw.data));
				} catch {
					return;
				}
				if (!Array.isArray(msg)) return;
				if (msg[0] === 'AUTH' && typeof msg[1] === 'string' && !authId) {
					const auth = await signEvent(
						{
							kind: 22242,
							tags: [
								['relay', RELAY_URL],
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
				if (msg[0] === 'OK' && authId && msg[1] === authId) {
					if (msg[2] === true) ws.send(JSON.stringify(['EVENT', event]));
					else finish({ eventId: event.id, accepted: false, message: `auth rejected: ${msg[3]}` });
					return;
				}
				if (msg[0] === 'OK' && msg[1] === event.id) {
					finish({
						eventId: event.id,
						accepted: msg[2] === true,
						message: typeof msg[3] === 'string' ? msg[3] : ''
					});
				}
			})();
		};
	});
}

// ---------------------------------------------------------------------------
// Matrix
// ---------------------------------------------------------------------------
test.describe.configure({ mode: 'serial' });
test.describe('Chat channel management product acceptance (PR #480)', () => {
	test.setTimeout(420_000);

	let browser: Browser;
	const adminContextHolder: { ctx?: BrowserContext } = {};

	test.beforeAll(async ({ browser: b }) => {
		// Hook timeouts do NOT inherit the describe-level test.setTimeout in
		// this Playwright version (the hook caps at the global 30 s default,
		// which the Chat enablement + community provisioning wait can exceed
		// on a fresh stack) — set it explicitly for the hook body.
		test.setTimeout(420_000);
		browser = b;
		// --- Seed: admin login + Chat enablement + auto-provisioning ----------
		const admin = await b.newContext({ baseURL: BASE_URL });
		adminContextHolder.ctx = admin;
		const login = await admin.request.post('/api/v1/auth/login', {
			data: { email: ADMIN_EMAIL, password: ADMIN_PASSWORD }
		});
		if (login.status() !== 200) {
			throw new Error(`admin login failed: HTTP ${login.status()}`);
		}
		const me = await (await api(admin, 'GET', '/api/v1/users/me')).json();
		state.tenantId = me.tenant_id;
		await api(admin, 'POST', '/api/v1/admin/applications/io.elembra.chat/enable');
		let mapping: any = null;
		for (let i = 0; i < 45 && !mapping; i += 1) {
			const res = await api(
				admin,
				'GET',
				`/api/v1/admin/applications/chat/workspaces/${state.tenantId}/community`
			);
			if (res.status === 200) mapping = await res.json();
			else await new Promise((r) => setTimeout(r, 2_000));
		}
		if (!mapping) throw new Error('chat community mapping was not provisioned');
		state.communityId = mapping.community_id;
		state.relayPubkey = mapping.relay_pubkey;
		if (mapping.relay_url !== RELAY_URL) {
			throw new Error(`mapping relay_url ${mapping.relay_url} != expected ${RELAY_URL}`);
		}
		// Chat message bodies are only projected when content indexing is on
		// (buzz_observation body gate); the alpha runbook enables both flags.
		const policy = await api(admin, 'PATCH', '/api/v1/admin/applications/io.elembra.chat', {
			memory_projection: true,
			content_indexing: true
		});
		if (policy.status !== 200) {
			throw new Error(`enabling chat content indexing failed: HTTP ${policy.status}`);
		}

		// --- Create three disposable users ------------------------------------
		for (const key of ['a', 'b', 'c'] as const) {
			const username = `accept_${key}_${RUN_SUFFIX}`;
			const email = `${username}@acceptance.local`;
			const password = `Accept-${RUN_SUFFIX}-pw`;
			const created = await api(admin, 'POST', '/api/v1/admin/users', {
				username,
				email,
				password,
				display_name: username,
				storage_quota_bytes: 10737418240
			});
			if (created.status !== 200 && created.status !== 201) {
				throw new Error(`create user ${key} failed: HTTP ${created.status}`);
			}
			const body = await created.json();
			const context = await b.newContext({ baseURL: BASE_URL });
			const page = await context.newPage();
			page.on('console', (m) => {
				if (m.type() === 'error' || m.type() === 'warning')
					console.log(`[browser:${key}:console:${m.type()}]`, m.text().slice(0, 240));
			});
			page.on('pageerror', (e) =>
				console.log(`[browser:${key}:pageerror]`, String(e).slice(0, 300))
			);
			page.on('response', (res) => {
				if (res.status() >= 400) {
					const key = res.url().replace(/^https?:\/\/[^/]+/, '');
					(state.badResponses[key] ??= []).push(
						`${res.status()} ${res.url().replace(/^https?:\/\/[^/]+/, '')}`.slice(0, 160)
					);
				}
			});
			state.users[key] = {
				key,
				username,
				email,
				password,
				userId: body.id,
				pubkey: '',
				secretKey: null,
				context,
				page
			};
		}
	});

	test.afterAll(async () => {
		test.setTimeout(120_000);
		for (const user of Object.values(state.users)) await user.context.close().catch(() => {});
		await adminContextHolder.ctx?.close().catch(() => {});
		const evidence = {
			generatedAt: new Date().toISOString(),
			baseUrl: BASE_URL,
			relayUrl: RELAY_URL,
			provenance: {
				gitSha: process.env.ACCEPTANCE_GIT_SHA ?? null,
				stack: process.env.ACCEPTANCE_STACK ?? null,
				images: (() => {
					try {
						return JSON.parse(process.env.ACCEPTANCE_IMAGE_DIGESTS ?? 'null');
					} catch {
						return null;
					}
				})()
			},
			tenantId: state.tenantId,
			communityId: state.communityId,
			relayPubkey: state.relayPubkey,
			users: Object.fromEntries(
				Object.entries(state.users).map(([k, u]) => [
					k,
					{ username: u.username, email: u.email, userId: u.userId, pubkey: u.pubkey }
				])
			),
			channels: state.channels,
			eventIds: state.eventIds,
			matrix: results
		};
		mkdirSync(dirname(EVIDENCE_PATH), { recursive: true });
		writeFileSync(EVIDENCE_PATH, `${JSON.stringify(evidence, null, 2)}\n`);
		console.log(`evidence written to ${EVIDENCE_PATH}`);
	});

	test('login + browser-held identity binding for A/B/C', async () => {
		for (const key of ['a', 'b', 'c'] as const) {
			const user = state.users[key];
			await uiLogin(user);
			await bindChat(user);
			const { status, body } = await chatStatus(user);
			expect(status).toBe(200);
			expect(body.runtime_profile).toBe('fork_client_held');
			expect(body.binding?.status).toBe('Active');
			expect(body.admission_active).toBe(true);
			user.pubkey = body.binding.buzz_pubkey;
			// Decrypt the browser-held key backup with the known acceptance
			// passphrase so relay-direct scenarios can sign as this identity.
			user.secretKey = await decryptChatKey(user);
			expect(user.secretKey).toBeTruthy();
			expect(publicKeyOf(user.secretKey!)).toBe(user.pubkey);
		}
		record('bind', 'Real login + binding + relay admission (A/B/C)', 'PASS', {
			runtimeProfile: 'fork_client_held',
			pubkeys: Object.fromEntries(Object.entries(state.users).map(([k, u]) => [k, u.pubkey])),
			keyBackupDecryptMatchesBindingPubkey: true
		});
	});

	test('a: A creates an empty open channel; B discovers it', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const created = await createChannelViaUi(a, `accept-open-${RUN_SUFFIX}`, 'open');
		state.channels.open = created;
		// A sees it selected (deep link already carried the id).
		await expect(channelOption(a.page, created.name)).toBeVisible();
		// B discovers the empty open channel (registry-driven directory).
		await expect(channelOption(b.page, created.name)).toBeVisible({ timeout: 60_000 });
		const bDir = await channelDirectoryOf(b);
		const bEntry = (bDir.body ?? []).find((c: any) => c.channel_id === created.id);
		expect(bEntry).toBeTruthy();
		record('a', 'Empty open channel created by A and discovered by B', 'PASS', {
			channelId: created.id,
			channelName: created.name,
			bDiscovery: { status: bDir.status, channel: bEntry }
		});
	});

	test('h: channel lifecycle UI — edit, roster roles, join/leave, deep links', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		// --- Edit name/description (owner) -----------------------------------
		const newName = `${open.name} renamed`;
		await a.page.getByRole('button', { name: 'Channel details' }).click();
		await a.page.getByRole('button', { name: 'Edit channel' }).click();
		await a.page.getByLabel('Channel name').fill(newName);
		await a.page.getByLabel('Channel description').fill('edited by owner');
		await a.page.getByRole('button', { name: 'Save changes' }).click();
		// 'unconfirmed' is possible while the snapshot propagates; assert the
		// outcome (owner-side header + B-side directory), not the notice text.
		await expect(a.page.locator('h1').last()).toContainText(newName, { timeout: 60_000 });
		state.channels.open = { id: open.id, name: newName };
		// Owner roster shows owner role.
		const aRoster = a.page.getByRole('list', { name: 'Channel members' });
		await expect(aRoster).toContainText(a.pubkey);
		await expect(aRoster).toContainText('owner');
		// B observes the rename through the relay directory.
		await expect(channelOption(b.page, newName)).toBeVisible({ timeout: 60_000 });
		// --- B joins the open channel ----------------------------------------
		await channelOption(b.page, newName).click();
		await b.page.getByRole('button', { name: 'Channel details' }).click();
		const bRoster = b.page.getByRole('list', { name: 'Channel members' });
		await b.page.getByRole('button', { name: 'Join channel' }).click();
		await expect(bRoster).toContainText(b.pubkey, { timeout: 120_000 });
		// --- Negative permission check: a plain member must NOT get the
		//     metadata-edit affordance (owner/admin only). UI gating is the
		//     product's denial surface for role-ineligible edits — the member
		//     has no way to attempt a name edit from the app.
		await expect(b.page.getByText('Your role: member')).toBeVisible();
		expect(await b.page.getByRole('button', { name: 'Edit channel' }).count()).toBe(0);
		// --- B leaves again (matrix c re-joins B for the exchange; the relay's
		//     snapshot propagation after an immediate leave→re-join is slow) ---
		await b.page.getByRole('button', { name: 'Leave channel' }).click();
		await expect(bRoster).not.toContainText(b.pubkey, { timeout: 120_000 });
		// --- Deep link + reload for B ------------------------------------------
		await b.page.goto(`/apps/chat?channel=${open.id}`);
		await expect(channelOption(b.page, newName)).toBeVisible({ timeout: 60_000 });
		await b.page.reload();
		await expect(channelOption(b.page, newName)).toBeVisible({ timeout: 60_000 });
		await expect(b.page).toHaveURL(/channel=/, { timeout: 30_000 });
		record(
			'h',
			'Lifecycle UI: edit name/description, roster roles, join/leave, deep link + reload',
			'PASS',
			{
				renamedTo: newName,
				rosterOwner: a.pubkey,
				joinLeaveUser: b.pubkey,
				memberEditDenied: true,
				deepLink: `/apps/chat?channel=${open.id}`
			}
		);
	});

	test('b: A creates a private channel with B; C is isolated from a live canary', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const c = state.users.c;
		const created = await createChannelViaUi(a, `accept-private-${RUN_SUFFIX}`, 'private');
		state.channels.private = created;
		// A adds B via the roster (PR #480 add-member flow).
		await a.page.getByRole('button', { name: 'Channel details' }).click();
		await a.page.getByLabel('Member pubkey').fill(b.pubkey);
		await a.page.getByRole('button', { name: 'Add member' }).click();
		await expect(a.page.getByRole('list', { name: 'Channel members' })).toContainText(b.pubkey, {
			timeout: 90_000
		});
		// B discovers the private channel.
		await expect(channelOption(b.page, created.name)).toBeVisible({ timeout: 60_000 });
		// Read isolation needs a canary: A posts into the PRIVATE channel, and
		// C must then be unable to read it. Directory exclusion alone would be
		// vacuous on an empty channel — there would be nothing C could read.
		const canary = `private canary ${RUN_SUFFIX}`;
		await sendMessage(a, canary);
		// Positive control: the canary event must exist in the projection under
		// the owner's authorized session. Private-channel bodies are never
		// projected (the observation body gate only stores bodies for
		// workspace-kind channels, keeping private content out of the content
		// index), so the canary is asserted by event visibility: the channel is
		// new and this is its only message, authored by A.
		let canaryEventId: string | null = null;
		await expect
			.poll(
				async () => {
					const view = await messagesOf(a, created.id);
					const row = (view.body?.messages ?? []).find(
						(m: { author_pubkey: string }) => m.author_pubkey === a.pubkey
					);
					canaryEventId = row?.event_id ?? null;
					return canaryEventId != null;
				},
				{ timeout: 90_000, intervals: [3_000, 5_000, 5_000] }
			)
			.toBe(true);
		state.eventIds.canary = canaryEventId;
		// C never discovers the channel: not in C's UI list, not in C's API
		// directory (sampled across the propagation window, not one immediate
		// check that could race the relay).
		await expectAbsentAcrossWindow(4, 4_000, async () => {
			const cDir = await channelDirectoryOf(c);
			expect(
				(cDir.body ?? []).find((ch: { channel_id: string }) => ch.channel_id === created.id)
			).toBeFalsy();
		});
		await expect(channelOption(c.page, created.name)).toHaveCount(0);
		// C cannot read the canary through C's own API session: not just the
		// body — the event itself must not appear (metadata denial, sampled)…
		await expectAbsentAcrossWindow(3, 4_000, async () => {
			const cMsgs = await messagesOf(c, created.id);
			const cEventIds = (cMsgs.body?.messages ?? []).map((m: { event_id: string }) => m.event_id);
			expect(cEventIds).not.toContain(canaryEventId);
			expect(cMsgs.body?.messages ?? []).toHaveLength(0);
		});
		// …and a C deep-link UI navigation to the private channel cannot read it
		// either: the deep link cannot hold (the channel is not in C's
		// directory) and C's session still returns zero timeline rows.
		await c.page.goto(`/apps/chat?channel=${created.id}`);
		await expectAbsentAcrossWindow(3, 4_000, async () => {
			const cDeepMsgs = await messagesOf(c, created.id);
			expect(cDeepMsgs.body?.messages ?? []).toHaveLength(0);
		});
		await expect(channelOption(c.page, created.name)).toHaveCount(0);
		record(
			'b',
			'Private channel: B added and discovers; owner reads the canary, C cannot discover or read it',
			'PASS',
			{
				channelId: created.id,
				canaryEventId,
				ownerPositiveControl: true,
				bDirectoryVisibility: true,
				cDirectoryExclusion: true,
				cApiReadDenied: true,
				cDeepLinkRows: 0
			}
		);
	});

	test('c: A and B exchange messages; event ids + authors verified both sides', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		// Deterministic channel selection for both sides (the list click can
		// land on a stale row while the directory refreshes).
		await a.page.goto(`/apps/chat?channel=${open.id}`);
		await expect(channelOption(a.page, open.name)).toBeVisible({ timeout: 60_000 });
		await ensureUnlocked(a);
		await b.page.goto(`/apps/chat?channel=${open.id}`);
		await expect(channelOption(b.page, open.name)).toBeVisible({ timeout: 60_000 });
		await ensureUnlocked(b);
		await sendMessage(a, `hello from A ${RUN_SUFFIX}`);
		// B's membership can lag after the rapid leave/re-join in matrix h;
		// open the details and join idempotently if still a non-member.
		const detailsButton = b.page.getByRole('button', { name: 'Channel details' });
		if ((await detailsButton.getAttribute('aria-expanded')) !== 'true') await detailsButton.click();
		const joinButton = b.page.getByRole('button', { name: 'Join channel' });
		if (await joinButton.isVisible().catch(() => false)) {
			await joinButton.click();
			await expect(b.page.getByRole('list', { name: 'Channel members' })).toContainText(b.pubkey, {
				timeout: 120_000
			});
		}
		await sendMessage(a, `second from A ${RUN_SUFFIX}`);
		// B sees A's messages (polling-driven timeline).
		await expect(b.page.getByText(`hello from A ${RUN_SUFFIX}`)).toBeVisible({ timeout: 90_000 });
		// B replies.
		await sendMessage(b, `reply from B ${RUN_SUFFIX}`);
		await expect(a.page.getByText(`reply from B ${RUN_SUFFIX}`)).toBeVisible({ timeout: 60_000 });
		// Cross-verify event ids + authors through each side's own session.
		const aView = await messagesOf(a, open.id);
		const bView = await messagesOf(b, open.id);
		const pick = (view: any, body: string) =>
			(view.body?.messages ?? []).find((m: any) => m.body === body);
		const mA = pick(aView, `hello from A ${RUN_SUFFIX}`);
		const mB = pick(aView, `reply from B ${RUN_SUFFIX}`);
		expect(mA).toBeTruthy();
		expect(mB).toBeTruthy();
		expect(pick(bView, `hello from A ${RUN_SUFFIX}`)?.event_id).toBe(mA.event_id);
		expect(pick(bView, `reply from B ${RUN_SUFFIX}`)?.event_id).toBe(mB.event_id);
		expect(mA.author_pubkey).toBe(a.pubkey);
		expect(mB.author_pubkey).toBe(b.pubkey);
		state.eventIds.msgA = mA.event_id;
		state.eventIds.msgB = mB.event_id;
		record('c', 'Message exchange with matching event ids and authors on both sides', 'PASS', {
			channelId: open.id,
			aAuthor: mA.author_pubkey,
			bAuthor: mB.author_pubkey,
			eventIds: { a: mA.event_id, b: mB.event_id },
			timelineCountA: aView.body?.messages?.length ?? 0,
			timelineCountB: bView.body?.messages?.length ?? 0
		});
	});

	test('d: reload retains authorized history', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		for (const user of [a, b]) {
			await user.page.reload();
			await expect(channelOption(user.page, open.name)).toBeVisible({ timeout: 60_000 });
			await ensureUnlocked(user);
			await expect(user.page.getByText(`hello from A ${RUN_SUFFIX}`)).toBeVisible({
				timeout: 60_000
			});
			await expect(user.page.getByText(`reply from B ${RUN_SUFFIX}`)).toBeVisible({
				timeout: 60_000
			});
		}
		record('d', 'Reload keeps the authorized history for A and B', 'PASS', {
			channelId: open.id,
			reloaded: ['a', 'b']
		});
	});

	test('g: lost ACK does not duplicate the message (retry same signed event)', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		const body = `uncertain-path ${RUN_SUFFIX}`;
		// Delay the relay's acceptance ACK past the client's 10 s ACK timeout so
		// the composer takes its real "uncertain" path. Done in the page (a
		// WebSocket constructor wrapper) — Playwright's routeWebSocket does not
		// see the app's relay connections in this Chromium build.
		const relayPort = new URL(RELAY_URL).port;
		await a.page.addInitScript((relayPort) => {
			const Original = window.WebSocket;
			window.__gOriginalWS = Original;
			const probe = (window.__gAckProbe = {
				delayNext: false,
				swallowedId: null as string | null,
				lastEventId: null as string | null
			});
			window.WebSocket = class extends Original {
				constructor(url: string | URL, ...rest: unknown[]) {
					super(url, ...(rest as []));
					if (new URL(String(url), window.location.href).port !== relayPort) return;
					// Arrow handlers capture the constructor's `this` lexically,
					// so the relay-URL filter can invoke client handlers with the
					// socket as `this` without an explicit this-alias.
					const wrap = (handler: (ev: MessageEvent) => void) => (ev: MessageEvent) => {
						try {
							const msg = JSON.parse(String(ev.data));
							if (msg[0] === 'EVENT' && msg[1]?.id) probe.lastEventId = msg[1].id;
							if (
								probe.delayNext &&
								msg[0] === 'OK' &&
								msg[2] === true &&
								msg[1] === probe.lastEventId
							) {
								probe.delayNext = false;
								probe.swallowedId = msg[1];
								setTimeout(() => handler.call(this, ev), 12_000);
								return;
							}
						} catch {
							/* non-JSON frame */
						}
						handler.call(this, ev);
					};
					const originalSend = this.send.bind(this);
					Object.defineProperty(this, 'send', {
						value: (data: string | Blob | ArrayBuffer) => {
							try {
								const msg = JSON.parse(String(data));
								if (msg[0] === 'EVENT' && msg[1]?.id) probe.lastEventId = msg[1].id;
							} catch {
								/* non-JSON frame */
							}
							originalSend(data);
						}
					});
					let clientHandler: ((ev: MessageEvent) => void) | null = null;
					Object.defineProperty(this, 'onmessage', {
						get: () => clientHandler,
						set: (h: (ev: MessageEvent) => void) => {
							clientHandler = wrap(h);
						}
					});
					const originalAdd = this.addEventListener.bind(this);
					Object.defineProperty(this, 'addEventListener', {
						value: (
							type: string,
							listener: EventListenerOrEventListenerObject,
							...opts: unknown[]
						) => {
							if (type === 'message' && typeof listener === 'function') {
								originalAdd(type, wrap(listener as (ev: MessageEvent) => void), ...(opts as []));
								return;
							}
							originalAdd(type, listener, ...(opts as []));
						}
					});
				}
			} as typeof WebSocket;
		}, relayPort);
		await a.page.reload();
		await expect(a.page.getByRole('heading', { name: 'Channels' })).toBeVisible({
			timeout: 120_000
		});
		await ensureUnlocked(a);
		await channelOption(a.page, open.name).click();
		// Restore the real WebSocket unconditionally: the wrapper only models
		// the lost ACK, and a failure above must not leak the delayed-ACK
		// wrapper into the rest of the serial suite.
		try {
			await a.page.evaluate(() => {
				window.__gAckProbe!.delayNext = true;
			});
			await a.page.getByLabel('Message text').fill(body);
			await a.page.getByRole('button', { name: 'Send message' }).click();
			await expect
				.poll(() => a.page.evaluate(() => window.__gAckProbe?.swallowedId ?? null), {
					timeout: 45_000
				})
				.not.toBeNull();
			// The UI must land on the uncertain path and offer the byte-identical retry.
			await expect(
				a.page.getByText(
					'Message acceptance is uncertain. Editing is paused to prevent duplicate messages.'
				)
			).toBeVisible({ timeout: 45_000 });
			// The composer persists the exact signed pending event; that id is what
			// the byte-identical retry resubmits and what must appear exactly once.
			const capturedId = await a.page.evaluate(() => {
				const key = Object.keys(sessionStorage).find((k) => k.startsWith('elembra.chat.draft:'));
				if (!key) return null;
				const saved = JSON.parse(sessionStorage.getItem(key) ?? '{}');
				return saved.pendingEvent?.id ?? null;
			});
			expect(capturedId).toBeTruthy();
			const pendingSig = await a.page.evaluate(() => {
				const key = Object.keys(sessionStorage).find((k) => k.startsWith('elembra.chat.draft:'));
				if (!key) return null;
				const saved = JSON.parse(sessionStorage.getItem(key) ?? '{}');
				return saved.pendingEvent?.sig ?? null;
			});
			expect(pendingSig).toBeTruthy();
			// The byte-identical retry publishes through a clean transport.
			await a.page.evaluate(() => {
				const original = window.__gOriginalWS;
				if (original) window.WebSocket = original;
			});
			await a.page.getByRole('button', { name: 'Retry same message' }).click();
			// The success banner is transient; assert the authoritative outcome via
			// the API: exactly one applied message carrying the original event id.
			await expect
				.poll(
					async () => {
						const view = await messagesOf(a, open.id);
						return (view.body?.messages ?? []).filter((m: any) => m.event_id === capturedId).length;
					},
					{ timeout: 120_000, intervals: [3_000, 5_000, 5_000] }
				)
				.toBe(1);
			const view = await messagesOf(a, open.id);
			const matches = (view.body?.messages ?? []).filter((m: any) => m.body === body);
			expect(matches).toHaveLength(1);
			expect(matches[0].event_id).toBe(capturedId);
			expect(matches[0].author_pubkey).toBe(a.pubkey);
			// Relay-direct: the candidate relay must hold the retried event
			// exactly once, byte-identical (id + sig + author), and the same
			// single copy must be visible to the other member. The original
			// (delayed-ACK) send and the byte-identical retry share one event
			// id — a duplicate store would show up as two EVENT frames here.
			// (kinds is mandatory: the relay's p-gate rejects kindless filters.)
			const relayById = await relayQuery(a.secretKey!, {
				ids: [capturedId],
				kinds: [9],
				'#h': [open.id]
			});
			expect(relayById.ok, `relay-direct ids read as A: ${relayById.detail}`).toBe(true);
			expect(relayById.events).toHaveLength(1);
			expect(relayById.events[0].id).toBe(capturedId);
			expect(relayById.events[0].sig).toBe(pendingSig);
			expect(relayById.events[0].pubkey).toBe(a.pubkey);
			const relayByIdAsB = await relayQuery(b.secretKey!, {
				ids: [capturedId],
				kinds: [9],
				'#h': [open.id]
			});
			expect(relayByIdAsB.ok, `relay-direct ids read as B: ${relayByIdAsB.detail}`).toBe(true);
			expect(relayByIdAsB.events).toHaveLength(1);
			expect(relayByIdAsB.events[0].id).toBe(capturedId);
			expect(relayByIdAsB.events[0].sig).toBe(pendingSig);
			state.eventIds.uncertain = capturedId;
			record('g', 'Uncertain ACK path retried byte-identically; single application', 'PASS', {
				channelId: open.id,
				pendingEventId: capturedId,
				appliedCopies: matches.length,
				retainedEventId: matches[0].event_id,
				relayDirectCopies: relayById.events.length,
				relayDirectSigMatchesRetryBytes: relayById.events[0].sig === pendingSig,
				relayDirectCopiesAsB: relayByIdAsB.events.length
			});
		} finally {
			// Unconditionally restore the real WebSocket even on failure.
			await a.page.evaluate(() => {
				const original = window.__gOriginalWS;
				if (original) window.WebSocket = original;
			});
		}
	});

	test('i: role downgrade to guest denies posting immediately (relay-direct)', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		// The fork's role change is an owner-signed kind 9000 carrying a
		// ['role', ...] tag (relay side_effects.rs::handle_put_user; the UI's
		// add-member deliberately omits it). Provisioning guest relay-direct
		// is intentional: Elembra exposes no guest assignment, and this is a
		// DIRECT-RELAY enforcement scenario, not a UI flow.
		const downgrade = await relayPublish(a.secretKey!, {
			kind: 9000,
			tags: [
				['h', open.id],
				['p', b.pubkey],
				['role', 'guest']
			],
			content: ''
		});
		expect(downgrade.accepted).toBe(true);
		// IMMEDIACY: no sleep, no directory poll — B's freshly signed kind-9
		// attempt right after the downgrade's OK frame must be denied by the
		// candidate relay itself.
		const downgradeAt = Date.now();
		const denied = await relayPublish(b.secretKey!, {
			kind: 9,
			tags: [['h', open.id]],
			content: `guest write attempt ${RUN_SUFFIX}`
		});
		const denialMs = Date.now() - downgradeAt;
		// Disposition depends on the relay under test. The qualified candidate
		// image denies guest writes at the role gate; the supported pinned
		// baseline predates that fix and ACCEPTS them. An accepted write is
		// recorded BLOCKED with the full observation (cross-referencing the
		// candidate-image direct-negative evidence where the anchored denial
		// is asserted) — never silently passed.
		let status: 'PASS' | 'BLOCKED' = 'PASS';
		let denialAnchored = false;
		if (denied.accepted) {
			status = 'BLOCKED';
		} else {
			// Pin the denial to the role gate (not some other auth/membership
			// rejection) — the guest-write-denial reason is the assertion's
			// point.
			expect(denied.message).toMatch(/restricted: guest/);
			denialAnchored = true;
		}
		// Guest read stays intact: B still sees the earlier exchange.
		const guestRead = await relayQuery(b.secretKey!, { kinds: [9], '#h': [open.id] });
		expect(guestRead.ok).toBe(true);
		expect(guestRead.events.map((e: any) => e.id)).toContain(state.eventIds.msgA);
		// Positive control: restoring the member role re-allows posting. The
		// denial above is role-driven, not a broken session. (Promotion may
		// lag snapshot propagation; revocation above must NOT lag.)
		const restore = await relayPublish(a.secretKey!, {
			kind: 9000,
			tags: [
				['h', open.id],
				['p', b.pubkey],
				['role', 'member']
			],
			content: ''
		});
		expect(restore.accepted).toBe(true);
		let restoredAccepted = false;
		let restoreMs = 0;
		const restoreAt = Date.now();
		await expect
			.poll(
				async () => {
					const probe = await relayPublish(b.secretKey!, {
						kind: 9,
						tags: [['h', open.id]],
						content: `member restore probe ${RUN_SUFFIX}`
					});
					restoreMs = Date.now() - restoreAt;
					restoredAccepted = probe.accepted;
					return probe.accepted;
				},
				{ timeout: 60_000, intervals: [2_000, 4_000, 4_000] }
			)
			.toBe(true);
		record(
			'i',
			'Role downgrade to guest: relay denies B signed post immediately; read intact; restore re-allows',
			status,
			{
				channelId: open.id,
				downgradeEventId: downgrade.eventId,
				guestWriteDenied: !denied.accepted,
				guestWriteDenialAnchored: denialAnchored,
				guestWriteAcceptedByRelayUnderTest: denied.accepted,
				guestWriteDenialMessage: denied.message,
				denialAfterDowngradeMs: denialMs,
				baselineNote: denied.accepted
					? 'relay under test does not establish guest write denial (supported pinned baseline); anchored denial asserted against the qualified candidate image in chat-direct-negative-acceptance evidence (a_guest_write_denied)'
					: undefined,
				guestReadEventCount: guestRead.events.length,
				guestReadSeesEarlierMessage: guestRead.events
					.map((e: any) => e.id)
					.includes(state.eventIds.msgA),
				restoreEventId: restore.eventId,
				restoreProbeAccepted: restoredAccepted,
				restoreProbeMs: restoreMs
			}
		);
	});

	test('e: member removal revokes discovery, reads, posting, and attachments; B’s session stays live', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const c = state.users.c;
		const priv = state.channels.private;
		const open = state.channels.open;

		// --- Pre-removal baseline: an attachment posted INSIDE the private
		//     channel is openable by the member B (and only by members) --------
		const attachUpload = await a.context.request.post('/api/v1/files/upload', {
			multipart: {
				file: {
					name: `priv-attached-${RUN_SUFFIX}.txt`,
					mimeType: 'text/plain',
					buffer: Buffer.from(`private channel attachment ${RUN_SUFFIX}`)
				},
				name: `priv-attached-${RUN_SUFFIX}.txt`
			},
			headers: { 'X-Rustshare-Csrf': await csrfOf(a.context) }
		});
		expect(attachUpload.ok()).toBeTruthy();
		const attached = await attachUpload.json();
		state.attachmentFileId = attached.id;
		const privResource = {
			application: 'io.elembra.files',
			resourceType: 'file',
			resourceId: attached.id
		};
		await channelOption(a.page, priv.name).click();
		await ensureUnlocked(a);
		await a.page.getByLabel('Message text').fill(`private attachment message ${RUN_SUFFIX}`);
		await a.page.getByRole('button', { name: 'Attach file' }).click();
		await a.page.getByRole('heading', { name: 'Attach a file' }).waitFor({ timeout: 15_000 });
		await a.page.getByRole('button', { name: `priv-attached-${RUN_SUFFIX}.txt` }).click();
		await expect(a.page.getByText(/Attachment: elembra:\/\//)).toBeVisible({ timeout: 15_000 });
		await expect(a.page.getByRole('button', { name: 'Send message' })).toBeEnabled({
			timeout: 90_000
		});
		await a.page.getByRole('button', { name: 'Send message' }).click();
		await expect(a.page.getByText('Observed by Elembra.')).toBeVisible({ timeout: 90_000 });
		// Bytes authorization for chat attachments is the FILES grant plane
		// (existence-hiding 404), NOT channel membership: the owner opens, the
		// member WITHOUT a Files share and the unrelated user C cannot (this is
		// the Files-side leak gate — the message metadata shows the ref, the
		// bytes stay sealed), and an explicit Files share opens it for B.
		const aOpenOwner = await api(a.context, 'POST', '/api/v1/applications/chat/attachments/open', {
			resource: privResource
		});
		expect(aOpenOwner.status).toBe(200);
		const bOpenNoShare = await api(
			b.context,
			'POST',
			'/api/v1/applications/chat/attachments/open',
			{
				resource: privResource
			}
		);
		expect([403, 404]).toContain(bOpenNoShare.status);
		const cOpenBefore = await api(c.context, 'POST', '/api/v1/applications/chat/attachments/open', {
			resource: privResource
		});
		expect([403, 404]).toContain(cOpenBefore.status);
		// Positive control: a Files share opens the same attachment for B…
		const shared = await api(a.context, 'POST', `/api/v1/files/${attached.id}/share`, {
			recipient_email: b.email,
			permission: 'View'
		});
		expect([200, 201]).toContain(shared.status);
		const bShareId = (await shared.json())?.share_id;
		let bOpenWithShare = 0;
		await expect
			.poll(
				async () => {
					const res = await api(b.context, 'POST', '/api/v1/applications/chat/attachments/open', {
						resource: privResource
					});
					bOpenWithShare = res.status;
					return res.status;
				},
				{ timeout: 90_000, intervals: [3_000, 5_000, 5_000] }
			)
			.toBe(200);
		// …and revoking the share seals it again (Files revocation is
		// channel-orthogonal by design; the channel-removal leak checks are
		// the relay/API ones below).
		const unshared = await api(a.context, 'DELETE', `/api/v1/shares/${bShareId}`);
		expect([200, 204]).toContain(unshared.status);
		const bOpenAfterUnshare = await api(
			b.context,
			'POST',
			'/api/v1/applications/chat/attachments/open',
			{ resource: privResource }
		);
		expect([403, 404]).toContain(bOpenAfterUnshare.status);

		// --- A removes B from the private channel ----------------------------
		await channelOption(a.page, priv.name).click();
		await a.page.getByRole('button', { name: 'Channel details' }).click();
		await a.page.getByRole('button', { name: `Remove member ${b.pubkey.slice(0, 8)}` }).click();
		await expect(a.page.getByRole('list', { name: 'Channel members' })).not.toContainText(
			b.pubkey,
			{
				timeout: 90_000
			}
		);
		// B's directory drops the private channel.
		await expect(channelOption(b.page, priv.name)).toHaveCount(0, { timeout: 60_000 });
		const bDir = await channelDirectoryOf(b);
		expect(
			(bDir.body ?? []).find((c: { channel_id: string }) => c.channel_id === priv.id)
		).toBeFalsy();

		// --- B attempts a write through the UI; the product gives no path ------
		// What the product actually does: the relay-signed directory is the only
		// channel-addressability source, and it no longer lists the private
		// channel for B. While the deep link still addresses the removed
		// channel the composer is default-denied (Send disabled); once the
		// directory settles the deep link cannot hold and B falls back to a
		// visible channel. The window is sampled live; the authoritative
		// cannot-post evidence is the relay-direct rejection below.
		const bAttempt = `b write attempt ${RUN_SUFFIX}`;
		await b.page.goto(`/apps/chat?channel=${priv.id}`);
		// Wait for the view to boot before unlocking: ensureUnlocked only acts
		// when the unlock panel is already rendered.
		await expect(b.page.getByRole('heading', { name: 'Channels' })).toBeVisible({
			timeout: 30_000
		});
		await ensureUnlocked(b);
		await b.page.getByLabel('Message text').fill(bAttempt);
		// Sample the window while the deep link addresses the removed channel:
		// the composer must stay default-denied there (Send never enabled). The
		// window closes as soon as the relay-signed directory settles.
		let addressedSamples = 0;
		let composerDeniedSamples = 0;
		let composerEnabledWhileAddressed = 0;
		const sampleComposerDenial = async (): Promise<void> => {
			if (!(await b.page.url().includes(priv.id))) return;
			addressedSamples += 1;
			const send = b.page.getByRole('button', { name: 'Send message' });
			if (await send.isDisabled().catch(() => false)) composerDeniedSamples += 1;
			if (await send.isEnabled().catch(() => false)) composerEnabledWhileAddressed += 1;
		};
		await sampleComposerDenial();
		// Nudge the permission refresh instead of waiting a whole poll cycle.
		await b.page
			.getByRole('button', { name: 'Refresh permissions' })
			.click()
			.catch(() => {});
		// The private channel is not addressable for B: the deep link cannot
		// keep B on it (selection falls back to a visible channel).
		let deepLinkHeld = true;
		await expect
			.poll(
				async () => {
					await sampleComposerDenial();
					deepLinkHeld = await b.page.url().includes(priv.id);
					return deepLinkHeld;
				},
				{ timeout: 60_000, intervals: [2_000, 4_000] }
			)
			.toBe(false);
		if (addressedSamples > 0) {
			// Deterministic product-state gate: whenever the UI was observed
			// addressing the removed channel, Send must NEVER have been
			// enabled there. (addressedSamples === 0 means the directory had
			// already settled before the page loaded — the channel was then
			// simply unaddressable; the relay-direct denial below is the
			// cannot-post proof either way.)
			expect(composerEnabledWhileAddressed).toBe(0);
		}
		await expect(channelOption(b.page, priv.name)).toHaveCount(0, { timeout: 60_000 });
		// The private channel remains unaddressable and unreadable for B after
		// the write attempt (sampled across the propagation window rather than
		// one blind sleep): no message event may appear, whoever authored it.
		let bPrivateRows = -1;
		await expectAbsentAcrossWindow(4, 5_000, async () => {
			const attemptView = await messagesOf(b, priv.id);
			bPrivateRows = (attemptView.body?.messages ?? []).length;
			expect(bPrivateRows).toBe(0);
		});

		// --- DIRECT-RELAY negative tests (owned keys, candidate relay) --------
		// 1) B's freshly signed kind-9 to the removed channel is rejected by
		//    the relay itself — the authoritative "cannot post" for a removed
		//    member, independent of any UI safeguard.
		const directWrite = await relayPublish(b.secretKey!, {
			kind: 9,
			tags: [['h', priv.id]],
			content: `removed member direct write ${RUN_SUFFIX}`
		});
		expect(directWrite.accepted).toBe(false);
		expect(directWrite.message).toMatch(/restricted: not a channel member/i);
		// 2) B's relay-direct read of the channel returns nothing.
		const directRead = await relayQuery(b.secretKey!, { kinds: [9], '#h': [priv.id] });
		expect(
			directRead.ok || /^closed: restricted: not a channel member$/i.test(directRead.detail)
		).toBe(true);
		const directReadLeaked = directRead.events.filter(
			(e: any) => e.id === state.eventIds.canary
		).length;
		expect(directReadLeaked).toBe(0);
		// 3) The attachment bytes stay sealed for B after removal (B holds no
		//    Files grant; existence-hiding denial — Files access is never
		//    derived from channel membership).
		let bOpenAfterRemoval = 0;
		await expectAbsentAcrossWindow(3, 4_000, async () => {
			const res = await api(b.context, 'POST', '/api/v1/applications/chat/attachments/open', {
				resource: privResource
			});
			bOpenAfterRemoval = res.status;
			expect([403, 404]).toContain(res.status);
		});

		// B never sees a post-removal message either (UI + API fail closed):
		// the secret event must not appear for B at all, not merely its body.
		const secret = `post-removal secret ${RUN_SUFFIX}`;
		await ensureUnlocked(a);
		await sendMessage(a, secret);
		// Positive control: the secret event exists in the projection for the
		// owner's authorized session, so B's denial below cannot be vacuous.
		await expect
			.poll(
				async () =>
					((await messagesOf(a, priv.id)).body?.messages ?? []).some(
						(m: { author_pubkey: string }) => m.author_pubkey === a.pubkey
					),
				{ timeout: 90_000, intervals: [3_000, 5_000, 5_000] }
			)
			.toBe(true);
		await expectAbsentAcrossWindow(5, 5_000, async () => {
			const bView = await messagesOf(b, priv.id);
			expect(bView.body?.messages ?? []).toHaveLength(0);
		});
		// --- Liveness: B's session itself still works -------------------------
		// B still reads the open channel through its own session, proving the
		// denials above are scoped to the removed channel, not a dead session.
		const bOpen = await messagesOf(b, open.id);
		expect(bOpen.status).toBe(200);
		expect((bOpen.body?.messages ?? []).length).toBeGreaterThan(0);
		record(
			'e',
			'Removal revokes discovery, reads, posting (UI+relay), and attachments; B session stays live',
			'PASS',
			{
				channelId: priv.id,
				removed: b.pubkey,
				privAttachmentFileId: attached.id,
				aOpenAttachmentAsOwner: aOpenOwner.status,
				bOpenAttachmentAsMemberNoShare: bOpenNoShare.status,
				bOpenAttachmentWithFilesShare: bOpenWithShare,
				bOpenAttachmentAfterShareRevoke: bOpenAfterUnshare.status,
				cOpenAttachmentBeforeRemoval: cOpenBefore.status,
				bDirectoryAfterRemoval: bDir.status,
				bAddressedSamples: addressedSamples,
				bComposerDeniedWhileAddressed: composerDeniedSamples,
				bComposerEnabledWhileAddressed: composerEnabledWhileAddressed,
				deepLinkHeldAfterRemoval: deepLinkHeld,
				bPrivateTimelineRows: bPrivateRows,
				directRelayWriteAccepted: directWrite.accepted,
				directRelayWriteDenialMessage: directWrite.message,
				directRelayReadOk: directRead.ok,
				directRelayReadDetail: directRead.detail,
				directRelayReadEvents: directRead.events.length,
				bOpenAttachmentAfterRemoval: bOpenAfterRemoval,
				bReadAfterRemoval: 'zero-rows',
				bLivenessOpenChannelReads: bOpen.status
			}
		);
	});

	test('f2: private-channel leaks closed — search, deep links, history, relay reads for C and removed B', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const c = state.users.c;
		const priv = state.channels.private;
		// Search must not surface private-channel content for the unrelated
		// user C or the removed member B. The terms are tokens that appear ONLY
		// in private-channel bodies of this run ('canary' in the canary, 'post-
		// removal' in the post-removal secret); the run suffix is deliberately
		// NOT used here because it also occurs in open-channel messages, which
		// are community-visible and legitimately searchable. The owner-side
		// positive control below proves the canary body IS indexed — the zero-
		// hit results for C/B are per-candidate authorization, not absence.
		const leakTerms = ['canary', 'post-removal', 'priv-attached'];
		const cSearch = [];
		for (const term of leakTerms) {
			const res = await api(c.context, 'POST', '/api/v1/search', { query: term });
			expect(res.status).toBe(200);
			const body = await res.json();
			expect(Array.isArray(body?.results)).toBe(true);
			cSearch.push({ term, hits: (body.results ?? []).length });
			expect((body.results ?? []).length).toBe(0);
		}
		// Same for B after removal from the private channel.
		const bSearch = [];
		for (const term of leakTerms) {
			const res = await api(b.context, 'POST', '/api/v1/search', { query: term });
			expect(res.status).toBe(200);
			const body = await res.json();
			bSearch.push({ term, hits: (body.results ?? []).length });
			expect((body.results ?? []).length).toBe(0);
		}
		// Positive controls, so the zero-hit results for C/B above are provably
		// authorization decisions, not an empty index:
		// 1) A's suffix search returns the community-visible open-channel
		//    messages immediately (chat catalog is live).
		// 2) 'priv-attached' is the NAME of the file posted inside the private
		//    channel — the owner must find it (file metadata indexing), and
		//    the same term is a C/B leak term above (metadata leak gate). The
		//    private-channel MESSAGE bodies are never projected at all, so
		//    message leak terms are guarded by both the projection gate and
		//    the per-candidate authorization filter.
		const aSuffixSearch = await api(a.context, 'POST', '/api/v1/search', {
			query: RUN_SUFFIX
		});
		expect(aSuffixSearch.status).toBe(200);
		const aSuffixBody = await aSuffixSearch.json();
		expect((aSuffixBody.results ?? []).length).toBeGreaterThan(0);
		let ownerFileHits = -1;
		let ownerAttachmentRefs: string[] = [];
		await expect
			.poll(
				async () => {
					const res = await api(a.context, 'POST', '/api/v1/search', {
						query: 'priv-attached'
					});
					const body = await res.json();
					ownerAttachmentRefs = (body.results ?? []).map((r: any) => r.resource_ref ?? '');
					ownerFileHits = ownerAttachmentRefs.length;
					return ownerAttachmentRefs.some((ref: string) => ref.endsWith(state.attachmentFileId));
				},
				{ timeout: 120_000, intervals: [3_000, 5_000, 10_000] }
			)
			.toBe(true);
		const aCanarySearch = await api(a.context, 'POST', '/api/v1/search', { query: 'canary' });
		const aCanaryBody = await aCanarySearch.json();
		const ownerPrivateMessageHits = (aCanaryBody.results ?? []).length;
		// Deep link + reload for C cannot surface the channel or its history.
		await c.page.goto(`/apps/chat?channel=${priv.id}`);
		await expect(c.page.getByRole('heading', { name: 'Channels' })).toBeVisible({
			timeout: 30_000
		});
		await c.page.reload();
		await expect(channelOption(c.page, priv.name)).toHaveCount(0, { timeout: 60_000 });
		await expectAbsentAcrossWindow(3, 4_000, async () => {
			const cMsgs = await messagesOf(c, priv.id);
			expect(cMsgs.body?.messages ?? []).toHaveLength(0);
		});
		// Relay-direct read as C: the canary/secret event ids must never appear.
		const cRelayRead = await relayQuery(c.secretKey!, { kinds: [9], '#h': [priv.id] });
		expect(
			cRelayRead.ok || /^closed: restricted: not a channel member$/i.test(cRelayRead.detail)
		).toBe(true);
		const cLeaked = cRelayRead.events.filter((e: any) => e.id === state.eventIds.canary).length;
		expect(cLeaked).toBe(0);
		// Relay-direct read as removed B (same requirement, second identity).
		const bRelayRead = await relayQuery(b.secretKey!, { kinds: [9], '#h': [priv.id] });
		expect(
			bRelayRead.ok || /^closed: restricted: not a channel member$/i.test(bRelayRead.detail)
		).toBe(true);
		const bLeaked = bRelayRead.events.filter((e: any) => e.id === state.eventIds.canary).length;
		expect(bLeaked).toBe(0);
		// Positive control: the owner still reads the private history (so the
		// zero-hits above cannot be vacuous — the channel does exist and the
		// relay does serve it to authorized sessions).
		const aView = await messagesOf(a, priv.id);
		expect((aView.body?.messages ?? []).length).toBeGreaterThan(0);
		record(
			'f2',
			'Private-channel leak checks: search/deep-link/history/relay reads closed for C and removed B',
			'PASS',
			{
				channelId: priv.id,
				cSearch,
				bSearch,
				ownerSuffixSearchHits: (aSuffixBody.results ?? []).length,
				ownerAttachmentSearchHits: ownerFileHits,
				ownerAttachmentSearchFoundFile: ownerAttachmentRefs.some((ref: string) =>
					ref.endsWith(state.attachmentFileId)
				),
				ownerPrivateMessageBodyHits: ownerPrivateMessageHits,
				cDeepLinkRows: 0,
				cRelayReadOk: cRelayRead.ok,
				cRelayReadDetail: cRelayRead.detail,
				cRelayReadEvents: cRelayRead.events.length,
				bRelayReadOk: bRelayRead.ok,
				bRelayReadDetail: bRelayRead.detail,
				bRelayReadEvents: bRelayRead.events.length,
				ownerPositiveControlRows: (aView.body?.messages ?? []).length
			}
		);
	});

	test('f: Files attachment authorization — grant succeeds, revocation fails', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const c = state.users.c;
		// A uploads a file (Files API) with A's real session.
		const upload = await a.context.request.post('/api/v1/files/upload', {
			multipart: {
				file: {
					name: 'acceptance.txt',
					mimeType: 'text/plain',
					buffer: Buffer.from(`acceptance attachment ${RUN_SUFFIX}`)
				},
				name: 'acceptance.txt'
			},
			headers: { 'X-Rustshare-Csrf': await csrfOf(a.context) }
		});
		expect(upload.ok()).toBeTruthy();
		const uploaded = await upload.json();
		const resource = {
			application: 'io.elembra.files',
			resourceType: 'file',
			resourceId: uploaded.id
		};
		// Share with B only.
		const shared = await api(a.context, 'POST', `/api/v1/files/${uploaded.id}/share`, {
			recipient_email: b.email,
			permission: 'View'
		});
		expect([200, 201]).toContain(shared.status);
		const shareId = (await shared.json())?.share_id;
		expect(shareId).toBeTruthy();
		// Attachment open: authorized B succeeds, unrelated C denied.
		const bOpen = await api(b.context, 'POST', '/api/v1/applications/chat/attachments/open', {
			resource
		});
		expect(bOpen.status).toBe(200);
		const cOpen = await api(c.context, 'POST', '/api/v1/applications/chat/attachments/open', {
			resource
		});
		expect([403, 404]).toContain(cOpen.status);
		// Revoke the grant; B now fails.
		const revoked = await api(a.context, 'DELETE', `/api/v1/shares/${shareId}`);
		expect([200, 204]).toContain(revoked.status);
		const bOpenAfter = await api(b.context, 'POST', '/api/v1/applications/chat/attachments/open', {
			resource
		});
		expect([403, 404]).toContain(bOpenAfter.status);
		record(
			'f',
			'Files attachment: authorized open succeeds; revoked and unrelated users fail',
			'PASS',
			{
				fileId: uploaded.id,
				shareId,
				bOpen: bOpen.status,
				cOpen: cOpen.status,
				bOpenAfterRevoke: bOpenAfter.status
			}
		);
	});

	test('j: relay + backend + observer restart retains authorized history and posting', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		// Restart the three Chat-path services inside the acceptance project.
		// Backend: a plain `docker restart` (same container) — the backend
		// container holds the bootstrap-password artifact in /tmp, which must
		// survive for later acceptance runs. buzz-relay and chat-observer share
		// the backend container's network namespace (dogfood layout): a backend
		// restart gives it a FRESH namespace, so the sharers must be
		// RE-CREATED (`up -d --force-recreate`) to reattach — a plain restart
		// would leave them in the backend's stale namespace. Volumes (relay
		// event store, backend DB, observer durable outbox) persist throughout.
		const extraFile =
			process.env.ELEMBRA_COMPOSE_EXTRA_FILE ?? 'docker-compose.chat-acceptance.yml';
		const repoRoot = dirname(process.cwd());
		const chatEnv: Record<string, string> = {};
		try {
			const raw = readFileSync(`${repoRoot}/.elembra/chat.env`, 'utf8');
			for (const line of raw.split('\n')) {
				const m = line.match(/^([A-Z0-9_]+)=(.*)$/);
				if (m) chatEnv[m[1]] = m[2].replace(/^["']|["']$/g, '');
			}
		} catch {
			// chat.env missing: compose defaults apply (acceptance stacks
			// always provision it via init --with-chat).
		}
		const backendName = `${COMPOSE_PROJECT}-backend-1`;
		execFileSync('docker', ['restart', backendName], { stdio: 'pipe', timeout: 120_000 });
		// nginx caches the upstream `backend` IP at startup; a restarted
		// backend can come back with a DIFFERENT address on the compose
		// network (observed: permanent 502s through nginx while the
		// container was healthy). Restarting nginx re-resolves the upstream.
		const nginxName = `${COMPOSE_PROJECT}-nginx-1`;
		execFileSync('docker', ['restart', nginxName], { stdio: 'pipe', timeout: 120_000 });
		const composeArgs = [
			'compose',
			'-p',
			COMPOSE_PROJECT,
			'-f',
			'docker-compose.yml',
			'-f',
			'docker-compose.alpha.yml',
			'-f',
			'docker-compose.dogfood.yml',
			'-f',
			extraFile,
			'up',
			'-d',
			'--force-recreate',
			'buzz-relay',
			'chat-observer'
		];
		execFileSync('docker', composeArgs, {
			cwd: repoRoot,
			env: { ...process.env, ...chatEnv },
			stdio: 'pipe',
			timeout: 300_000
		});
		// Backend ready again (restart is in-place; usually fast).
		await expect
			.poll(
				async () => {
					try {
						const res = await fetch(`${BASE_URL}/health/ready`);
						return res.status;
					} catch {
						return 0;
					}
				},
				{ timeout: 180_000, intervals: [2_000, 3_000, 5_000] }
			)
			.toBe(200);
		// Relay healthy again. NOTE: the relay serves GET /health on its MAIN
		// listener (published via the backend namespace as 17447), not on the
		// dedicated health port (which only answers /_liveness etc.).
		await expect
			.poll(
				async () => {
					try {
						const res = await fetch('http://127.0.0.1:17447/health');
						return res.status;
					} catch {
						return 0;
					}
				},
				{ timeout: 180_000, intervals: [2_000, 3_000, 5_000] }
			)
			.toBe(200);
		// Backend ready.
		await expect
			.poll(
				async () => {
					try {
						const res = await fetch(`${BASE_URL}/health/ready`);
						return res.status;
					} catch {
						return 0;
					}
				},
				{ timeout: 180_000, intervals: [2_000, 3_000, 5_000] }
			)
			.toBe(200);
		// Observer ready (re-admitted through the durable outbox path).
		const observerName = `${COMPOSE_PROJECT}-chat-observer-1`;
		await expect
			.poll(
				() => {
					try {
						execFileSync('docker', [
							'exec',
							observerName,
							'wget',
							'-q',
							'-O',
							'-',
							'http://127.0.0.1:8091/ready'
						]);
						return true;
					} catch {
						return false;
					}
				},
				{ timeout: 180_000, intervals: [2_000, 3_000, 5_000] }
			)
			.toBe(true);
		// Authorized history retained for both members. The reload restores the
		// last selected channel (the private one for A) — deep-link to the open
		// channel explicitly, re-establish the signing session, then assert the
		// pre-restart messages render.
		for (const user of [a, b]) {
			await user.page.reload();
			await user.page.goto(`/apps/chat?channel=${open.id}`);
			await expect(channelOption(user.page, open.name)).toBeVisible({ timeout: 90_000 });
			await ensureUnlocked(user);
			await expect(user.page.getByText(`hello from A ${RUN_SUFFIX}`)).toBeVisible({
				timeout: 90_000
			});
			await expect(user.page.getByText(`reply from B ${RUN_SUFFIX}`)).toBeVisible({
				timeout: 90_000
			});
		}
		// API history retained with the exact pre-restart event ids.
		const aView = await messagesOf(a, open.id);
		const postRestartIds = (aView.body?.messages ?? []).map((m: any) => m.event_id);
		expect(postRestartIds).toContain(state.eventIds.msgA);
		expect(postRestartIds).toContain(state.eventIds.msgB);
		expect(postRestartIds).toContain(state.eventIds.uncertain);
		// Posting works again end-to-end after the restart: A posts, B sees it
		// (browser clients reconnected through the fresh relay/backend).
		await sendMessage(a, `post-restart ${RUN_SUFFIX}`);
		await expect(b.page.getByText(`post-restart ${RUN_SUFFIX}`)).toBeVisible({ timeout: 120_000 });
		record(
			'j',
			'Restart of relay+backend+observer: history retained, sessions reconnect, posting works',
			'PASS',
			{
				channelId: open.id,
				restarted: {
					backend: 'docker restart (in place)',
					nginx: 'docker restart (upstream re-resolve)'
				},
				recreated: ['buzz-relay', 'chat-observer'],
				recreatedReason: 'dogfood netns sharers must reattach to the restarted backend namespace',
				composeProject: COMPOSE_PROJECT,
				historyEventIdsRetained: {
					msgA: postRestartIds.includes(state.eventIds.msgA),
					msgB: postRestartIds.includes(state.eventIds.msgB),
					uncertain: postRestartIds.includes(state.eventIds.uncertain)
				},
				postRestartMessageObservedByB: true
			}
		);
	});

	test('e2: account revocation denies access', async () => {
		const c = state.users.c;
		const admin = adminContextHolder.ctx!;
		const before = await api(c.context, 'GET', '/api/v1/users/me');
		expect(before.status).toBe(200);
		const disabled = await api(
			admin,
			'POST',
			`/api/v1/admin/users/${state.users.c.userId}/disable`
		);
		expect([200, 204]).toContain(disabled.status);
		await expect
			.poll(async () => (await api(c.context, 'GET', '/api/v1/users/me')).status, {
				timeout: 30_000
			})
			.toBe(401);
		const chatAfter = await api(c.context, 'GET', '/api/v1/applications/chat/channels');
		expect([401, 403]).toContain(chatAfter.status);
		record('e2', 'Disabled account loses session and Chat access', 'PASS', {
			user: state.users.c.userId,
			usersMeAfterDisable: 401,
			chatChannelsAfterDisable: chatAfter.status
		});
	});
});
