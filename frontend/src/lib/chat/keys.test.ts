import { describe, expect, it, beforeEach, vi } from 'vitest';
import {
	saveChatKey,
	loadChatKey,
	hasChatKey,
	clearChatKey,
	exportChatKey,
	importChatKey,
	storedKeyPubkey,
	setChatKeyUser
} from './keys';
import { generateSecretKey, publicKeyOf } from './nostr';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const LEGACY_KEY = 'elembra.chat.key.v1';

beforeEach(() => {
	localStorage.clear();
	setChatKeyUser(null);
});

describe('chat key custody', () => {
	it('round-trips the key with the correct passphrase', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		expect(hasChatKey()).toBe(true);
		expect(storedKeyPubkey()).toBe(publicKeyOf(sk));
		await expect(loadChatKey('correct horse')).resolves.toBe(sk);
	});

	it('rejects the wrong passphrase', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		await expect(loadChatKey('wrong')).rejects.toThrow();
	});

	it('rejects tampered ciphertext', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		const raw = localStorage.getItem(`${LEGACY_KEY}.user-1`)!;
		const parsed = JSON.parse(raw);
		parsed.ciphertext = '00' + parsed.ciphertext.slice(2);
		localStorage.setItem(`${LEGACY_KEY}.user-1`, JSON.stringify(parsed));
		await expect(loadChatKey('correct horse')).rejects.toThrow();
	});

	it('imports a backup and clears on demand', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		const backup = exportChatKey();
		clearChatKey();
		expect(hasChatKey()).toBe(false);
		await expect(importChatKey(backup, 'pass')).resolves.toBe(sk);
		await expect(loadChatKey('pass')).resolves.toBe(sk);
	});

	it('is scoped per user: a second user never inherits the first envelope', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		expect(hasChatKey()).toBe(true);

		setChatKeyUser('user-2');
		expect(hasChatKey()).toBe(false);
		expect(storedKeyPubkey()).toBeNull();
		await expect(loadChatKey('pass')).rejects.toThrow();

		// Switching back restores user-1's vault.
		setChatKeyUser('user-1');
		expect(hasChatKey()).toBe(true);
		await expect(loadChatKey('pass')).resolves.toBe(sk);
	});

	it('hides the vault when logged out and still saves under the scoped key', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		setChatKeyUser(null);
		expect(hasChatKey()).toBe(false);
		expect(storedKeyPubkey()).toBeNull();
	});

	it('migrates a legacy unscoped envelope to the first scoped user', async () => {
		const sk = generateSecretKey();
		const envelope = {
			v: 1,
			pubkey: publicKeyOf(sk),
			salt: '00'.repeat(16),
			iv: '00'.repeat(12),
			ciphertext: '00'.repeat(32),
			iter: 100_000
		};
		localStorage.setItem(LEGACY_KEY, JSON.stringify(envelope));
		setChatKeyUser('user-1');
		expect(localStorage.getItem(`${LEGACY_KEY}.user-1`)).toBe(JSON.stringify(envelope));
		expect(localStorage.getItem(LEGACY_KEY)).toBeNull();
		expect(hasChatKey()).toBe(true);
	});

	it('decrypts a legacy envelope that records its own iterations', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		// A 100k envelope (as produced before the KDF bump) must still load.
		const legacySalt = new Uint8Array(16).fill(7);
		const legacyIv = new Uint8Array(12).fill(9);
		const material = await crypto.subtle.importKey(
			'raw',
			new TextEncoder().encode('pass'),
			'PBKDF2',
			false,
			['deriveKey']
		);
		const derived = await crypto.subtle.deriveKey(
			{ name: 'PBKDF2', salt: legacySalt, iterations: 100_000, hash: 'SHA-256' },
			material,
			{ name: 'AES-GCM', length: 256 },
			false,
			['encrypt', 'decrypt']
		);
		const ciphertext = await crypto.subtle.encrypt(
			{ name: 'AES-GCM', iv: legacyIv },
			derived,
			hexToBytes(sk)
		);
		localStorage.setItem(
			`${LEGACY_KEY}.user-1`,
			JSON.stringify({
				v: 1,
				pubkey: publicKeyOf(sk),
				salt: bytesToHex(legacySalt),
				iv: bytesToHex(legacyIv),
				ciphertext: bytesToHex(new Uint8Array(ciphertext)),
				iter: 100_000
			})
		);
		await expect(loadChatKey('pass')).resolves.toBe(sk);
	});

	it('new envelopes record the hardened iteration count', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		const envelope = JSON.parse(localStorage.getItem(`${LEGACY_KEY}.user-1`)!);
		expect(envelope.iter).toBe(600_000);
	});

	it('derives the pubkey when importing a pubkey-less backup', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		// A legacy envelope without the `pubkey` field.
		const salt = new Uint8Array(16).fill(1);
		const iv = new Uint8Array(12).fill(2);
		const material = await crypto.subtle.importKey(
			'raw',
			new TextEncoder().encode('pass'),
			'PBKDF2',
			false,
			['deriveKey']
		);
		const derived = await crypto.subtle.deriveKey(
			{ name: 'PBKDF2', salt, iterations: 100_000, hash: 'SHA-256' },
			material,
			{ name: 'AES-GCM', length: 256 },
			false,
			['encrypt', 'decrypt']
		);
		const ciphertext = await crypto.subtle.encrypt(
			{ name: 'AES-GCM', iv },
			derived,
			hexToBytes(sk)
		);
		localStorage.setItem(
			`${LEGACY_KEY}.user-1`,
			JSON.stringify({
				v: 1,
				salt: bytesToHex(salt),
				iv: bytesToHex(iv),
				ciphertext: bytesToHex(new Uint8Array(ciphertext)),
				iter: 100_000
			})
		);
		await importChatKey(localStorage.getItem(`${LEGACY_KEY}.user-1`)!, 'pass');
		expect(storedKeyPubkey()).toBe(publicKeyOf(sk));
	});

	it('degrades gracefully when localStorage is unavailable', () => {
		const getItem = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
			throw new Error('storage denied');
		});
		try {
			setChatKeyUser('user-1');
			expect(hasChatKey()).toBe(false);
			expect(storedKeyPubkey()).toBeNull();
		} finally {
			getItem.mockRestore();
		}
	});
});
