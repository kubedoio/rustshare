// Client-held Buzz key custody: passphrase-encrypted at rest (WebCrypto
// PBKDF2 + AES-GCM). The raw key never leaves the browser; export/import is
// the only recovery path (ADR-0034: no silent server custody).
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';
import { publicKeyOf } from './nostr';

const LEGACY_STORAGE_KEY = 'elembra.chat.key.v1';
/** PBKDF2-HMAC-SHA256 iterations for NEW envelopes (OWASP-recommended for
 * offline-crackable localStorage blobs). Old envelopes record their own
 * iterations in `iter` and keep working. */
const PBKDF2_ITERATIONS = 600_000;
const LEGACY_PBKDF2_ITERATIONS = 100_000;

/** The user the key vault is scoped to (set by the auth store on
 * login/bootstrap/logout) so a second user on the same browser never inherits
 * the previous user's envelope. */
let storageScope: string | null = null;
let tenantScope: string | null = null;
let scopeGeneration = 0;
const attempts = new Map<string, { failures: number; retryAt: number; busy: boolean }>();

export class ChatKeyRetryError extends Error {}

/** Shared by unlock and both import entry points; deliberately session-local.
 * This cannot slow offline attacks against a stolen encrypted backup. */
async function authenticate<T>(operation: () => Promise<T>): Promise<T> {
	const scope = storageScope;
	const generation = scopeGeneration;
	if (scope === null) throw new Error('no chat key scope — sign in first');
	const attemptScope = `${tenantScope ?? ''}:${scope}`;
	let state = attempts.get(attemptScope);
	if (!state) {
		state = { failures: 0, retryAt: 0, busy: false };
		attempts.set(attemptScope, state);
	}
	if (state.busy) throw new ChatKeyRetryError('A key verification is already in progress.');
	if (Date.now() < state.retryAt) {
		throw new ChatKeyRetryError(
			`Try again in ${Math.ceil((state.retryAt - Date.now()) / 1000)} seconds.`
		);
	}
	state.busy = true;
	try {
		const result = await operation();
		assertChatKeySession(generation);
		state.failures = 0;
		state.retryAt = 0;
		return result;
	} catch (error) {
		if (generation === scopeGeneration) {
			state.failures = Math.min(state.failures + 1, 9);
			if (state.failures >= 3)
				state.retryAt = Date.now() + Math.min(60_000, 1000 * 2 ** (state.failures - 3));
			if (state.retryAt > Date.now()) {
				throw new ChatKeyRetryError(
					`${error instanceof Error ? error.message : 'Key verification failed.'} Try again in ${Math.ceil((state.retryAt - Date.now()) / 1000)} seconds.`
				);
			}
		}
		throw error;
	} finally {
		state.busy = false;
	}
}

export function chatKeySession(): number {
	return scopeGeneration;
}

export function assertChatKeySession(generation: number): void {
	if (generation !== scopeGeneration || storageScope === null) {
		throw new Error('Chat account changed. Try again.');
	}
}

/** Scope the key vault to `userId`, migrating a pre-scoping legacy envelope
 * on first use so existing users keep their key. `null` (logged out) hides
 * the vault entirely. */
export function setChatKeyUser(userId: string | null, tenantId: string | null = null): void {
	if (storageScope === userId && tenantScope === tenantId) return;
	scopeGeneration += 1;
	storageScope = userId;
	tenantScope = tenantId;
	if (userId === null) {
		attempts.clear();
		return;
	}
	// This runs on every login/bootstrap; a storage failure (private mode,
	// quota) must never break auth. Degrade to "no scope" — Chat then reports
	// no local key, which is the correct optional-feature behavior.
	try {
		const scoped = `${LEGACY_STORAGE_KEY}.${userId}`;
		if (localStorage.getItem(scoped) !== null) return;
		const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
		if (legacy !== null) {
			localStorage.setItem(scoped, legacy);
			localStorage.removeItem(LEGACY_STORAGE_KEY);
		}
	} catch {
		storageScope = null;
	}
}

function storageKey(): string | null {
	return storageScope === null ? null : `${LEGACY_STORAGE_KEY}.${storageScope}`;
}

