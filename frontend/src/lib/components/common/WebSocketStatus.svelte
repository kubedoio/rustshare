<script lang="ts">
  import { websocketStore } from '$lib/stores/websocket';

  $: state = $websocketStore.state;
  $: error = $websocketStore.error;
  $: reconnectAttempts = $websocketStore.reconnectAttempts;

  $: statusText = {
    disconnected: 'Disconnected',
    connecting: 'Connecting...',
    connected: 'Live',
    reconnecting: `Reconnecting (${reconnectAttempts})...`,
    error: error || 'Connection error'
  }[state];

  $: statusColor = {
    disconnected: 'bg-gray-500',
    connecting: 'bg-yellow-500',
    connected: 'bg-green-500',
    reconnecting: 'bg-orange-500',
    error: 'bg-red-500'
  }[state];

  // Always show indicator so users know the connection status
  $: showIndicator = true;
</script>

{#if showIndicator}
  <div class="flex items-center gap-2 text-sm">
    <div class="flex items-center gap-1.5">
      <span class="relative flex h-2 w-2">
        {#if state === 'connecting' || state === 'reconnecting'}
          <span class="animate-ping absolute inline-flex h-full w-full rounded-full {statusColor} opacity-75"></span>
        {/if}
        <span class="relative inline-flex rounded-full h-2 w-2 {statusColor}"></span>
      </span>
      <span class="text-xs opacity-75">{statusText}</span>
    </div>
  </div>
{/if}
