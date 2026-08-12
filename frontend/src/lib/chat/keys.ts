// Client-held Buzz key custody: passphrase-encrypted at rest (WebCrypto
// PBKDF2 + AES-GCM). The raw key never leaves the browser; export/import is
// the only recovery path (ADR-0034: no silent server custody).
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const STORAGE_KEY = 'elembra.chat.key.v1';
const PBKDF2_ITERATIONS = 100_000;

export interface EncryptedChatKey {
	v: 1;
	salt: string; // hex
	iv: string; // hex
	ciphertext: string; // hex
}

export function hasChatKey(): boolean {
	return localStorage.getItem(STORAGE_KEY) !== null;
}

export function storedKeyPubkey(): string | null {
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		if (!raw) return null;
		const envelope: { pubkey?: string } = JSON.parse(raw);
		return envelope.pubkey ?? null;
	} catch {
		return null;
	}
}

async function deriveKey(passphrase: string, salt: Uint8Array<ArrayBuffer>): Promise<CryptoKey> {
	const material = await crypto.subtle.importKey(
		'raw',
		new TextEncoder().encode(passphrase),
		'PBKDF2',
		false,
		['deriveKey']
	);
	return crypto.subtle.deriveKey(
		{ name: 'PBKDF2', salt, iterations: PBKDF2_ITERATIONS, hash: 'SHA-256' },
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
	const salt = crypto.getRandomValues(new Uint8Array(16));
	const iv = crypto.getRandomValues(new Uint8Array(12));
	const key = await deriveKey(passphrase, salt);
	const ciphertext = await crypto.subtle.encrypt(
		{ name: 'AES-GCM', iv },
		key,
		hexToBytes(secretKey)
	);
	const envelope: EncryptedChatKey = {
		v: 1,
		salt: bytesToHex(salt),
		iv: bytesToHex(iv),
		ciphertext: bytesToHex(new Uint8Array(ciphertext))
	};
	localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...envelope, pubkey }));
}

export async function loadChatKey(passphrase: string): Promise<string> {
	const raw = localStorage.getItem(STORAGE_KEY);
	if (!raw) throw new Error('no stored chat key');
	const envelope = JSON.parse(raw) as EncryptedChatKey;
	if (envelope.v !== 1) throw new Error('unsupported chat key format');
	return decryptEnvelope(envelope, passphrase);
}

async function decryptEnvelope(envelope: EncryptedChatKey, passphrase: string): Promise<string> {
	const key = await deriveKey(passphrase, hexToBytes(envelope.salt));
	const plaintext = await crypto.subtle.decrypt(
		{ name: 'AES-GCM', iv: hexToBytes(envelope.iv) },
		key,
		hexToBytes(envelope.ciphertext)
	);
	return bytesToHex(new Uint8Array(plaintext));
}

export function exportChatKey(): string {
	const raw = localStorage.getItem(STORAGE_KEY);
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
	localStorage.removeItem(STORAGE_KEY);
}
