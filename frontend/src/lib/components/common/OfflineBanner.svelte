<script lang="ts">
	import { WifiOff } from 'lucide-svelte';

	const HEALTH_TIMEOUT_MS = 5000;

	let isOnline = $state(typeof navigator !== 'undefined' ? navigator.onLine : true);

	async function checkBackendHealth(abortSignal?: AbortSignal): Promise<boolean> {
		if (typeof fetch === 'undefined') return true;
		try {
			const response = await fetch('/health', {
				method: 'GET',
				signal: abortSignal,
				credentials: 'include'
			});
			return response.ok;
		} catch {
			return false;
		}
	}

	async function verifyConnectivity() {
		const controller = new AbortController();
		const timeout = setTimeout(() => controller.abort(), HEALTH_TIMEOUT_MS);
		const online = await checkBackendHealth(controller.signal);
		clearTimeout(timeout);
		isOnline = online;
	}

	function handleOnline() {
		isOnline = true;
		verifyConnectivity();
	}

	function handleOffline() {
		isOnline = false;
		verifyConnectivity();
	}

	$effect(() => {
		verifyConnectivity();
	});
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
