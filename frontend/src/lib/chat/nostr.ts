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
	| { ok: true; event_id: string; detail?: string }
	| { ok: false; reason: 'transport' }
	| { ok: false; reason: 'uncertain'; event_id: string; event: NostrEvent }
	| { ok: false; reason: 'rejected'; detail?: string };

/** An EVENT without a definitive ACK is uncertain, never safe to re-sign. */
export async function publishEvent(
	relayUrl: string,
	unsigned: Omit<NostrEvent, 'id' | 'sig'>,
	secretKey: string,
	onSigned?: (event: NostrEvent) => void
): Promise<PublishResult> {
	const event = await signEvent(unsigned, secretKey);
	onSigned?.(event);
	return publishSignedEvent(relayUrl, event, secretKey);
}

/** Retry the original signed bytes; NIP-42 authentication uses a separate event. */
export async function publishSignedEvent(
	relayUrl: string,
	signed: NostrEvent,
	secretKey: string
): Promise<PublishResult> {
	if (signed.pubkey !== publicKeyOf(secretKey) || !(await verifyEvent(signed))) {
		return { ok: false, reason: 'rejected', detail: 'Invalid event or signing identity mismatch' };
	}
	return new Promise<PublishResult>((resolve) => {
		let settled = false;
		let sent = false;
		let authId: string | undefined;
		let authStarted = false;
		let preAuthRejection: string | undefined;
		let socket: WebSocket;
		const finish = (result: PublishResult) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			socket?.close();
			resolve(result);
		};
		const unavailable = () =>
			finish(
				sent
					? { ok: false, reason: 'uncertain', event_id: signed.id, event: signed }
					: { ok: false, reason: 'transport' }
			);
		const send = () => {
			if (settled) return;
			socket.send(JSON.stringify(['EVENT', signed]));
			sent = true;
		};
		const timer = setTimeout(unavailable, 10_000);
		try {
			socket = new WebSocket(relayUrl);
		} catch {
			unavailable();
			return;
		}
		socket.onopen = () => {
			try {
				send();
			} catch {
				unavailable();
			}
		};
		socket.onmessage = (raw) => {
			void (async () => {
				if (settled) return;
				let message: unknown;
				try {
					message = JSON.parse(String(raw.data));
				} catch {
					return;
				}
				if (!Array.isArray(message)) return;
				if (message[0] === 'AUTH' && typeof message[1] === 'string' && !authStarted) {
					authStarted = true;
					const auth = await signEvent(
						await buildUnsignedEvent(
							NOSTR_KIND_AUTH,
							'',
							[
								['relay', relayUrl],
								['challenge', message[1]]
							],
							signed.pubkey
						),
						secretKey
					);
					if (settled) return;
					authId = auth.id;
					socket.send(JSON.stringify(['AUTH', auth]));
					return;
				}
				if (message[0] !== 'OK' || typeof message[2] !== 'boolean') return;
				const detail = typeof message[3] === 'string' ? message[3] : undefined;
				// authId is unset until our AUTH is sent, so this matches only the
				// relay's OK for that event — never an unrelated or id-less frame.
				if (typeof authId === 'string' && message[1] === authId) {
					if (message[2]) send();
					else finish({ ok: false, reason: 'rejected', detail });
					return;
				}
				if (message[1] !== signed.id) return;
				if (message[2]) {
					finish({ ok: true, event_id: signed.id, detail });
				} else if (
					detail &&
					/auth[- ]required|authentication required|not authenticated/i.test(detail) &&
					!preAuthRejection
				) {
					// ACKs have no attempt ID: a delayed pre-auth denial cannot deny a resend.
					preAuthRejection = detail;
				} else {
					finish({ ok: false, reason: 'rejected', detail });
				}
			})().catch(unavailable);
		};
		socket.onerror = unavailable;
		socket.onclose = unavailable;
	});
}

/** Verify the canonical ID as well as the Schnorr signature at the read boundary. */
export async function verifyEvent(value: unknown): Promise<boolean> {
	try {
		const e = value as NostrEvent;
		return (
			typeof e.id === 'string' &&
			/^[0-9a-f]{64}$/.test(e.id) &&
			typeof e.pubkey === 'string' &&
			/^[0-9a-f]{64}$/.test(e.pubkey) &&
			typeof e.sig === 'string' &&
			/^[0-9a-f]{128}$/.test(e.sig) &&
			Number.isSafeInteger(e.created_at) &&
			e.created_at >= 0 &&
			Number.isSafeInteger(e.kind) &&
			e.kind >= 0 &&
			typeof e.content === 'string' &&
			Array.isArray(e.tags) &&
			e.tags.every((t) => Array.isArray(t) && t.every((v) => typeof v === 'string')) &&
			e.id === (await sha256Hex(serializeForId(e))) &&
			schnorr.verify(hexToBytes(e.sig), hexToBytes(e.id), hexToBytes(e.pubkey))
		);
	} catch {
		return false;
	}
}

