import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { writable } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import BindingPanel from './BindingPanel.svelte';
import {
	exportChatKey,
	hasChatKey,
	saveChatKey,
	setChatKeyUser,
	storedKeyPubkey,
	clearChatKey
} from '$lib/chat/keys';
import { generateSecretKey, publicKeyOf } from '$lib/chat/nostr';

const mocks = vi.hoisted(() => ({ post: vi.fn() }));
vi.mock('$lib/api/client', () => ({ apiClient: { post: mocks.post } }));
vi.mock('$lib/stores/auth', async () => {
	const { writable } = await import('svelte/store');
	return { currentUser: writable({ id: 'user-1', tenant_id: 'tenant-1' }) };
});
import { currentUser } from '$lib/stores/auth';

beforeEach(() => {
	localStorage.clear();
	setChatKeyUser(null);
	setChatKeyUser('user-1', 'tenant-1');
	(currentUser as ReturnType<typeof writable>).set({ id: 'user-1', tenant_id: 'tenant-1' });
	mocks.post.mockReset();
	mocks.post.mockImplementation(async (path, body) =>
		path.endsWith('/challenge')
			? {
					challenge_id: 'challenge-1',
					nonce: 'nonce-1',
					relay_url: 'wss://buzz.example',
					buzz_pubkey: body.buzz_pubkey,
					expires_at: new Date(Date.now() + 300_000).toISOString()
				}
			: {}
	);
});
afterEach(cleanup);

describe('Chat onboarding with real local cryptography', () => {
	it('creates one identity and sends only its public key and signed proof to the binding API', async () => {
		const onBound = vi.fn();
		render(BindingPanel, { onBound });
		expect(screen.getByText(/your original signing key cannot be restored/)).toBeTruthy();
		await fireEvent.input(screen.getByLabelText('key passphrase'), {
			target: { value: 'private passphrase' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Create identity & bind' }));
		await waitFor(() => expect(onBound).toHaveBeenCalledOnce());
		expect(mocks.post.mock.calls.map(([path]) => path)).toEqual([
			'/applications/chat/identity-binding/challenge',
			'/applications/chat/identity-binding/verify',
			'/applications/chat/admission'
		]);
		const proof = mocks.post.mock.calls[1][1].event;
		expect(proof.kind).toBe(22242);
		expect(proof.pubkey).toBe(storedKeyPubkey());
		expect(proof.tags).toContainEqual(['challenge', 'nonce-1']);
		expect(JSON.stringify(mocks.post.mock.calls)).not.toContain('private passphrase');
		expect((screen.getByLabelText('key passphrase') as HTMLInputElement).value).toBe('');
	});

	it('imports before requesting a challenge and binds the imported public key', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		await saveChatKey(sk, pk, 'pass');
		const backup = exportChatKey();
		clearChatKey();
		const onBound = vi.fn();
		render(BindingPanel, { onBound });
		await fireEvent.click(screen.getByRole('button', { name: 'Import existing Chat identity' }));
		await fireEvent.input(screen.getByLabelText('Encrypted key backup'), {
			target: { value: backup }
		});
		await fireEvent.input(screen.getByLabelText('backup passphrase'), {
			target: { value: 'pass' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Import & bind' }));
		await waitFor(() => expect(onBound).toHaveBeenCalledOnce());
		expect(storedKeyPubkey()).toBe(pk);
		expect(mocks.post.mock.calls[0][1]).toEqual({ workspace_id: 'tenant-1', buzz_pubkey: pk });
		expect(JSON.stringify(mocks.post.mock.calls)).not.toContain(sk);
		expect(JSON.stringify(mocks.post.mock.calls)).not.toContain(backup);
		expect((screen.getByLabelText('Encrypted key backup') as HTMLTextAreaElement).value).toBe('');
	});

	it('rejects a malformed import before any binding request and clears both inputs', async () => {
		render(BindingPanel, { onBound: vi.fn() });
		await fireEvent.click(screen.getByRole('button', { name: 'Import existing Chat identity' }));
		await fireEvent.input(screen.getByLabelText('Encrypted key backup'), {
			target: { value: 'null' }
		});
		await fireEvent.input(screen.getByLabelText('backup passphrase'), {
			target: { value: 'pass' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Import & bind' }));
		await screen.findByRole('alert');
		expect(mocks.post).not.toHaveBeenCalled();
		expect(hasChatKey()).toBe(false);
		expect((screen.getByLabelText('backup passphrase') as HTMLInputElement).value).toBe('');
		expect((screen.getByLabelText('Encrypted key backup') as HTMLTextAreaElement).value).toBe('');
	});

	it('does not send proof under a new account after the challenge response', async () => {
		mocks.post.mockImplementation(async () => {
			setChatKeyUser('user-2', 'tenant-2');
			return { challenge_id: 'old' };
		});
		render(BindingPanel, { onBound: vi.fn() });
		await fireEvent.input(screen.getByLabelText('key passphrase'), { target: { value: 'pass' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Create identity & bind' }));
		await screen.findByRole('alert');
		expect(mocks.post).toHaveBeenCalledOnce();
	});
});
