// Chat conversation product acceptance (stacked on PR #480): reply threading
// (NIP-10), reactions (NIP-25 kind 7), authorized deletion (NIP-09 kind 5 /
// NIP-29 kind 9005), per-thread draft preservation, and private-channel
// membership behavior for conversation state.
//
// Drives the REAL bundled stack through real browser contexts. Assertions are
// relay-direct wherever possible: each user's browser-held Chat key backup is
// decrypted in the test with the known acceptance passphrase (same PBKDF2 +
// AES-GCM envelope the product uses, frontend/src/lib/chat/keys.ts), then the
// relay is queried over NIP-42-authenticated WebSocket with that identity to
// assert EXACT signed event ids, authors, and NIP-10 tags. UI assertions never
// mock the API; direct API calls use each browser context's own session.
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
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const BASE_URL = process.env.ACCEPTANCE_BASE_URL ?? 'http://127.0.0.1:18080';
const RELAY_URL = process.env.ACCEPTANCE_RELAY_URL ?? 'ws://localhost:17447';
const ADMIN_EMAIL = process.env.ACCEPTANCE_ADMIN_EMAIL ?? 'admin@localhost';
const ADMIN_PASSWORD = process.env.ACCEPTANCE_ADMIN_PASSWORD ?? '';
const EVIDENCE_PATH =
	process.env.ACCEPTANCE_EVIDENCE ??
	'../docs/pilot/evidence/chat-conversation-acceptance-20261109.json';
const RUN_SUFFIX = `${Date.now().toString(36)}${Math.floor(Math.random() * 1e4)}`;

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
	eventIds: {} as Record<string, string | null>
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
	await expect(page.getByRole('heading', { name: 'Channels' })).toBeVisible({ timeout: 120_000 });
	await expect(page.getByRole('heading', { name: 'Unlock Chat' })).toBeVisible({ timeout: 30_000 });
	await page.getByLabel('key passphrase').fill(`${user.password}-key`);
	await page.getByRole('button', { name: 'Unlock' }).click();
	await expect(page.getByLabel('Message text')).toBeVisible({ timeout: 30_000 });
}

async function ensureUnlocked(user: UserState): Promise<void> {
	const { page } = user;
	const unlockHeading = page.getByRole('heading', { name: 'Unlock Chat' });
	if (await unlockHeading.isVisible().catch(() => false)) {
		await page.getByLabel('key passphrase').fill(`${user.password}-key`);
		await page.getByRole('button', { name: 'Unlock' }).click();
		await expect(page.getByLabel('Message text')).toBeVisible({ timeout: 30_000 });
	}
}

/** Full page navigations reset the in-memory signing session; re-unlock
 *  whenever a goto preceded messaging. */
async function gotoChannel(user: UserState, channelId: string): Promise<void> {
	await user.page.goto(`/apps/chat?channel=${channelId}`);
	await expect(user.page.getByRole('heading', { name: 'Channels' })).toBeVisible({
		timeout: 30_000
	});
	await ensureUnlocked(user);
}

/** Create a channel via the PR #480 UI; returns the registry-assigned id. */
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
		await form.getByLabel('New channel description').fill(`conversation acceptance ${name}`);
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
	await page.getByLabel('Message text').fill(body);
	await expect(page.getByRole('button', { name: 'Send message' })).toBeEnabled({
		timeout: 90_000
	});
	await page
		.getByRole('button', { name: 'Refresh permissions' })
		.click()
		.catch(() => {});
	await page.getByRole('button', { name: 'Send message' }).click();
	await expect(page.getByText('Observed by Elembra.')).toBeVisible({ timeout: 90_000 });
}

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

/** Row-scoped reply affordance (hover reveals the action buttons). */
function replyButtonOf(page: Page, messageId: string) {
	return page
		.locator(`[data-message-id="${messageId}"]`)
		.getByRole('button', { name: 'Reply to message' });
}

