// Minimal Nostr client for the Chat application: key generation, kind-9
// stream-message / kind-1 legacy note / kind-22242 signing (BIP-340 Schnorr),
// and NIP-42 relay publish. The private key never leaves the browser; the
// backend never sees it.
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

// Legacy global text note: the fallback publish kind while channels are
// identified by names (the Buzz relay requires a UUID `h` tag for stream
// kinds, so name-based channels cannot publish kind 9).
export const NOSTR_KIND_TEXT = 1;
// Buzz KIND_STREAM_MESSAGE: channel-scoped via the NIP-29 `["h", "<channel-uuid>"]`
// tag — the canonical chat message kind (spec: "Canonical publish tags and kinds").
export const NOSTR_KIND_STREAM_MESSAGE = 9;
export const NOSTR_KIND_AUTH = 22242;

/** Canonical UUID form (8-4-4-4-12 hex, case-insensitive) — the only form the
 * Buzz relay accepts in `h` tags for stream kinds (it parses them as `Uuid`). */
export function isUuid(value: string): boolean {
	return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
}

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
			socket.close();
			resolve(result);
		};
		const timer = setTimeout(() => finish({ ok: false, reason: 'transport' }), 10_000);
		const socket = new WebSocket(relayUrl);

		// NIP-42 flow proven against the Buzz relay: publish the event first so
		// the relay answers with an AUTH challenge when authentication is
		// required; authenticate, then re-send the event. An initial REQ probe
		// does NOT provoke a challenge on the Buzz relay, so publishing without
		// auth first would always be rejected with "auth required".
		socket.onopen = () => {
			socket.send(JSON.stringify(['EVENT', signed]));
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
				// An auth-required rejection is the relay demanding NIP-42 auth;
				// the AUTH + re-send above answers it and a later OK for the same
				// event id carries the real outcome.
				if (message[2] === false && typeof message[3] === 'string' && message[3].toLowerCase().includes('auth')) {
					return;
				}
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
