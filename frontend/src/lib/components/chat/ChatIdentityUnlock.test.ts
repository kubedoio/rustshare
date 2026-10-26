import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ChatIdentityUnlock from './ChatIdentityUnlock.svelte';
import { saveChatKey, setChatKeyUser, exportChatKey, clearChatKey } from '$lib/chat/keys';
import { clear, getSigningKey } from '$lib/chat/session';
import { generateSecretKey, publicKeyOf } from '$lib/chat/nostr';
import * as keys from '$lib/chat/keys';

beforeEach(() => {
	localStorage.clear();
	setChatKeyUser(null);
	setChatKeyUser('user-1');
	clear();
});
afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});

describe('Chat recovery', () => {
	it.each(['rotation', 'unmount'])(
		'does not commit a backup after %s during verification',
		async (transition) => {
			const sk = generateSecretKey();
			const pk = publicKeyOf(sk);
			await saveChatKey(sk, pk, 'pass');
			const backup = exportChatKey();
			clearChatKey();
			const onUnlocked = vi.fn();
			const importing = vi.spyOn(keys, 'importChatKey');
			const view = render(ChatIdentityUnlock, { boundPubkey: pk, onUnlocked });
			await fireEvent.input(screen.getByLabelText('Key backup'), { target: { value: backup } });
			await fireEvent.input(screen.getByLabelText('backup passphrase'), {
				target: { value: 'pass' }
			});
			await fireEvent.click(screen.getByRole('button', { name: 'Import key' }));
			if (transition === 'rotation')
				await view.rerender({ boundPubkey: publicKeyOf(generateSecretKey()), onUnlocked });
			else view.unmount();
			await expect(importing.mock.results[0].value).rejects.toThrow('Chat binding changed');
			expect(localStorage.getItem('elembra.chat.key.v1.user-1')).toBeNull();
			expect(onUnlocked).not.toHaveBeenCalled();
			expect(getSigningKey()).toBeNull();
		}
	);
	it('recovers a bound identity with local validation before unlock', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');
		const backup = exportChatKey();
		clearChatKey();
		const onUnlocked = vi.fn();
		render(ChatIdentityUnlock, { boundPubkey: pk, onUnlocked });
		await fireEvent.input(screen.getByLabelText('Key backup'), { target: { value: backup } });
		await fireEvent.input(screen.getByLabelText('backup passphrase'), {
			target: { value: 'pass' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Import key' }));
		await waitFor(() => expect(onUnlocked).toHaveBeenCalledOnce());
		expect(getSigningKey()).toBe(sk);
	});

	it('preserves the existing vault when a recovery backup does not match the binding', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');
		const original = exportChatKey();
		render(ChatIdentityUnlock, {
			boundPubkey: publicKeyOf(generateSecretKey()),
			onUnlocked: vi.fn()
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Recover from backup' }));
		await fireEvent.input(screen.getByLabelText('Key backup'), { target: { value: original } });
		await fireEvent.input(screen.getByLabelText('backup passphrase'), {
			target: { value: 'pass' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Import key' }));
		await screen.findByRole('alert');
		expect(exportChatKey()).toBe(original);
		expect(getSigningKey()).toBeNull();
		expect((screen.getByLabelText('backup passphrase') as HTMLInputElement).value).toBe('');
		expect((screen.getByLabelText('Key backup') as HTMLTextAreaElement).value).toBe('');
	});

	it('clears a passphrase when an empty backup is submitted', async () => {
		render(ChatIdentityUnlock, {
			boundPubkey: publicKeyOf(generateSecretKey()),
			onUnlocked: vi.fn()
		});
		await fireEvent.input(screen.getByLabelText('backup passphrase'), {
			target: { value: 'secret' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Import key' }));
		expect((screen.getByLabelText('backup passphrase') as HTMLInputElement).value).toBe('');
		expect(screen.getByRole('alert').textContent).toContain('Paste the key backup first');
	});
});
