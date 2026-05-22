<script lang="ts">
	import { websocketStore } from '$lib/stores/websocket';

	let state = $derived($websocketStore.state);
	let error = $derived($websocketStore.error);
	let reconnectAttempts = $derived($websocketStore.reconnectAttempts);

	let statusText = $derived(
		(
			{
				disconnected: 'Disconnected',
				connecting: 'Connecting...',
				connected: 'Live',
				reconnecting: `Reconnecting (${reconnectAttempts})...`,
				error: error || 'Connection error'
			} as Record<string, string>
		)[state]
	);

	let statusColor = $derived(
		(
			{
				disconnected: 'bg-gray-500',
				connecting: 'bg-yellow-500',
				connected: 'bg-green-500',
				reconnecting: 'bg-orange-500',
				error: 'bg-red-500'
			} as Record<string, string>
		)[state]
	);

	// Always show indicator so users know the connection status
	const showIndicator = true;
</script>

{#if showIndicator}
	<div class="flex items-center gap-2 text-sm">
		<div class="flex items-center gap-1.5">
			<span class="relative flex h-2 w-2">
				{#if state === 'connecting' || state === 'reconnecting'}
					<span
						class="absolute inline-flex h-full w-full animate-ping rounded-full {statusColor} opacity-75"
					></span>
				{/if}
				<span class="relative inline-flex h-2 w-2 rounded-full {statusColor}"></span>
			</span>
			<span class="text-xs opacity-75">{statusText}</span>
		</div>
	</div>
{/if}
