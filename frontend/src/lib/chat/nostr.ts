// Minimal Nostr client for the Chat application: key generation, kind-1/kind-22242
// signing (BIP-340 Schnorr), and NIP-42 relay publish. The private key never
// leaves the browser; the backend never sees it.
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

export const NOSTR_KIND_TEXT = 1;
export const NOSTR_KIND_AUTH = 22242;

export type NostrTag = string[];

export interface NostrEvent {
	id: string;
	pubkey: string;
	created_at: number;
	kind: number;
	tags: NostrTag[];
	content: string;
	sig: string;
}

export function generateSecretKey(): string {
	const bytes = crypto.getRandomValues(new Uint8Array(32));
	return bytesToHex(bytes);
}

export function publicKeyOf(secretKey: string): string {
	return bytesToHex(schnorr.getPublicKey(hexToBytes(secretKey)));
}

function serializeForId(event: Omit<NostrEvent, 'id' | 'sig'>): string {
	return JSON.stringify([0, event.pubkey, event.created_at, event.kind, event.tags, event.content]);
}

async function sha256Hex(input: string): Promise<string> {
	const bytes = new TextEncoder().encode(input);
	const digest = await crypto.subtle.digest('SHA-256', bytes);
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

export async function buildUnsignedEvent(
	kind: number,
	content: string,
	tags: NostrTag[],
	pubkey: string
): Promise<Omit<NostrEvent, 'id' | 'sig'>> {
	return {
		pubkey,
		created_at: Math.floor(Date.now() / 1000),
		kind,
		tags,
		content
	};
}

export async function signEvent(
	unsigned: Omit<NostrEvent, 'id' | 'sig'>,
	secretKey: string
): Promise<NostrEvent> {
	const id = await sha256Hex(serializeForId(unsigned));
	const sig = bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(secretKey)));
	return { ...unsigned, id, sig };
}

/** Sign and publish one event over a NIP-42 relay session. Returns false on
 * any failure (timeout, rejected auth, relay error) — never throws. */
export async function publishEvent(
	relayUrl: string,
	unsigned: Omit<NostrEvent, 'id' | 'sig'>,
	secretKey: string
): Promise<boolean> {
	const signed = await signEvent(unsigned, secretKey);
	return await new Promise<boolean>((resolve) => {
		let settled = false;
		const finish = (ok: boolean) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			socket.close();
			resolve(ok);
		};
		const timer = setTimeout(() => finish(false), 10_000);
		const socket = new WebSocket(relayUrl);

		socket.onopen = () => {
			// Ask for the AUTH challenge by sending an empty subscription.
			socket.send(JSON.stringify(['REQ', 'auth-probe', { limit: 0 }]));
		};
		socket.onmessage = async (raw) => {
			let message: unknown;
			try {
				message = JSON.parse(String(raw.data));
			} catch {
				return;
			}
			if (!Array.isArray(message)) return;
			if (message[0] === 'AUTH' && typeof message[1] === 'string') {
				const auth = await signEvent(
					await buildUnsignedEvent(
						NOSTR_KIND_AUTH,
						'',
						[
							['relay', relayUrl],
							['challenge', message[1]]
						],
						unsigned.pubkey
					),
					secretKey
				);
				socket.send(JSON.stringify(['AUTH', auth]));
				socket.send(JSON.stringify(['EVENT', signed]));
			}
			if (message[0] === 'OK' && message[1] === signed.id) {
				finish(message[2] === true);
			}
		};
		socket.onerror = () => finish(false);
		socket.onclose = () => finish(false);
	});
}
