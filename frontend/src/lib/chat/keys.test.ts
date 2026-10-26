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
	it('preserves a valid vault on claimed-key and binding mismatch', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');
		const backup = exportChatKey();
		const other = publicKeyOf(generateSecretKey());
		await expect(
			importChatKey(JSON.stringify({ ...JSON.parse(backup), pubkey: other }), 'pass', pk)
		).rejects.toThrow('does not match');
		expect(exportChatKey()).toBe(backup);
		await expect(importChatKey(backup, 'pass', other)).rejects.toThrow('does not match');
		expect(exportChatKey()).toBe(backup);
	});

	it.each([
		'null',
		'[]',
		'{}',
		'{',
		'x'.repeat(4097),
		'{"v":1,"secret_key":"' + '01'.repeat(32) + '"}'
	])('rejects malformed or plaintext backup without changing the vault: %.20s', async (backup) => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		const original = exportChatKey();
		await expect(importChatKey(backup, 'pass')).rejects.toThrow();
		expect(exportChatKey()).toBe(original);
	});

	it('rejects cross-account and cross-tenant backups', async () => {
		setChatKeyUser('user-1', 'tenant-1');
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');
		const backup = exportChatKey();
		setChatKeyUser('user-2', 'tenant-1');
		await expect(importChatKey(backup, 'pass', pk)).rejects.toThrow('another account');
		expect(hasChatKey()).toBe(false);
		setChatKeyUser('user-1', 'tenant-2');
		await expect(importChatKey(backup, 'pass', pk)).rejects.toThrow('another workspace');
	});

	it.each([null, 'user-2', 'user-1'])(
		'cancels an import on logout or account switching, including ABA (%s)',
		async (nextUser) => {
			setChatKeyUser('user-1');
			const sk = generateSecretKey();
			await saveChatKey(sk, publicKeyOf(sk), 'pass');
			const original = exportChatKey();
			const pending = importChatKey(original, 'pass');
			setChatKeyUser('user-2');
			setChatKeyUser(nextUser);
			await expect(pending).rejects.toThrow('account changed');
			setChatKeyUser('user-1');
			expect(exportChatKey()).toBe(original);
		}
	);

	it('shares bounded attempts and increasing backoff across unlock and import, resetting on success', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		const backup = exportChatKey();
		const now = vi.spyOn(Date, 'now').mockReturnValue(100_000);
		try {
			await expect(loadChatKey('wrong')).rejects.toThrow();
			await expect(importChatKey(backup, 'wrong')).rejects.toThrow();
			await expect(loadChatKey('wrong')).rejects.toThrow();
			await expect(importChatKey(backup, 'pass')).rejects.toThrow('1 seconds');
			setChatKeyUser('user-2');
			await expect(loadChatKey('pass')).rejects.toThrow('no stored chat key');
			setChatKeyUser('user-1');
			await expect(loadChatKey('pass')).rejects.toThrow('1 seconds');
			now.mockReturnValue(101_000);
			await expect(importChatKey(backup, 'wrong')).rejects.toThrow('wrong passphrase');
			await expect(loadChatKey('pass')).rejects.toThrow('2 seconds');
			now.mockReturnValue(103_000);
			await expect(loadChatKey('pass')).resolves.toBe(sk);
			await expect(loadChatKey('wrong')).rejects.toThrow();
			await expect(loadChatKey('pass')).resolves.toBe(sk);
		} finally {
			now.mockRestore();
		}
	});

	it('rejects concurrent verification through another entry point', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		const backup = exportChatKey();
		const pending = loadChatKey('pass');
		await expect(importChatKey(backup, 'pass')).rejects.toThrow('already in progress');
		await expect(pending).resolves.toBe(sk);
	});

	it('rejects a decrypted value that is not a valid cryptographic secret without replacing the vault', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		const original = exportChatKey();
		const decrypt = vi.spyOn(crypto.subtle, 'decrypt').mockResolvedValue(new Uint8Array(32).buffer);
		try {
			await expect(importChatKey(original, 'pass')).rejects.toThrow();
			expect(exportChatKey()).toBe(original);
		} finally {
			decrypt.mockRestore();
		}
	});

	it('requires a known binding or explicit confirmation for legacy encrypted backups', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');
		const salt = new Uint8Array(16).fill(1);
		const iv = new Uint8Array(12).fill(2);
		const material = await crypto.subtle.importKey(
			'raw',
			new TextEncoder().encode('pass'),
			'PBKDF2',
			false,
			['deriveKey']
		);
		const key = await crypto.subtle.deriveKey(
			{ name: 'PBKDF2', salt, iterations: 100_000, hash: 'SHA-256' },
			material,
			{ name: 'AES-GCM', length: 256 },
			false,
			['encrypt']
		);
		const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, hexToBytes(sk));
		const parsed = {
			v: 1,
			salt: bytesToHex(salt),
			iv: bytesToHex(iv),
			ciphertext: bytesToHex(new Uint8Array(ciphertext))
		};
		clearChatKey();
		await expect(importChatKey(JSON.stringify(parsed), 'pass')).rejects.toThrow(
			'existing account binding'
		);
		await expect(importChatKey(JSON.stringify(parsed), 'pass', pk)).resolves.toBe(sk);
		clearChatKey();
		await expect(importChatKey(JSON.stringify(parsed), 'pass', undefined, true)).resolves.toBe(sk);
	});

	it('authenticates account metadata and rejects relabeling or stripping it', async () => {
		setChatKeyUser('user-1', 'tenant-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		const backup = JSON.parse(exportChatKey());
		setChatKeyUser('user-2', 'tenant-2');
		await expect(
			importChatKey(JSON.stringify({ ...backup, owner: 'user-2', tenant: 'tenant-2' }), 'pass')
		).rejects.toThrow('wrong passphrase or corrupt');
		const { owner: _owner, tenant: _tenant, aad: _aad, ...stripped } = backup;
		await expect(importChatKey(JSON.stringify(stripped), 'pass', undefined, true)).rejects.toThrow(
			'wrong passphrase or corrupt'
		);
		expect(hasChatKey()).toBe(false);
	});

	it.each([0, 99_999, 600_001, 1.5, '600000'])(
		'rejects unsafe KDF iteration values: %s',
		async (iter) => {
			setChatKeyUser('user-1');
			const sk = generateSecretKey();
			await saveChatKey(sk, publicKeyOf(sk), 'pass');
			await expect(
				importChatKey(JSON.stringify({ ...JSON.parse(exportChatKey()), iter }), 'pass')
			).rejects.toThrow('unsupported');
		}
	);
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
		// XOR the first ciphertext byte with 0xff so the tamper is a change on
		// every run (a fixed byte like '00' is a no-op ~1/256 of the time,
		// which made this test flaky). AES-GCM then fails tag validation.
		const first = (parseInt(parsed.ciphertext.slice(0, 2), 16) ^ 0xff)
			.toString(16)
			.padStart(2, '0');
		parsed.ciphertext = first + parsed.ciphertext.slice(2);
		localStorage.setItem(`${LEGACY_KEY}.user-1`, JSON.stringify(parsed));
		await expect(loadChatKey('correct horse')).rejects.toThrow();
	});

	it('reports a corrupt envelope as an unsupported format, not a raw parse error', async () => {
		setChatKeyUser('user-1');
		localStorage.setItem(`${LEGACY_KEY}.user-1`, '{"v":1,"ciphertext":"garbage');
		await expect(loadChatKey('pass')).rejects.toThrow('unsupported chat key format');
	});

	it('reports a friendly error for a non-JSON backup', async () => {
		setChatKeyUser('user-1');
		await expect(importChatKey('not json', 'pass')).rejects.toThrow('invalid backup format');
	});

	it('reports a friendly error for a wrong passphrase on an envelope backup', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		const backup = exportChatKey();
		await expect(importChatKey(backup, 'wrong')).rejects.toThrow(
			'wrong passphrase or corrupt backup'
		);
	});

	it('rejects a backup whose secret key is not a 32-byte hex key', async () => {
		setChatKeyUser('user-1');
		await expect(importChatKey('{"v":1,"secret_key":"zzzz"}', 'pass')).rejects.toThrow(
			'unsupported chat key format'
		);
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
		await importChatKey(localStorage.getItem(`${LEGACY_KEY}.user-1`)!, 'pass', publicKeyOf(sk));
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
