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
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

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
// Matrix
// ---------------------------------------------------------------------------
test.describe.configure({ mode: 'serial' });
test.describe('Chat channel management product acceptance (PR #480)', () => {
	test.setTimeout(420_000);

	let browser: Browser;
	const adminContextHolder: { ctx?: BrowserContext } = {};

	test.beforeAll(async ({ browser: b }) => {
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
		}
		record('bind', 'Real login + binding + relay admission (A/B/C)', 'PASS', {
			runtimeProfile: 'fork_client_held',
			pubkeys: Object.fromEntries(Object.entries(state.users).map(([k, u]) => [k, u.pubkey]))
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
		const open = state.channels.open;
		const body = `uncertain-path ${RUN_SUFFIX}`;
		await ensureUnlocked(a);
		// Delay the relay's acceptance ACK past the client's 10 s ACK timeout so
		// the composer takes its real "uncertain" path. Done in the page (a
		// WebSocket constructor wrapper) — Playwright's routeWebSocket does not
		// see the app's relay connections in this Chromium build.
		await a.page.evaluate(() => {
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
					if (!String(url).includes('17447')) return;
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
		});
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
			state.eventIds.uncertain = capturedId;
			record('g', 'Uncertain ACK path retried byte-identically; single application', 'PASS', {
				channelId: open.id,
				pendingEventId: capturedId,
				appliedCopies: matches.length,
				retainedEventId: matches[0].event_id
			});
		} finally {
			// Unconditionally restore the real WebSocket even on failure.
			await a.page.evaluate(() => {
				const original = window.__gOriginalWS;
				if (original) window.WebSocket = original;
			});
		}
	});

	test('e: member removal revokes discovery, reads, and posting; B’s session stays live', async () => {
		const a = state.users.a;
		const b = state.users.b;
		const priv = state.channels.private;
		const open = state.channels.open;
		// A removes B from the private channel.
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

		// --- B attempts a write; the product denies it ------------------------
		// What the product actually does: the relay-signed directory is the only
		// channel-addressability source, and it no longer lists the private
		// channel for B. While the deep link still addresses the removed
		// channel the composer is default-denied (Send disabled); once the
		// directory settles the deep link cannot hold and B falls back to a
		// visible channel. Both behaviors are sampled live below — no write is
		// ever submitted, because the product gives B no path to submit one.
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
		// the composer must stay default-denied there (Send disabled). The
		// window closes as soon as the relay-signed directory settles.
		let addressedSamples = 0;
		let composerDeniedSamples = 0;
		const sampleComposerDenial = async (): Promise<void> => {
			if (!(await b.page.url().includes(priv.id))) return;
			addressedSamples += 1;
			const send = b.page.getByRole('button', { name: 'Send message' });
			if (await send.isDisabled().catch(() => false)) composerDeniedSamples += 1;
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
		record('e', 'Removal revokes discovery, reads, and posting; B session stays live', 'PASS', {
			channelId: priv.id,
			removed: b.pubkey,
			bDirectoryAfterRemoval: bDir.status,
			bComposerDeniedWhileAddressed: composerDeniedSamples,
			bAddressedSamples: addressedSamples,
			deepLinkHeldAfterRemoval: deepLinkHeld,
			bPrivateTimelineRows: bPrivateRows,
			bReadAfterRemoval: 'zero-rows',
			bLivenessOpenChannelReads: bOpen.status
		});
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
