import { authStore } from '$lib/stores/auth';
import { get } from 'svelte/store';

export type WebSocketEventType =
  | 'FileUploaded'
  | 'FileModified'
  | 'FileRenamed'
  | 'FileMoved'
  | 'FileDeleted'
  | 'FileRestored'
  | 'FolderCreated'
  | 'FolderRenamed'
  | 'FolderMoved'
  | 'FolderDeleted'
  | 'ShareCreated'
  | 'ShareRevoked'
  | 'ShareUpdated';

export interface WebSocketEvent {
  event_id: string;
  type: WebSocketEventType;
  aggregate_id: string;
  user_id: string;
  timestamp: string;
  payload: any;
}

export type EventHandler = (event: WebSocketEvent) => void;

export class WebSocketClient {
  private ws: WebSocket | null = null;
  private url: string;
  private handlers: Map<WebSocketEventType, Set<EventHandler>> = new Map();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000; // Start with 1 second
  private isManualClose = false;

  constructor(url: string) {
    // Convert http/https to ws/wss
    this.url = url.replace(/^http/, 'ws');
  }

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      const token = get(authStore);

      if (!token) {
        reject(new Error('No authentication token available'));
        return;
      }

      try {
        // LIMITATION: Browser WebSocket API doesn't support custom headers
        // The backend expects Authorization header, which we cannot set from browser
        // Workaround: Use Sec-WebSocket-Protocol to pass token
        // Backend would need to be modified to extract token from subprotocol
        // For now, this is a known limitation - connection will fail with 401
        //
        // TODO: Modify backend to accept token via:
        // 1. Query parameter: ws://host/api/sync?token=<jwt>
        // 2. Subprotocol: new WebSocket(url, token)
        // 3. First message: send token in first WebSocket message

        this.ws = new WebSocket(this.url);

        this.ws.onopen = () => {
          console.log('[WebSocket] Connected');
          this.reconnectAttempts = 0;
          this.reconnectDelay = 1000;
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
        };

        this.ws.onclose = (event) => {
          console.log('[WebSocket] Disconnected', event.code, event.reason);

          if (!this.isManualClose && this.reconnectAttempts < this.maxReconnectAttempts) {
            // Don't retry if unauthorized (401/403)
            if (event.code !== 1008) {
              this.reconnect();
            }
          }
        };
      } catch (error) {
        reject(error);
      }
    });
  }

  private reconnect(): void {
    this.reconnectAttempts++;
    const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

    console.log(
      `[WebSocket] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`
    );

    setTimeout(() => {
      this.connect().catch((error) => {
        console.error('[WebSocket] Reconnection failed:', error);
      });
    }, delay);
  }

  private handleEvent(event: WebSocketEvent): void {
    const handlers = this.handlers.get(event.type);

    if (handlers) {
      handlers.forEach((handler) => {
        try {
          handler(event);
        } catch (error) {
          console.error(`[WebSocket] Handler error for ${event.type}:`, error);
        }
      });
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

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  get isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }
}

// Global instance (created when needed)
let wsClient: WebSocketClient | null = null;

export function getWebSocketClient(): WebSocketClient {
  if (!wsClient) {
    const baseUrl = import.meta.env.VITE_API_URL || '/api';
    const wsUrl = `${baseUrl}/sync`.replace(/^\/api/, 'http://localhost/api');
    wsClient = new WebSocketClient(wsUrl);
  }

  return wsClient;
}

export function disconnectWebSocket(): void {
  if (wsClient) {
    wsClient.disconnect();
    wsClient = null;
  }
}