export interface RelayEventFilter {
	kinds: number[];
	'#d'?: string[];
	'#h'?: string[];
	'#e'?: string[];
	search?: string;
	ids?: string[];
	limit?: number;
}

export type RelayQueryResult =
	| { ok: true; events: NostrEvent[] }
	| { ok: false; reason: 'transport' | 'rejected' | 'invalid'; detail?: string };

/** Subject-authenticated historical read. Snapshots require the trusted relay signer. */
export async function queryRelayEvents(
	relayUrl: string,
	filter: RelayEventFilter,
	secretKey: string,
	expectedRelayPubkey: string
): Promise<RelayQueryResult> {
	if (
		!/^[0-9a-f]{64}$/.test(expectedRelayPubkey) ||
		!filter.kinds.length ||
		filter.kinds.some((k) => !Number.isSafeInteger(k) || k < 0) ||
		(filter.limit !== undefined && (!Number.isSafeInteger(filter.limit) || filter.limit < 1))
	) {
		return { ok: false, reason: 'invalid' };
	}
	const limit = Math.min(Math.max(filter.limit ?? 1000, 1), 1000);
	const pubkey = publicKeyOf(secretKey);
	return new Promise((resolve) => {
		let socket: WebSocket;
		let settled = false;
		let authStarted = false;
		let authId: string | undefined;
		let authenticated = false;
		let received = 0;
		let frames = 0;
		const subscription = crypto.randomUUID();
		const events = new Map<string, NostrEvent>();
		const finish = (result: RelayQueryResult) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			socket?.close();
			resolve(result);
		};
		const unavailable = () => finish({ ok: false, reason: 'transport' });
		const request = () => socket.send(JSON.stringify(['REQ', subscription, { ...filter, limit }]));
		const timer = setTimeout(unavailable, 10_000);
		try {
			socket = new WebSocket(relayUrl);
		} catch {
			unavailable();
			return;
		}
		// Buzz issues its challenge proactively; the initial REQ also supports other NIP-42 relays.
		socket.onopen = () => {
			try {
				request();
			} catch {
				unavailable();
			}
		};
		let processing = Promise.resolve();
		socket.onmessage = (raw) => {
			if (settled) return;
			if (++frames > 1100 || String(raw.data).length > 1_048_576) {
				finish({ ok: false, reason: 'invalid' });
				return;
			}
			processing = processing
				.then(async () => {
					if (settled) return;
					let frame: unknown;
					try {
						frame = JSON.parse(String(raw.data));
					} catch {
						return;
					}
					if (!Array.isArray(frame)) return;
					if (frame[0] === 'AUTH' && typeof frame[1] === 'string' && !authStarted) {
						authStarted = true;
						const auth = await signEvent(
							await buildUnsignedEvent(
								NOSTR_KIND_AUTH,
								'',
								[
									['relay', relayUrl],
									['challenge', frame[1]]
								],
								pubkey
							),
							secretKey
						);
						if (settled) return;
						authId = auth.id;
						socket.send(JSON.stringify(['AUTH', auth]));
					} else if (frame[0] === 'OK' && typeof authId === 'string' && frame[1] === authId) {
						if (frame[2] !== true)
							finish({
								ok: false,
								reason: 'rejected',
								detail: typeof frame[3] === 'string' ? frame[3] : undefined
							});
						else {
							authenticated = true;
							request();
						}
					} else if (frame[0] === 'CLOSED' && frame[1] === subscription) {
						const detail = typeof frame[2] === 'string' ? frame[2] : undefined;
						if (!authenticated && detail?.toLowerCase().includes('auth')) return;
						finish({ ok: false, reason: 'rejected', detail });
					} else if (frame[0] === 'EVENT' && frame[1] === subscription && authenticated) {
						const event = frame[2] as NostrEvent;
						if (
							++received > limit ||
							!(await verifyEvent(event)) ||
							!filter.kinds.includes(event.kind) ||
							(event.kind >= 39000 &&
								event.kind <= 39003 &&
								event.pubkey !== expectedRelayPubkey) ||
							(filter.ids && !filter.ids.includes(event.id)) ||
							(filter['#d'] &&
								!event.tags.some((t) => t[0] === 'd' && filter['#d']!.includes(t[1]))) ||
							(filter['#h'] &&
								!event.tags.some((t) => t[0] === 'h' && filter['#h']!.includes(t[1]))) ||
							(filter['#e'] &&
								!event.tags.some((t) => t[0] === 'e' && filter['#e']!.includes(t[1])))
						) {
							finish({ ok: false, reason: 'invalid' });
							return;
						}
						events.set(event.id, event);
					} else if (frame[0] === 'EOSE' && frame[1] === subscription && authenticated) {
						finish({ ok: true, events: [...events.values()] });
					}
				})
				.catch(unavailable);
		};
		socket.onerror = unavailable;
		socket.onclose = unavailable;
	});
}
