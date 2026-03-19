import { writable, derived } from 'svelte/store';

export type WebSocketState = 'disconnected' | 'connecting' | 'connected' | 'reconnecting' | 'error';

interface WebSocketStoreState {
  state: WebSocketState;
  error: string | null;
  reconnectAttempts: number;
}

function createWebSocketStore() {
  const { subscribe, set, update } = writable<WebSocketStoreState>({
    state: 'disconnected',
    error: null,
    reconnectAttempts: 0
  });

  return {
    subscribe,
    setState: (state: WebSocketState) => {
      update(s => ({ ...s, state, error: state === 'error' ? s.error : null }));
    },
    setError: (error: string) => {
      update(s => ({ ...s, state: 'error', error }));
    },
    incrementReconnectAttempts: () => {
      update(s => ({ ...s, reconnectAttempts: s.reconnectAttempts + 1 }));
    },
    resetReconnectAttempts: () => {
      update(s => ({ ...s, reconnectAttempts: 0 }));
    },
    reset: () => {
      set({ state: 'disconnected', error: null, reconnectAttempts: 0 });
    }
  };
}

export const websocketStore = createWebSocketStore();

// Derived stores for easy access
export const isWebSocketConnected = derived(
  websocketStore,
  $ws => $ws.state === 'connected'
);

export const websocketConnectionState = derived(
  websocketStore,
  $ws => $ws.state
);
