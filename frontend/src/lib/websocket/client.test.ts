import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebSocketClient } from './client';
import type { WebSocketEvent, WebSocketEventType } from './events';

describe('WebSocketClient diagnostics', () => {
	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it('does not log malformed event payloads or interpret attacker text as a format string', () => {
		const error = vi.spyOn(console, 'error').mockImplementation(() => {});
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const client = new WebSocketClient('http://localhost');
		const handleEvent = (client as unknown as { handleEvent(event: WebSocketEvent): void })
			.handleEvent;
		const invoke = handleEvent.bind(client);
		const untrustedEventType = 'chat\nforged %s';

		invoke({ content: 'private message' } as unknown as WebSocketEvent);
		invoke({ event_type: untrustedEventType } as unknown as WebSocketEvent);
		client.on('chat.message' as WebSocketEventType, () => {
			throw new Error('private message');
		});
		invoke({ event_type: 'chat.message', content: 'private message' } as unknown as WebSocketEvent);

		expect(error.mock.calls[0]).toEqual(['[WebSocket] Event missing event_type field']);
		expect(warn.mock.calls[0]).toEqual(['[WebSocket] No handler registered for event']);
		expect(error.mock.calls[1]).toEqual(['[WebSocket] Handler failed']);
		const calls = JSON.stringify([...error.mock.calls, ...warn.mock.calls]);
		expect(calls).not.toContain('private message');
		expect(calls).not.toContain(untrustedEventType);
	});

	it('does not log WebSocket payloads, error objects, close reasons, or credential URLs', async () => {
		const error = vi.spyOn(console, 'error').mockImplementation(() => {});
		const debug = vi.spyOn(console, 'debug').mockImplementation(() => {});
		const callbacks = {
			onMessage: null as ((event: { data: string }) => void) | null,
			onError: null as ((event: unknown) => void) | null,
			onClose: null as ((event: { code: number; reason: string }) => void) | null
		};
		class FakeWebSocket {
			static CONNECTING = 0;
			static OPEN = 1;
			static CLOSING = 2;
			static CLOSED = 3;
			readyState = FakeWebSocket.CONNECTING;
			onopen: (() => void) | null = null;
			get onmessage() {
				return callbacks.onMessage;
			}
			set onmessage(handler: typeof callbacks.onMessage) {
				callbacks.onMessage = handler;
			}
			get onerror() {
				return callbacks.onError;
			}
			set onerror(handler: typeof callbacks.onError) {
				callbacks.onError = handler;
			}
			get onclose() {
				return callbacks.onClose;
			}
			set onclose(handler: typeof callbacks.onClose) {
				callbacks.onClose = handler;
			}
			constructor(_url: string) {}
		}
		vi.stubGlobal('WebSocket', FakeWebSocket);

		const secret = 'sensitive-token-and-message';
		const client = new WebSocketClient('https://chat.example/ws');
		const connection = client.connect(secret).catch(() => {});
		callbacks.onMessage?.({ data: `not-json ${secret}` });
		callbacks.onError?.({ message: secret });
		callbacks.onClose?.({ code: 1008, reason: secret });
		await connection;

		const calls = JSON.stringify([...error.mock.calls, ...debug.mock.calls]);
		expect(calls).not.toContain(secret);
		expect(error.mock.calls).toContainEqual(['[WebSocket] Failed to parse message']);
		expect(error.mock.calls).toContainEqual(['[WebSocket] Connection error']);
		expect(debug.mock.calls).toContainEqual(['[WebSocket] Disconnected']);
	});

	it('does not log credentials when WebSocket construction fails', async () => {
		const error = vi.spyOn(console, 'error').mockImplementation(() => {});
		const secret = 'sensitive-token';
		class ThrowingWebSocket {
			static CONNECTING = 0;
			static OPEN = 1;
			static CLOSING = 2;
			static CLOSED = 3;
			constructor(_url: string) {
				throw new Error(secret);
			}
		}
		vi.stubGlobal('WebSocket', ThrowingWebSocket);

		await expect(new WebSocketClient('https://chat.example/ws').connect(secret)).rejects.toThrow(
			'Failed to create WebSocket connection'
		);
		expect(JSON.stringify(error.mock.calls)).not.toContain(secret);
		expect(error.mock.calls).toContainEqual(['[WebSocket] Failed to create connection']);
	});
});