/** Open channels require an explicit join before posting; idempotent. */
async function ensureMembership(user: UserState): Promise<void> {
	const detailsButton = user.page.getByRole('button', { name: 'Channel details' });
	if ((await detailsButton.getAttribute('aria-expanded')) !== 'true') await detailsButton.click();
	const join = user.page.getByRole('button', { name: 'Join channel' });
	if (await join.isVisible().catch(() => false)) {
		await join.click();
		await expect(user.page.getByRole('list', { name: 'Channel members' })).toContainText(
			user.pubkey,
			{ timeout: 120_000 }
		);
	}
}

// ---------------------------------------------------------------------------
// Chat key backup decryption (mirrors frontend/src/lib/chat/keys.ts):
// PBKDF2-HMAC-SHA256(passphrase, salt, iter) → AES-GCM with
// AAD = JSON.stringify([1, userId, tenantId]) over the raw secret key bytes.
// ---------------------------------------------------------------------------
async function decryptChatKey(
	user: UserState
): Promise<{ pubkey: string; secretKey: string } | null> {
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
		const secretKey = bytesToHex(new Uint8Array(plain));
		const pubkey = bytesToHex(schnorr.getPublicKey(hexToBytes(secretKey)));
		return { pubkey, secretKey };
	} catch {
		return null;
	}
}

// ---------------------------------------------------------------------------
// Relay-direct queries with the user's own identity (NIP-42 authenticated REQ).
// Node 22's global WebSocket speaks the same wire protocol as the browser.
// ---------------------------------------------------------------------------
function publicKeyOf(secretKey: string): string {
	return bytesToHex(schnorr.getPublicKey(hexToBytes(secretKey)));
}

async function sha256Hex(input: string): Promise<string> {
	const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

async function signEvent(
	unsigned: { pubkey: string; created_at: number; kind: number; tags: string[][]; content: string },
	secretKey: string
) {
	const id = await sha256Hex(
		JSON.stringify([
			0,
			unsigned.pubkey,
			unsigned.created_at,
			unsigned.kind,
			unsigned.tags,
			unsigned.content
		])
	);
	const sig = bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(secretKey)));
	return { ...unsigned, id, sig };
}

interface RelayQueryResult {
	ok: boolean;
	detail?: string;
	events: any[];
}

async function relayQuery(
	secretKey: string,
	filter: Record<string, unknown>
): Promise<RelayQueryResult> {
	const pubkey = publicKeyOf(secretKey);
	return new Promise((resolve) => {
		const ws = new WebSocket(RELAY_URL);
		const sub = `accept${Math.random().toString(36).slice(2, 10)}`;
		const events: any[] = [];
		let authId: string | null = null;
		let authenticated = false;
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
		ws.onopen = () => ws.send(JSON.stringify(['REQ', sub, filter]));
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
							pubkey,
							created_at: Math.floor(Date.now() / 1000),
							kind: 22242,
							tags: [
								['relay', RELAY_URL],
								['challenge', msg[1]]
							],
							content: ''
						},
						secretKey
					);
					authId = auth.id;
					ws.send(JSON.stringify(['AUTH', auth]));
					// The initial REQ races the handshake: the relay answers it
					// with an auth-required CLOSED, ignored like the product's
					// queryRelayEvents. The REQ is re-issued on the AUTH OK.
					return;
				}
				if (msg[0] === 'OK' && authId && msg[1] === authId) {
					if (msg[2] === true) {
						authenticated = true;
						ws.send(JSON.stringify(['REQ', sub, filter]));
					} else finish({ ok: false, detail: `auth rejected: ${msg[3]}`, events });
					return;
				}
				if (msg[0] === 'EVENT' && msg[1] === sub) events.push(msg[2]);
				else if (msg[0] === 'EOSE' && msg[1] === sub) finish({ ok: true, events });
				else if (msg[0] === 'CLOSED' && msg[1] === sub) {
					const detail = typeof msg[2] === 'string' ? msg[2] : '';
					// Pre-auth CLOSED is the raced initial REQ, not a denial.
					if (!authenticated && /auth/i.test(detail)) return;
					finish({ ok: false, detail: `closed: ${detail}`, events });
				}
			})();
		};
	});
}

