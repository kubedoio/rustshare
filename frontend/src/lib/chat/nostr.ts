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

export type PublishResult =
	| { ok: true; event_id: string }
	| { ok: false; reason: 'transport' } // socket error/timeout before an OK frame
	| { ok: false; reason: 'rejected'; detail?: string }; // relay answered OK false (NIP-20 message)

/** Sign and publish one event over a NIP-42 relay session. Never throws: a
 * transport failure (timeout, socket error) resolves `{ok:false,
 * reason:'transport'}` and an explicit relay rejection resolves
 * `{ok:false, reason:'rejected'}` so callers can tell "relay down" from
 * "not admitted / blocked". */
export async function publishEvent(
	relayUrl: string,
	unsigned: Omit<NostrEvent, 'id' | 'sig'>,
	secretKey: string
): Promise<PublishResult> {
	const signed = await signEvent(unsigned, secretKey);
	return await new Promise<PublishResult>((resolve) => {
		let settled = false;
		const finish = (result: PublishResult) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			clearTimeout(authGrace);
			socket.close();
			resolve(result);
		};
		const timer = setTimeout(() => finish({ ok: false, reason: 'transport' }), 10_000);
		const socket = new WebSocket(relayUrl);
		let authGrace: ReturnType<typeof setTimeout> | undefined;

		socket.onopen = () => {
			// Ask for the AUTH challenge by sending an empty subscription.
			socket.send(JSON.stringify(['REQ', 'auth-probe', { limit: 0 }]));
			// Some relays never challenge; give them a short grace, then
			// publish anyway so a challenge-less relay still works.
			authGrace = setTimeout(() => {
				socket.send(JSON.stringify(['EVENT', signed]));
			}, 1500);
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
				clearTimeout(authGrace);
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
				finish(
					message[2] === true
						? { ok: true, event_id: signed.id }
						: {
								ok: false,
								reason: 'rejected',
								detail: typeof message[3] === 'string' ? message[3] : undefined
							}
				);
			}
		};
		socket.onerror = () => finish({ ok: false, reason: 'transport' });
		socket.onclose = () => finish({ ok: false, reason: 'transport' });
	});
}
