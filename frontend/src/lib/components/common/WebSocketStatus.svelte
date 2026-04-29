<script lang="ts">
	import { browser } from '$app/environment';
	import { onDestroy } from 'svelte';
	import { currentUser } from '$lib/stores/auth';
	import { websocketStore } from '$lib/stores/websocket';
	import { initializeWebSocket } from '$lib/websocket/manager';

	const WEBSOCKET_TOKEN_KEY = 'rustshare.websocket_token';

	let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	let reconnectInFlight = false;

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

	function loadWebSocketToken(): string | null {
		if (!browser) {
			return null;
		}

		return window.sessionStorage.getItem(WEBSOCKET_TOKEN_KEY);
	}

	async function ensureWebSocketConnection(): Promise<void> {
		if (!browser || !$currentUser || reconnectInFlight) {
			return;
		}

		if (state === 'connected' || state === 'connecting' || state === 'reconnecting') {
			return;
		}

		reconnectInFlight = true;

		try {
			await initializeWebSocket(loadWebSocketToken(), $currentUser.id);
		} catch (connectionError) {
			console.error('WebSocketStatus reconnect attempt failed:', connectionError);
		} finally {
			reconnectInFlight = false;
		}
	}

	$: {
		if (reconnectTimer) {
			clearTimeout(reconnectTimer);
			reconnectTimer = null;
		}

		if (browser && $currentUser && (state === 'disconnected' || state === 'error')) {
			const delay = state === 'error' ? 1000 : 200;
			reconnectTimer = setTimeout(() => {
				void ensureWebSocketConnection();
			}, delay);
		}
	}

	onDestroy(() => {
		if (reconnectTimer) {
			clearTimeout(reconnectTimer);
		}
	});
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