export interface EncryptedChatKey {
	v: 1;
	salt: string; // hex
	iv: string; // hex
	ciphertext: string; // hex
	iter?: number; // PBKDF2 iterations used for this envelope (defaults to legacy 100k)
	pubkey?: string;
	owner?: string;
	tenant?: string;
	aad?: 1;
}

function parseEnvelope(json: string): EncryptedChatKey {
	if (json.length > 4096) throw new Error('unsupported chat key format');
	let value;
	try {
		value = JSON.parse(json);
	} catch {
		throw new Error('invalid backup format');
	}
	const hex = (input: unknown, length: number) =>
		typeof input === 'string' && new RegExp(`^[0-9a-f]{${length}}$`, 'i').test(input);
	if (
		!value ||
		value.v !== 1 ||
		!hex(value.salt, 32) ||
		!hex(value.iv, 24) ||
		!hex(value.ciphertext, 96) ||
		(value.iter !== undefined &&
			(!Number.isSafeInteger(value.iter) || value.iter < 100_000 || value.iter > 600_000)) ||
		(value.pubkey !== undefined && !hex(value.pubkey, 64)) ||
		(value.owner !== undefined && typeof value.owner !== 'string') ||
		(value.tenant !== undefined && typeof value.tenant !== 'string') ||
		(value.aad !== undefined && value.aad !== 1) ||
		(value.aad === 1 && typeof value.owner !== 'string')
	) {
		throw new Error('unsupported chat key format');
	}
	if (value.owner !== undefined && value.owner !== storageScope)
		throw new Error('That backup belongs to another account.');
	if (value.tenant !== undefined && value.tenant !== tenantScope)
		throw new Error('That backup belongs to another workspace.');
	return value;
}

export function hasChatKey(): boolean {
	try {
		const key = storageKey();
		return key !== null && localStorage.getItem(key) !== null;
	} catch {
		return false;
	}
}

export function storedKeyPubkey(): string | null {
	try {
		const key = storageKey();
		if (key === null) return null;
		const raw = localStorage.getItem(key);
		if (!raw) return null;
		const envelope: { pubkey?: string } = JSON.parse(raw);
		return envelope.pubkey ?? null;
	} catch {
		return null;
	}
}

async function deriveKey(
	passphrase: string,
	salt: Uint8Array<ArrayBuffer>,
	iterations: number
): Promise<CryptoKey> {
	const material = await crypto.subtle.importKey(
		'raw',
		new TextEncoder().encode(passphrase),
		'PBKDF2',
		false,
		['deriveKey']
	);
	return crypto.subtle.deriveKey(
		{ name: 'PBKDF2', salt, iterations, hash: 'SHA-256' },
		material,
		{ name: 'AES-GCM', length: 256 },
		false,
		['encrypt', 'decrypt']
	);
}

export async function saveChatKey(
	secretKey: string,
	pubkey: string,
	passphrase: string,
	assertCurrent?: () => void
): Promise<void> {
	const key = storageKey();
	if (key === null) throw new Error('no chat key scope — sign in first');
	const generation = scopeGeneration;
	const previous = localStorage.getItem(key);
	if (!passphrase) throw new Error('Choose a key passphrase.');
	if (publicKeyOf(secretKey) !== pubkey) throw new Error('Chat public key mismatch.');
	const salt = crypto.getRandomValues(new Uint8Array(16));
	const iv = crypto.getRandomValues(new Uint8Array(12));
	const derived = await deriveKey(passphrase, salt, PBKDF2_ITERATIONS);
	const ciphertext = await crypto.subtle.encrypt(
		{ name: 'AES-GCM', iv, additionalData: identityData(storageScope!, tenantScope) },
		derived,
		hexToBytes(secretKey)
	);
	const envelope: EncryptedChatKey = {
		v: 1,
		salt: bytesToHex(salt),
		iv: bytesToHex(iv),
		ciphertext: bytesToHex(new Uint8Array(ciphertext)),
		iter: PBKDF2_ITERATIONS
	};
	assertChatKeySession(generation);
	assertCurrent?.();
	if (localStorage.getItem(key) !== previous)
		throw new Error('Chat key changed during verification. Try again.');
	try {
		localStorage.setItem(
			key,
			JSON.stringify({
				...envelope,
				aad: 1,
				pubkey,
				owner: storageScope,
				...(tenantScope === null ? {} : { tenant: tenantScope })
			})
		);
	} catch {
		throw new Error('could not store the chat key (storage unavailable)');
	}
}

