// Example: Using WebSocket in Custom Components
// This file shows how to use the WebSocket functionality in your own components

import { onMount, onDestroy } from 'svelte';
import { getWebSocketClient } from '$lib/websocket/client';
import { toastStore } from '$lib/stores/toast';
import { websocketStore, isWebSocketConnected } from '$lib/stores/websocket';
import type { WebSocketEvent } from '$lib/websocket/events';

// ============================================================================
// Example 1: Listen to specific file events in a component
// ============================================================================

export function Example1_FileWatcher() {
  const wsClient = getWebSocketClient();

  // Handler function
  const handleFileUploaded = (event: WebSocketEvent) => {
    const payload = event.payload as any;
    console.log('File uploaded:', payload.file_name);

    // Do something with the event
    // e.g., update local state, show custom notification, etc.
  };

  onMount(() => {
    // Register handler when component mounts
    wsClient.on('FileUploaded', handleFileUploaded);
  });

  onDestroy(() => {
    // Clean up handler when component unmounts
    wsClient.off('FileUploaded', handleFileUploaded);
  });
}

// ============================================================================
// Example 2: Show custom toast notification
// ============================================================================

export function Example2_CustomToast() {
  function showCustomNotification() {
    // Show info notification (auto-dismiss after 3s)
    toastStore.show('Operation completed successfully', 'info');

    // Show success notification
    toastStore.show('File uploaded!', 'success');

    // Show error notification
    toastStore.show('Something went wrong', 'error');

    // Show notification with custom duration (5 seconds)
    toastStore.show('This will stay for 5 seconds', 'info', 5000);

    // Show persistent notification (manual dismiss only)
    const id = toastStore.show('Click X to dismiss', 'info', 0);

    // Dismiss programmatically after 10 seconds
    setTimeout(() => toastStore.dismiss(id), 10000);
  }
}

// ============================================================================
// Example 3: Monitor WebSocket connection state
// ============================================================================

export function Example3_ConnectionMonitor() {
  // Subscribe to connection state
  const unsubscribe = websocketStore.subscribe(state => {
    console.log('WebSocket state:', state.state);
    console.log('Reconnect attempts:', state.reconnectAttempts);
    console.log('Error:', state.error);
  });

  // Or use derived store
  const unsubscribe2 = isWebSocketConnected.subscribe(connected => {
    console.log('Connected:', connected);
  });

  // Clean up subscriptions
  onDestroy(() => {
    unsubscribe();
    unsubscribe2();
  });
}

// ============================================================================
// Example 4: Listen to multiple event types
// ============================================================================

export function Example4_MultipleEventListener() {
  const wsClient = getWebSocketClient();

  const handleFileEvent = (event: WebSocketEvent) => {
    switch (event.type) {
      case 'FileUploaded':
        console.log('File uploaded');
        break;
      case 'FileModified':
        console.log('File modified');
        break;
      case 'FileDeleted':
        console.log('File deleted');
        break;
    }
  };

  onMount(() => {
    // Register same handler for multiple events
    wsClient.on('FileUploaded', handleFileEvent);
    wsClient.on('FileModified', handleFileEvent);
    wsClient.on('FileDeleted', handleFileEvent);
  });

  onDestroy(() => {
    wsClient.off('FileUploaded', handleFileEvent);
    wsClient.off('FileModified', handleFileEvent);
    wsClient.off('FileDeleted', handleFileEvent);
  });
}

// ============================================================================
// Example 5: Custom notification with filtering
// ============================================================================

export function Example5_FilteredNotifications() {
  const wsClient = getWebSocketClient();
  const currentUserId = 'current-user-id'; // Get from auth store

  const handleFileUploaded = (event: WebSocketEvent) => {
    // Only show notification if it's from another user
    if (event.user_id !== currentUserId) {
      const payload = event.payload as any;
      toastStore.show(`${payload.file_name} was uploaded by another user`, 'info');
    } else {
      // This is our own upload, maybe update UI differently
      console.log('Own file uploaded');
    }
  };

  onMount(() => {
    wsClient.on('FileUploaded', handleFileUploaded);
  });

  onDestroy(() => {
    wsClient.off('FileUploaded', handleFileUploaded);
  });
}

// ============================================================================
// Example 6: Using WebSocket status in Svelte component template
// ============================================================================

/*
<script lang="ts">
  import { websocketStore, isWebSocketConnected } from '$lib/stores/websocket';

  $: state = $websocketStore.state;
  $: connected = $isWebSocketConnected;
</script>

{#if !connected}
  <div class="alert alert-warning">
    <span>You're offline. Changes won't sync in real-time.</span>
  </div>
{/if}

{#if state === 'reconnecting'}
  <div class="alert alert-info">
    <span>Reconnecting... (Attempt {$websocketStore.reconnectAttempts})</span>
  </div>
{/if}

{#if state === 'error'}
  <div class="alert alert-error">
    <span>Connection error: {$websocketStore.error}</span>
  </div>
{/if}
*/

// ============================================================================
// Example 7: Manually trigger WebSocket connection (advanced)
// ============================================================================

export function Example7_ManualConnection() {
  // Note: Usually you don't need to do this manually
  // The auth store handles connection automatically

  import { initializeWebSocket, cleanupWebSocket } from '$lib/websocket/manager';

  async function connectManually() {
    const token = 'your-jwt-token';
    const userId = 'your-user-id';

    try {
      await initializeWebSocket(token, userId);
      console.log('WebSocket connected');
    } catch (error) {
      console.error('Failed to connect:', error);
    }
  }

  function disconnectManually() {
    cleanupWebSocket();
    console.log('WebSocket disconnected');
  }
}

// ============================================================================
// Example 8: Check WebSocket connection status
// ============================================================================

export function Example8_CheckConnection() {
  const wsClient = getWebSocketClient();

  // Check if connected
  const connected = wsClient.isConnected;
  console.log('Connected:', connected);

  // Get raw WebSocket state
  // 0 = CONNECTING, 1 = OPEN, 2 = CLOSING, 3 = CLOSED
  const state = wsClient.connectionState;
  console.log('Connection state:', state);
}
