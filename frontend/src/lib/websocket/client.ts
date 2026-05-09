import { websocketStore } from '$lib/stores/websocket';
import type { WebSocketEvent, WebSocketEventType, EventHandler } from './events';

export class WebSocketClient {
	private ws: WebSocket | null = null;
	private url: string;
	private token: string | null = null;
	private handlers: Map<WebSocketEventType, Set<EventHandler>> = new Map();
	private reconnectAttempts = 0;
	private maxReconnectAttempts = 10;
	private baseReconnectDelay = 1000; // Start with 1 second
	private maxReconnectDelay = 30000; // Max 30 seconds
	private isManualClose = false;
	private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

	constructor(url: string) {
		// Convert http/https to ws/wss
		this.url = url.replace(/^http/, 'ws');
	}

	connect(token?: string | null): Promise<void> {
		return new Promise((resolve, reject) => {
			this.token = token ?? null;
			this.isManualClose = false;

			const wsUrlWithToken = this.token
				? `${this.url}?token=${encodeURIComponent(this.token)}`
				: this.url;

			try {
				websocketStore.setState('connecting');

				this.ws = new WebSocket(wsUrlWithToken);

				this.ws.onopen = () => {
					console.log('[WebSocket] Connected');
					websocketStore.setState('connected');
					websocketStore.resetReconnectAttempts();
					this.reconnectAttempts = 0;
					resolve();
				};

				this.ws.onmessage = (event) => {
					try {
						const data: WebSocketEvent = JSON.parse(event.data);
						this.handleEvent(data);
					} catch (error) {
						console.error('[WebSocket] Failed to parse message:', error);
					}
				};

				this.ws.onerror = (error) => {
					console.error('[WebSocket] Error:', error);
					websocketStore.setError('WebSocket connection error');
				};

				this.ws.onclose = (event) => {
					console.log('[WebSocket] Disconnected', event.code, event.reason);

					if (!this.isManualClose) {
						// Handle different close codes
						if (event.code === 1008 || event.code === 1002) {
							// 1008: Policy Violation (auth failure)
							// 1002: Protocol error
							console.error('[WebSocket] Authentication failed or protocol error');
							websocketStore.setError('WebSocket authentication failed');
							websocketStore.setState('error');
						} else if (this.reconnectAttempts < this.maxReconnectAttempts) {
							// Attempt reconnection with exponential backoff
							this.reconnect();
						} else {
							console.error('[WebSocket] Max reconnection attempts reached');
							websocketStore.setError('Failed to reconnect after multiple attempts');
							websocketStore.setState('error');
						}
					} else {
						websocketStore.setState('disconnected');
					}
				};
			} catch (error) {
				console.error('[WebSocket] Failed to create connection:', error);
				websocketStore.setError('Failed to create WebSocket connection');
				reject(error);
			}
		});
	}

	private reconnect(): void {
		if (this.isManualClose) {
			return;
		}

		this.reconnectAttempts++;
		websocketStore.incrementReconnectAttempts();

		// Exponential backoff: 1s, 2s, 4s, 8s, 16s, 30s (max)
		const delay = Math.min(
			this.baseReconnectDelay * Math.pow(2, this.reconnectAttempts - 1),
			this.maxReconnectDelay
		);

		console.log(
			`[WebSocket] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`
		);

		websocketStore.setState('reconnecting');

		this.reconnectTimer = setTimeout(() => {
			this.connect(this.token).catch((error) => {
				console.error('[WebSocket] Reconnection failed:', error);
			});
		}, delay);
	}

	private handleEvent(event: WebSocketEvent): void {
		// Backend sends 'event_type' field, not 'type'
		const eventType = (event as any).event_type || event.type;

		if (!eventType) {
			console.error('[WebSocket] Event missing event_type field:', event);
			return;
		}

		const handlers = this.handlers.get(eventType);

		if (handlers) {
			handlers.forEach((handler) => {
				try {
					handler(event);
				} catch (error) {
					console.error(`[WebSocket] Handler error for ${eventType}:`, error);
				}
			});
		} else {
			console.warn(`[WebSocket] No handlers registered for event type: ${eventType}`);
		}
	}

	on(eventType: WebSocketEventType, handler: EventHandler): void {
		if (!this.handlers.has(eventType)) {
			this.handlers.set(eventType, new Set());
		}

		this.handlers.get(eventType)!.add(handler);
	}

	off(eventType: WebSocketEventType, handler: EventHandler): void {
		const handlers = this.handlers.get(eventType);

		if (handlers) {
			handlers.delete(handler);
		}
	}

	disconnect(): void {
		this.isManualClose = true;

		// Clear any pending reconnection timer
		if (this.reconnectTimer) {
			clearTimeout(this.reconnectTimer);
			this.reconnectTimer = null;
		}

		if (this.ws) {
			this.ws.close();
			this.ws = null;
		}

		this.token = null;
		websocketStore.reset();
	}

	get isConnected(): boolean {
		return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
	}

	get connectionState(): number | null {
		return this.ws?.readyState ?? null;
	}
}

// Global instance (created when needed)
let wsClient: WebSocketClient | null = null;

export function getWebSocketClient(): WebSocketClient {
	if (!wsClient) {
		const wsUrl = resolveCanonicalWebSocketUrl(
			import.meta.env.VITE_WS_URL,
			import.meta.env.VITE_API_URL
		);
		wsClient = new WebSocketClient(wsUrl);
	}

	return wsClient;
}

function resolveCanonicalWebSocketUrl(rawWsUrl?: string, rawApiUrl?: string): string {
	const base = typeof window !== 'undefined' ? window.location.origin : 'http://localhost:8080';
	const fallback = new URL('/api/ws', base);
	fallback.protocol = fallback.protocol === 'https:' ? 'wss:' : 'ws:';

	const candidates = [rawWsUrl, rawApiUrl, 'http://localhost:8080/api/v1'].filter(
		(value): value is string => typeof value === 'string' && value.trim().length > 0
	);

	for (const candidate of candidates) {
		try {
			const url = new URL(candidate, base);
			url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';

			if (url.pathname.endsWith('/api/ws')) {
				return url.toString();
			}

			if (url.pathname.endsWith('/api/v1')) {
				url.pathname = url.pathname.replace(/\/api\/v1$/, '/api/ws');
				return url.toString();
			}

			if (url.pathname.endsWith('/api')) {
				url.pathname = `${url.pathname}/ws`;
				return url.toString();
			}
		} catch (error) {
			console.warn('[WebSocket] Ignoring invalid URL candidate:', candidate, error);
		}
	}

	return fallback.toString();
}

export function disconnectWebSocket(): void {
	if (wsClient) {
		wsClient.disconnect();
		wsClient = null;
	}
}