export async function loadChatKey(passphrase: string): Promise<string> {
	return authenticate(async () => {
		const key = storageKey();
		if (key === null) throw new Error('no stored chat key');
		let raw: string | null;
		try {
			raw = localStorage.getItem(key);
		} catch {
			raw = null;
		}
		if (!raw) throw new Error('no stored chat key');
		let envelope: EncryptedChatKey;
		try {
			envelope = parseEnvelope(raw);
		} catch {
			// Corrupt/truncated envelope: there is no usable key, and the caller's
			// only recovery is re-importing the backup.
			throw new Error('unsupported chat key format');
		}
		if (envelope.v !== 1) throw new Error('unsupported chat key format');
		const secret = await decryptEnvelope(envelope, passphrase);
		const pubkey = publicKeyOf(secret);
		if (envelope.pubkey !== undefined && envelope.pubkey !== pubkey)
			throw new Error('Chat public key mismatch.');
		if (localStorage.getItem(key) !== raw)
			throw new Error('Chat key changed during verification. Try again.');
		return secret;
	});
}

async function decryptEnvelope(envelope: EncryptedChatKey, passphrase: string): Promise<string> {
	const key = await deriveKey(
		passphrase,
		hexToBytes(envelope.salt),
		envelope.iter ?? LEGACY_PBKDF2_ITERATIONS
	);
	const plaintext = await crypto.subtle.decrypt(
		{
			name: 'AES-GCM',
			iv: hexToBytes(envelope.iv),
			...(envelope.aad === 1
				? { additionalData: identityData(envelope.owner!, envelope.tenant ?? null) }
				: {})
		},
		key,
		hexToBytes(envelope.ciphertext)
	);
	return bytesToHex(new Uint8Array(plaintext));
}

function identityData(owner: string, tenant: string | null): Uint8Array<ArrayBuffer> {
	return new TextEncoder().encode(JSON.stringify([1, owner, tenant]));
}

export function exportChatKey(): string {
	const key = storageKey();
	if (key === null) throw new Error('no stored chat key');
	const raw = localStorage.getItem(key);
	if (!raw) throw new Error('no stored chat key');
	return raw;
}

export async function importChatKey(
	json: string,
	passphrase: string,
	expectedPubkey?: string,
	confirmLegacy = false,
	assertCurrent?: () => void
): Promise<string> {
	return authenticate(async () => {
		const generation = scopeGeneration;
		const vaultKey = storageKey()!;
		const previous = localStorage.getItem(vaultKey);
		const parsed = parseEnvelope(json);
		// Old backups cannot attest account ownership; require explicit consent before binding.
		if (parsed.aad !== 1 && !expectedPubkey && !confirmLegacy)
			throw new Error(
				'Legacy backup requires an existing account binding or confirmation that this is your identity.'
			);
		let secretKey: string;
		try {
			secretKey = await decryptEnvelope(parsed, passphrase);
		} catch {
			throw new Error('wrong passphrase or corrupt backup');
		}
		const pubkey = publicKeyOf(secretKey);
		if (
			(parsed.pubkey !== undefined && parsed.pubkey !== pubkey) ||
			(expectedPubkey !== undefined && expectedPubkey !== pubkey)
		) {
			throw new Error('That backup does not match the Chat identity bound to this account.');
		}
		assertChatKeySession(generation);
		if (localStorage.getItem(vaultKey) !== previous)
			throw new Error('Chat key changed during verification. Try again.');
		// An unbound import must never replace a local identity silently.
		if (!expectedPubkey && hasChatKey() && storedKeyPubkey() !== pubkey)
			throw new Error(
				'A different Chat identity is already stored. Export it before importing a different identity.'
			);
		await saveChatKey(secretKey, pubkey, passphrase, assertCurrent);
		return secretKey;
	});
}

export function clearChatKey(): void {
	try {
		const key = storageKey();
		if (key !== null) localStorage.removeItem(key);
	} catch {
		// Storage unavailable — nothing to clear.
	}
}
