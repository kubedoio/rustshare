import { describe, expect, it, beforeEach } from 'vitest';
import {
	saveChatKey,
	loadChatKey,
	hasChatKey,
	clearChatKey,
	exportChatKey,
	importChatKey,
	storedKeyPubkey
} from './keys';
import { generateSecretKey, publicKeyOf } from './nostr';

beforeEach(() => {
	localStorage.clear();
});

describe('chat key custody', () => {
	it('round-trips the key with the correct passphrase', async () => {
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		expect(hasChatKey()).toBe(true);
		expect(storedKeyPubkey()).toBe(publicKeyOf(sk));
		await expect(loadChatKey('correct horse')).resolves.toBe(sk);
	});

	it('rejects the wrong passphrase', async () => {
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		await expect(loadChatKey('wrong')).rejects.toThrow();
	});

	it('rejects tampered ciphertext', async () => {
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		const raw = localStorage.getItem('elembra.chat.key.v1')!;
		const parsed = JSON.parse(raw);
		parsed.ciphertext = '00' + parsed.ciphertext.slice(2);
		localStorage.setItem('elembra.chat.key.v1', JSON.stringify(parsed));
		await expect(loadChatKey('correct horse')).rejects.toThrow();
	});

	it('imports a backup and clears on demand', async () => {
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		const backup = exportChatKey();
		clearChatKey();
		expect(hasChatKey()).toBe(false);
		await expect(importChatKey(backup, 'pass')).resolves.toBe(sk);
		await expect(loadChatKey('pass')).resolves.toBe(sk);
	});
});
