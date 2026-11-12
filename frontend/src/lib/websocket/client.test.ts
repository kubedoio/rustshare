import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebSocketClient } from './client';
import type { WebSocketEvent, WebSocketEventType } from './events';

describe('WebSocketClient diagnostics', () => {
	afterEach(() => vi.restoreAllMocks());

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
});