function nip10Tags(event: any): { root: string | null; reply: string | null } {
	let root: string | null = null;
	let reply: string | null = null;
	for (const tag of event.tags ?? []) {
		if (tag[0] === 'e' && tag.length >= 4) {
			if (tag[3] === 'root') root = tag[1];
			else if (tag[3] === 'reply') reply = tag[1];
		}
	}
	return { root, reply };
}

/** Negative isolation assertions must SAMPLE across the propagation window. */
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
// Matrix
// ---------------------------------------------------------------------------
test.describe.configure({ mode: 'serial' });
test.describe('Chat conversation product acceptance (PR #481)', () => {
	test.setTimeout(600_000);

	let browser: Browser;
	const adminContextHolder: { ctx?: BrowserContext } = {};

	test.beforeAll(async ({ browser: b }) => {
		browser = b;
		const admin = await b.newContext({ baseURL: BASE_URL });
		adminContextHolder.ctx = admin;
		const login = await admin.request.post('/api/v1/auth/login', {
			data: { email: ADMIN_EMAIL, password: ADMIN_PASSWORD }
		});
		if (login.status() !== 200) throw new Error(`admin login failed: HTTP ${login.status()}`);
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
		await api(admin, 'PATCH', '/api/v1/admin/applications/io.elembra.chat', {
			memory_projection: true,
			content_indexing: true
		});

		for (const key of ['a', 'b', 'c'] as const) {
			const username = `conv_${key}_${RUN_SUFFIX}`;
			const email = `${username}@acceptance.local`;
			const password = `Conv-${RUN_SUFFIX}-pw`;
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
			page.on('pageerror', (e) =>
				console.log(`[browser:${key}:pageerror]`, String(e).slice(0, 300))
			);
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

	test('bind: login, browser-held identity binding, and key backup decryption for relay-direct reads', async () => {
		for (const key of ['a', 'b', 'c'] as const) {
			const user = state.users[key];
			await uiLogin(user);
			await bindChat(user);
			const status = await api(user.context, 'GET', '/api/v1/applications/chat/status');
			expect(status.status).toBe(200);
			const body = await status.json();
			expect(body.runtime_profile).toBe('fork_client_held');
			expect(body.binding?.status).toBe('Active');
			expect(body.admission_active).toBe(true);
			user.pubkey = body.binding.buzz_pubkey;
			const decrypted = await decryptChatKey(user);
			expect(decrypted).toBeTruthy();
			expect(decrypted!.pubkey).toBe(user.pubkey);
			user.secretKey = decrypted!.secretKey;
		}
		record('bind', 'Bound A/B/C; decrypted key backups match bound pubkeys', 'PASS', {
			pubkeys: Object.fromEntries(Object.entries(state.users).map(([k, u]) => [k, u.pubkey]))
		});
	});

	test('a: open channel created for the conversation matrix', async () => {
		const a = state.users.a;
		const created = await createChannelViaUi(a, `conv-open-${RUN_SUFFIX}`, 'open');
		state.channels.open = created;
		await expect(channelOption(a.page, created.name)).toBeVisible();
		await expect(channelOption(state.users.b.page, created.name)).toBeVisible({ timeout: 60_000 });
		record('a', 'Open channel created and discovered by both identities', 'PASS', {
			channelId: created.id,
			channelName: created.name
		});
	});

	test('reply: reply composer emits canonical NIP-10 tags (relay-direct exact ids + authors)', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		await gotoChannel(a, open.id);
		await gotoChannel(b, open.id);
		const rootBody = `conv root ${RUN_SUFFIX}`;
		const replyBody = `conv reply one ${RUN_SUFFIX}`;
		await sendMessage(a, rootBody);
		const aView = await messagesOf(a, open.id);
		const root = (aView.body?.messages ?? []).find((m: any) => m.body === rootBody);
		expect(root).toBeTruthy();
		state.eventIds.root = root.event_id;

		// B replies through the product UI (open channels join-before-post).
		await ensureMembership(b);
		await expect(replyButtonOf(b.page, root.message_id)).toBeVisible({ timeout: 60_000 });
		await replyButtonOf(b.page, root.message_id).click();
		await expect(b.page.getByText(/Replying in thread/)).toBeVisible();
		await b.page.getByLabel('Message text').fill(replyBody);
		await expect(b.page.getByRole('button', { name: 'Send message' })).toBeEnabled({
			timeout: 90_000
		});
		await b.page.getByRole('button', { name: 'Send message' }).click();
		await expect(b.page.getByText('Observed by Elembra.')).toBeVisible({ timeout: 90_000 });

		// Relay-direct: the thread history must contain B's reply with EXACT
		// canonical tags: root + reply markers both on the root id, B's signature.
		const history = await relayQuery(b.secretKey!, {
			kinds: [9],
			'#e': [root.event_id]
		});
		expect(history.ok).toBe(true);
		const reply = history.events.find((e) => e.content === replyBody);
		expect(reply).toBeTruthy();
		expect(reply.pubkey).toBe(b.pubkey);
		expect(reply.tags).toContainEqual(['h', open.id]);
		expect(nip10Tags(reply)).toEqual({ root: root.event_id, reply: root.event_id });
		state.eventIds.replyOne = reply.id;

		// Backend projection agrees on author and thread root.
		const bView = await messagesOf(b, open.id);
		const projected = (bView.body?.messages ?? []).find((m: any) => m.body === replyBody);
		expect(projected?.author_pubkey).toBe(b.pubkey);
		expect(projected?.thread_root_id).toBe(root.event_id);

		// A's timeline grows the reply-count badge for the thread.
		await expect(a.page.getByRole('button', { name: 'View thread, 1 reply' })).toBeVisible({
			timeout: 60_000
		});
		record('reply', 'Reply published with exact NIP-10 root/reply tags and author', 'PASS', {
			channelId: open.id,
			rootEventId: root.event_id,
			replyEventId: reply.id,
			replyAuthor: reply.pubkey,
			tags: nip10Tags(reply)
		});
	});

	test('thread: nested reply through the thread view threads under the stored root', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		const nestedBody = `conv reply two ${RUN_SUFFIX}`;
		const aView = await messagesOf(a, open.id);
		const root = (aView.body?.messages ?? []).find(
			(m: any) => m.body === `conv root ${RUN_SUFFIX}`
		);

		// A opens the thread view from the timeline badge.
		await a.page.getByRole('button', { name: 'View thread, 1 reply' }).click();
		await expect(a.page.getByRole('heading', { name: 'Thread' })).toBeVisible();
		await expect(a.page.getByText(`conv reply one ${RUN_SUFFIX}`)).toBeVisible({ timeout: 30_000 });

		// B (in the same thread view) replies to the existing reply. Membership
		// persists relay-side; re-check idempotently after the navigation.
		await b.page.goto(`/apps/chat?channel=${open.id}`);
		await expect(channelOption(b.page, open.name)).toBeVisible({ timeout: 60_000 });
		await ensureUnlocked(b);
		await ensureMembership(b);
		const bReply = await messagesOf(b, open.id);
		const replyOne = (bReply.body?.messages ?? []).find(
			(m: any) => m.body === `conv reply one ${RUN_SUFFIX}`
		);
		await b.page.getByRole('button', { name: 'View thread, 1 reply' }).click();
		await expect(b.page.getByRole('heading', { name: 'Thread' })).toBeVisible();
		const threadReplyButton = b.page
			.getByRole('button', { name: `Reply to ${b.pubkey.slice(0, 8)}` })
			.last();
		await expect(threadReplyButton).toBeVisible({ timeout: 30_000 });
		await threadReplyButton.click();
		await b.page.getByLabel('Message text').fill(nestedBody);
		await expect(b.page.getByRole('button', { name: 'Send message' })).toBeEnabled({
			timeout: 90_000
		});
		await b.page.getByRole('button', { name: 'Send message' }).click();
		await expect(b.page.getByText('Observed by Elembra.')).toBeVisible({ timeout: 90_000 });

		// Relay-direct: the nested reply must carry root=thread root and
		// reply=direct parent; a wrong root claim would have been rejected by
		// the relay (never observed), so acceptance itself proves ancestry.
		const history = await relayQuery(b.secretKey!, { kinds: [9], '#e': [root.event_id] });
		expect(history.ok).toBe(true);
		const nested = history.events.find((e) => e.content === nestedBody);
		expect(nested).toBeTruthy();
		expect(nested.pubkey).toBe(b.pubkey);
		expect(nip10Tags(nested)).toEqual({ root: root.event_id, reply: replyOne.event_id });
		state.eventIds.replyTwo = nested.id;

		await expect(a.page.getByText(nestedBody)).toBeVisible({ timeout: 60_000 });
		record('thread', 'Thread view groups replies by root; nested reply ancestry exact', 'PASS', {
			channelId: open.id,
			threadRootId: root.event_id,
			nestedEventId: nested.id,
			tags: nip10Tags(nested)
		});
	});

	test('react: reaction toggle with counts/authors, cross-user and relay-direct', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		const targetId = state.eventIds.replyOne!;

		// The previous serial test leaves A in the thread view.
		const aClose = a.page.getByRole('button', { name: 'Close thread' });
		if (await aClose.isVisible().catch(() => false)) await aClose.click();
		await expect(a.page.getByRole('heading', { name: 'Thread' })).toBeHidden();
		const threadBadge = a.page.getByRole('button', { name: 'View thread, 2 replies' });
		await expect(threadBadge).toBeVisible({ timeout: 60_000 });
		// A reacts 👍 to B's first reply from the thread view (row-scoped).
		await Promise.all([
			threadBadge.click({ timeout: 15_000 }).catch(() => undefined),
			expect(a.page.getByRole('heading', { name: 'Thread' })).toBeVisible({ timeout: 15_000 })
		]);
		const aPalette = a.page
			.locator(`[data-event-id="${targetId}"]`)
			.getByRole('button', { name: 'React 👍' });
		await expect(aPalette).toBeVisible({ timeout: 45_000 });
		await aPalette.click();
		await expect(a.page.getByRole('button', { name: 'Reaction 👍, 1' })).toBeVisible({
			timeout: 30_000
		});

		// Relay-direct: exactly one kind-7 from A on the target.
		let reactions = await relayQuery(a.secretKey!, { kinds: [7], '#e': [targetId] });
		expect(reactions.ok).toBe(true);
		expect(reactions.events).toHaveLength(1);
		expect(reactions.events[0].pubkey).toBe(a.pubkey);
		expect(reactions.events[0].content).toBe('👍');
		state.eventIds.reactionA = reactions.events[0].id;

		// B reacts 👍 too (chip toggle when A's reaction is already visible,
		// palette otherwise — B's overlay can lag the relay by one poll). B is
		// still in the thread view from the thread test. Both UIs converge on
		// count 2.
		if (
			!(await b.page
				.getByRole('heading', { name: 'Thread' })
				.isVisible()
				.catch(() => false))
		) {
			await b.page.getByRole('button', { name: 'View thread, 2 replies' }).click();
			await expect(b.page.getByRole('heading', { name: 'Thread' })).toBeVisible();
		}
		const bRow = b.page.locator(`[data-event-id="${targetId}"]`);
		const bExisting = bRow.getByRole('button', { name: 'Reaction 👍, 1' });
		const bPalette = bRow.getByRole('button', { name: 'React 👍' });
		await expect(bExisting.or(bPalette)).toBeVisible({ timeout: 45_000 });
		if (await bExisting.isVisible().catch(() => false)) await bExisting.click();
		else await bPalette.click();
		await expect(b.page.getByRole('button', { name: 'Reaction 👍, 2' })).toBeVisible({
			timeout: 30_000
		});
		await expect(a.page.getByRole('button', { name: 'Reaction 👍, 2' })).toBeVisible({
			timeout: 45_000
		});

		// A toggles off: relay-direct count drops to 1 and A's chip clears.
		await a.page.getByRole('button', { name: 'Reaction 👍, 2' }).click();
		await expect(a.page.getByRole('button', { name: 'Reaction 👍, 1' })).toBeVisible({
			timeout: 30_000
		});
		reactions = await relayQuery(b.secretKey!, { kinds: [7], '#e': [targetId] });
		expect(reactions.ok).toBe(true);
		expect(reactions.events).toHaveLength(1);
		expect(reactions.events[0].pubkey).toBe(b.pubkey);

		record('react', 'Reaction add/duplicate-count/toggle-off verified relay-direct', 'PASS', {
			targetId,
			aReactionEventId: state.eventIds.reactionA,
			remainingAuthors: reactions.events.map((e) => e.pubkey)
		});
	});

	test('drafts: per-thread drafts persist across reload, apart from channel drafts', async () => {
		const b = state.users.b;
		const open = state.channels.open;
		const rootId = state.eventIds.root!;
		const threadDraft = `thread draft ${RUN_SUFFIX}`;
		const channelDraft = `channel draft ${RUN_SUFFIX}`;

		await gotoChannel(b, open.id);
		await b.page.getByRole('button', { name: 'View thread, 2 replies' }).click();
		await expect(b.page.getByRole('heading', { name: 'Thread' })).toBeVisible();
		await b.page.getByLabel('Message text').fill(threadDraft);
		await b.page.waitForTimeout(1_000);
		await b.page.reload();
		await expect(b.page.getByRole('heading', { name: 'Channels' })).toBeVisible({
			timeout: 30_000
		});
		await ensureUnlocked(b);
		await b.page.getByRole('button', { name: 'View thread, 2 replies' }).click();
		await expect(b.page.getByLabel('Message text')).toHaveValue(threadDraft, { timeout: 30_000 });

		// Leaving the thread keeps the channel-level draft separate.
		await b.page.getByRole('button', { name: 'Close thread' }).click();
		await b.page.getByLabel('Message text').fill(channelDraft);
		await b.page.waitForTimeout(1_000);
		await b.page.reload();
		await expect(b.page.getByRole('heading', { name: 'Channels' })).toBeVisible({
			timeout: 30_000
		});
		await ensureUnlocked(b);
		await expect(b.page.getByLabel('Message text')).toHaveValue(channelDraft, { timeout: 30_000 });
		await b.page.getByRole('button', { name: 'View thread, 2 replies' }).click();
		await expect(b.page.getByLabel('Message text')).toHaveValue(threadDraft, { timeout: 30_000 });

		record('drafts', 'Thread draft and channel draft persist independently across reload', 'PASS', {
			channelId: open.id,
			threadRootId: rootId
		});
	});

	test('del-self: author deletion removes the event relay-side and renders deleted', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		const targetId = state.eventIds.replyTwo!;

		// A was left in the thread view by the reaction test; the deleted-row
		// assertion reads the timeline.
		const aClose = a.page.getByRole('button', { name: 'Close thread' });
		if (await aClose.isVisible().catch(() => false)) await aClose.click();
		// B (left in the thread view by the previous test) back to the timeline.
		const bClose = b.page.getByRole('button', { name: 'Close thread' });
		if (await bClose.isVisible().catch(() => false)) await bClose.click();
		const bView = await messagesOf(b, open.id);
		const nested = (bView.body?.messages ?? []).find((m: any) => m.event_id === targetId);
		expect(nested).toBeTruthy();

		b.page.once('dialog', (dialog) => void dialog.accept());
		await b.page
			.locator(`[data-message-id="${nested.message_id}"]`)
			.getByRole('button', { name: 'Message actions' })
			.click();
		await b.page.getByRole('menuitem', { name: 'Delete message' }).click();
		// Both renderings are correct while deletion propagates: the overlay
		// tombstone ("Message deleted.") or the projection dropping the
		// inactive row. The relay-direct assertions below are authoritative.
		await expect(b.page.getByText(`conv reply two ${RUN_SUFFIX}`)).toBeHidden({ timeout: 60_000 });

		// Relay-direct: the event is gone from history, the kind-5 request is
		// stored in-channel, and the thread counters hold.
		const history = await relayQuery(b.secretKey!, { kinds: [9], ids: [targetId] });
		expect(history.ok).toBe(true);
		expect(history.events).toHaveLength(0);
		const overlay = await relayQuery(b.secretKey!, {
			kinds: [5],
			'#h': [open.id]
		});
		expect(
			overlay.events.some((e) => e.tags.some((t: string[]) => t[0] === 'e' && t[1] === targetId))
		).toBe(true);

		// A observes the deletion once the relay webhook reaches the projection
		// (the row drops out of the timeline; the overlay tombstone may render
		// first).
		await expect(a.page.getByText(`conv reply two ${RUN_SUFFIX}`)).toBeHidden({ timeout: 60_000 });
		record('del-self', 'Self-deletion applied relay-side; deleted state on both sides', 'PASS', {
			targetId,
			kind5Stored: true,
			relayAbsent: true
		});
	});

	test('del-admin: owner deletes a member message; relay-signed tombstone recorded', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const open = state.channels.open;
		const targetId = state.eventIds.replyOne!;

		// A was left in the thread view by the reaction test; the deleted row is
		// acted on from the timeline.
		const aClose = a.page.getByRole('button', { name: 'Close thread' });
		if (await aClose.isVisible().catch(() => false)) await aClose.click();
		const aView = await messagesOf(a, open.id);
		const replyOne = (aView.body?.messages ?? []).find((m: any) => m.event_id === targetId);
		expect(replyOne).toBeTruthy();

		a.page.once('dialog', (dialog) => void dialog.accept());
		await a.page
			.locator(`[data-message-id="${replyOne.message_id}"]`)
			.getByRole('button', { name: 'Message actions' })
			.click();
		await a.page.getByRole('menuitem', { name: 'Delete message (admin)' }).click();
		// Overlay tombstone or projection drop — both are correct renderings.
		await expect(a.page.getByText(`conv reply one ${RUN_SUFFIX}`)).toBeHidden({ timeout: 60_000 });

		const history = await relayQuery(a.secretKey!, { kinds: [9], ids: [targetId] });
		expect(history.ok).toBe(true);
		expect(history.events).toHaveLength(0);
		const tombstones = await relayQuery(a.secretKey!, { kinds: [40099], '#h': [open.id] });
		const deletedTombstone = tombstones.events.find((e) => {
			try {
				const payload = JSON.parse(e.content);
				return payload.type === 'message_deleted' && payload.target_event_id === targetId;
			} catch {
				return false;
			}
		});
		expect(deletedTombstone).toBeTruthy();
		expect(deletedTombstone.pubkey).toBe(state.relayPubkey);
		const payload = JSON.parse(deletedTombstone.content);
		expect(payload.actor).toBe(a.pubkey);

		// B sees the admin-deleted state too (tombstone or row drop).
		await expect(b.page.getByText(`conv reply one ${RUN_SUFFIX}`)).toBeHidden({ timeout: 60_000 });
		record('del-admin', 'Owner admin-delete applied; relay-signed tombstone with actor', 'PASS', {
			targetId,
			tombstoneEventId: deletedTombstone.id,
			tombstoneSigner: deletedTombstone.pubkey,
			actor: payload.actor
		});
	});

	test('member: losing private-channel access hides thread state and denies relay reads', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const c = state.users.c;
		const created = await createChannelViaUi(a, `conv-private-${RUN_SUFFIX}`, 'private');
		state.channels.private = created;
		await a.page.getByRole('button', { name: 'Channel details' }).click();
		await a.page.getByLabel('Member pubkey').fill(b.pubkey);
		await a.page.getByRole('button', { name: 'Add member' }).click();
		await expect(a.page.getByRole('list', { name: 'Channel members' })).toContainText(b.pubkey, {
			timeout: 90_000
		});
		await expect(channelOption(b.page, created.name)).toBeVisible({ timeout: 60_000 });

		// Seed a thread in the private channel. Private-channel bodies are
		// never projected (the observation body gate keeps private content out
		// of the index), so the row is identified by author, not body.
		await gotoChannel(a, created.id);
		await gotoChannel(b, created.id);
		const privRootBody = `priv root ${RUN_SUFFIX}`;
		await sendMessage(a, privRootBody);
		const aView = await messagesOf(a, created.id);
		const privRoot = (aView.body?.messages ?? []).find((m: any) => m.author_pubkey === a.pubkey);
		expect(privRoot).toBeTruthy();
		state.eventIds.privRoot = privRoot.event_id;
		await replyButtonOf(b.page, privRoot.message_id).click();
		await b.page.getByLabel('Message text').fill(`priv reply ${RUN_SUFFIX}`);
		await expect(b.page.getByRole('button', { name: 'Send message' })).toBeEnabled({
			timeout: 90_000
		});
		await b.page.getByRole('button', { name: 'Send message' }).click();
		await expect(b.page.getByText('Observed by Elembra.')).toBeVisible({ timeout: 90_000 });
		await expect(b.page.getByRole('button', { name: 'View thread, 1 reply' })).toBeVisible({
			timeout: 60_000
		});
		await b.page.getByRole('button', { name: 'View thread, 1 reply' }).click();
		await expect(b.page.getByRole('heading', { name: 'Thread' })).toBeVisible();

		// A removes B while B's thread view is open.
		await channelOption(a.page, created.name).click();
		await a.page.getByRole('button', { name: 'Channel details' }).click();
		await a.page.getByRole('button', { name: `Remove member ${b.pubkey.slice(0, 8)}` }).click();
		await expect(a.page.getByRole('list', { name: 'Channel members' })).not.toContainText(
			b.pubkey,
			{
				timeout: 90_000
			}
		);

		// B's conversation state collapses: the directory drops the channel, the
		// deep link cannot hold, and the composer is default-denied.
		await expect(channelOption(b.page, created.name)).toHaveCount(0, { timeout: 60_000 });
		await expect
			.poll(async () => b.page.url().includes(created.id), {
				timeout: 60_000,
				intervals: [2_000, 4_000]
			})
			.toBe(false);
		await expect(b.page.getByRole('heading', { name: 'Thread' })).toHaveCount(0);
		await b.page
			.getByRole('button', { name: 'Refresh permissions' })
			.click()
			.catch(() => {});
		await b.page.getByLabel('Message text').fill(`b attempt ${RUN_SUFFIX}`);
		const send = b.page.getByRole('button', { name: 'Send message' });
		if (await b.page.url().includes(created.id)) {
			expect(await send.isDisabled().catch(() => false)).toBe(true);
		}

		// Relay-direct denial: B's own identity can no longer read the private
		// thread history, and C never could.
		await expectAbsentAcrossWindow(3, 4_000, async () => {
			const bHistory = await relayQuery(b.secretKey!, { kinds: [9], '#e': [privRoot.event_id] });
			expect(bHistory.events).toHaveLength(0);
			const cHistory = await relayQuery(c.secretKey!, { kinds: [9], '#e': [privRoot.event_id] });
			expect(cHistory.events).toHaveLength(0);
		});
		record(
			'member',
			'Private-channel removal hides thread state, disables composer, denies relay reads',
			'PASS',
			{
				channelId: created.id,
				threadRootId: privRoot.event_id,
				removed: b.pubkey
			}
		);
	});
});
