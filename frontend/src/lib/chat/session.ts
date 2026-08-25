// In-memory Chat identity session. The secret key is loaded from the encrypted
// localStorage envelope once per session and retained only in module memory;
// the passphrase itself is never stored.
import { writable, type Readable } from 'svelte/store';
import { hasChatKey, loadChatKey } from './keys';
import { publicKeyOf } from './nostr';

export type ChatSessionState =
	{ readonly state: 'locked' } | { readonly state: 'unlocked'; readonly pubkey: string };

export class ChatSessionError extends Error {
	constructor(
		public readonly code: 'NO_KEY' | 'WRONG_PASSPHRASE' | 'CORRUPT_KEY' | 'PUBKEY_MISMATCH',
		message: string
	) {
		super(message);
		this.name = 'ChatSessionError';
	}
}

let memorySecretKey: string | null = null;

const internal = writable<ChatSessionState>({ state: 'locked' });

export const chatSessionStore: Readable<ChatSessionState> = {
	subscribe: internal.subscribe
};

export function isUnlocked(): boolean {
	return memorySecretKey !== null;
}

export function getSigningKey(): string | null {
	return memorySecretKey;
}

export async function unlock(passphrase: string, boundPubkey: string): Promise<void> {
	if (!hasChatKey()) {
		throw new ChatSessionError(
			'NO_KEY',
			'No Chat identity found. Import a key backup from your original device.'
		);
	}

	let secretKey: string;
	try {
		secretKey = await loadChatKey(passphrase);
	} catch (err) {
		const message = err instanceof Error ? err.message : '';
		if (message === 'unsupported chat key format') {
			throw new ChatSessionError(
				'CORRUPT_KEY',
				'Stored Chat key is corrupted or uses an unsupported format.'
			);
		}
		// Any other failure from loadChatKey is treated as a wrong passphrase.
		throw new ChatSessionError('WRONG_PASSPHRASE', 'Passphrase is incorrect.');
	}

	const pubkey = publicKeyOf(secretKey);
	if (pubkey !== boundPubkey) {
		throw new ChatSessionError(
			'PUBKEY_MISMATCH',
			'This Chat key does not match your bound Buzz identity.'
		);
	}

	memorySecretKey = secretKey;
	internal.set({ state: 'unlocked', pubkey });
}

export function lock(): void {
	memorySecretKey = null;
	internal.set({ state: 'locked' });
}

export function clear(): void {
	lock();
}
