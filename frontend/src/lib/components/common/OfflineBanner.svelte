<script lang="ts">
	import { WifiOff } from 'lucide-svelte';

	let isOnline = $state(typeof navigator !== 'undefined' ? navigator.onLine : true);

	function handleOnline() {
		isOnline = true;
	}

	function handleOffline() {
		isOnline = false;
	}
</script>

<svelte:window ononline={handleOnline} onoffline={handleOffline} />

{#if !isOnline}
	<div
		class="flex items-center justify-center gap-2 border-b border-warning/20 bg-warning/10 px-4 py-2 text-sm font-medium text-warning"
		role="status"
		aria-live="polite"
	>
		<WifiOff size={16} />
		<span>You are offline. Some features may be unavailable until your connection returns.</span>
	</div>
{/if}
