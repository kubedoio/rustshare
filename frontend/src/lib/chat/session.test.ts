import { describe, expect, it, beforeEach } from 'vitest';
import {
	unlock,
	lock,
	clear,
	isUnlocked,
	getSigningKey,
	chatSessionStore,
	ChatSessionError
} from './session';
import { setChatKeyUser, saveChatKey, clearChatKey } from './keys';
import { generateSecretKey, publicKeyOf } from './nostr';
import { get } from 'svelte/store';

beforeEach(() => {
	localStorage.clear();
	setChatKeyUser(null);
	clear();
});

describe('chat session', () => {
	it('unlocks, exposes the signing key, and reports unlocked', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');

		await unlock('pass', pk);

		expect(isUnlocked()).toBe(true);
		expect(getSigningKey()).toBe(sk);
		const state = get(chatSessionStore);
		expect(state.state).toBe('unlocked');
		if (state.state === 'unlocked') {
			expect(state.pubkey).toBe(pk);
		}
	});

	it('locks and clears the in-memory key without touching storage', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');
		await unlock('pass', pk);

		lock();

		expect(isUnlocked()).toBe(false);
		expect(getSigningKey()).toBeNull();
		expect(get(chatSessionStore).state).toBe('locked');
		// The encrypted envelope is still available for the next unlock.
		await expect(unlock('pass', pk)).resolves.toBeUndefined();
	});

	it('clear is an alias for lock', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');
		await unlock('pass', pk);

		clear();

		expect(isUnlocked()).toBe(false);
		expect(getSigningKey()).toBeNull();
	});

	it('rejects the wrong passphrase without unlocking', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');

		await expect(unlock('wrong', pk)).rejects.toBeInstanceOf(ChatSessionError);
		await expect(unlock('wrong', pk)).rejects.toMatchObject({ code: 'WRONG_PASSPHRASE' });

		expect(isUnlocked()).toBe(false);
		expect(getSigningKey()).toBeNull();
	});

	it('rejects a stored key whose pubkey does not match the bound identity', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');

		const otherPk = publicKeyOf(generateSecretKey());
		await expect(unlock('pass', otherPk)).rejects.toBeInstanceOf(ChatSessionError);
		await expect(unlock('pass', otherPk)).rejects.toMatchObject({ code: 'PUBKEY_MISMATCH' });

		expect(isUnlocked()).toBe(false);
		expect(getSigningKey()).toBeNull();
	});

	it('rejects unlock when no key is stored', async () => {
		setChatKeyUser('user-1');
		const pk = publicKeyOf(generateSecretKey());

		await expect(unlock('pass', pk)).rejects.toBeInstanceOf(ChatSessionError);
		await expect(unlock('pass', pk)).rejects.toMatchObject({ code: 'NO_KEY' });
	});

	it('rejects a corrupt stored envelope as CORRUPT_KEY', async () => {
		setChatKeyUser('user-1');
		localStorage.setItem('elembra.chat.key.v1.user-1', '{"v":1,"ciphertext":"garbage');
		const pk = publicKeyOf(generateSecretKey());

		await expect(unlock('pass', pk)).rejects.toBeInstanceOf(ChatSessionError);
		await expect(unlock('pass', pk)).rejects.toMatchObject({ code: 'CORRUPT_KEY' });
	});

	it('does not retain the passphrase or secret key after lock', async () => {
		setChatKeyUser('user-1');
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');
		await unlock('pass', pk);

		const before = getSigningKey();
		expect(before).toBe(sk);

		lock();

		expect(getSigningKey()).toBeNull();
		// No shadow copy should remain on the module.
		expect((globalThis as unknown as { __chatSecretKey?: string }).__chatSecretKey).toBeUndefined();
	});
});
