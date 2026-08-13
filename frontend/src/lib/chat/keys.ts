// Client-held Buzz key custody: passphrase-encrypted at rest (WebCrypto
// PBKDF2 + AES-GCM). The raw key never leaves the browser; export/import is
// the only recovery path (ADR-0034: no silent server custody).
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

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

/** Scope the key vault to `userId`, migrating a pre-scoping legacy envelope
 * on first use so existing users keep their key. `null` (logged out) hides
 * the vault entirely. */
export function setChatKeyUser(userId: string | null): void {
	if (storageScope === userId) return;
	storageScope = userId;
	if (userId === null) return;
	const scoped = `${LEGACY_STORAGE_KEY}.${userId}`;
	if (localStorage.getItem(scoped) !== null) return;
	const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
	if (legacy !== null) {
		localStorage.setItem(scoped, legacy);
		localStorage.removeItem(LEGACY_STORAGE_KEY);
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
}

export function hasChatKey(): boolean {
	const key = storageKey();
	return key !== null && localStorage.getItem(key) !== null;
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
	passphrase: string
): Promise<void> {
	const key = storageKey();
	if (key === null) throw new Error('no chat key scope — sign in first');
	const salt = crypto.getRandomValues(new Uint8Array(16));
	const iv = crypto.getRandomValues(new Uint8Array(12));
	const derived = await deriveKey(passphrase, salt, PBKDF2_ITERATIONS);
	const ciphertext = await crypto.subtle.encrypt(
		{ name: 'AES-GCM', iv },
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
	localStorage.setItem(key, JSON.stringify({ ...envelope, pubkey }));
}

export async function loadChatKey(passphrase: string): Promise<string> {
	const key = storageKey();
	if (key === null) throw new Error('no stored chat key');
	const raw = localStorage.getItem(key);
	if (!raw) throw new Error('no stored chat key');
	const envelope = JSON.parse(raw) as EncryptedChatKey;
	if (envelope.v !== 1) throw new Error('unsupported chat key format');
	return decryptEnvelope(envelope, passphrase);
}

async function decryptEnvelope(envelope: EncryptedChatKey, passphrase: string): Promise<string> {
	const key = await deriveKey(
		passphrase,
		hexToBytes(envelope.salt),
		envelope.iter ?? LEGACY_PBKDF2_ITERATIONS
	);
	const plaintext = await crypto.subtle.decrypt(
		{ name: 'AES-GCM', iv: hexToBytes(envelope.iv) },
		key,
		hexToBytes(envelope.ciphertext)
	);
	return bytesToHex(new Uint8Array(plaintext));
}

export function exportChatKey(): string {
	const key = storageKey();
	if (key === null) throw new Error('no stored chat key');
	const raw = localStorage.getItem(key);
	if (!raw) throw new Error('no stored chat key');
	return raw;
}

export async function importChatKey(json: string, passphrase: string): Promise<string> {
	const parsed = JSON.parse(json) as EncryptedChatKey & { secret_key?: string; pubkey?: string };
	if (parsed.v !== 1) throw new Error('unsupported chat key format');
	let secretKey: string;
	if (typeof parsed.secret_key === 'string') {
		// Plaintext-key backup: re-encrypt it under this passphrase.
		secretKey = parsed.secret_key;
	} else if (
		typeof parsed.salt === 'string' &&
		typeof parsed.iv === 'string' &&
		typeof parsed.ciphertext === 'string'
	) {
		// Encrypted-envelope backup: verify it decrypts, then restore it.
		secretKey = await decryptEnvelope(parsed, passphrase);
	} else {
		throw new Error('backup does not contain a key');
	}
	const pubkey = parsed.pubkey ?? storedKeyPubkey() ?? '';
	await saveChatKey(secretKey, pubkey, passphrase);
	return secretKey;
}

export function clearChatKey(): void {
	const key = storageKey();
	if (key !== null) localStorage.removeItem(key);
}
