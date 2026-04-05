<script lang="ts">
	/**
	 * ==============================================================================
	 * LEGACY SHARED-WITH-ME REDIRECT
	 * ==============================================================================
	 * 
	 * This page exists only as a compatibility redirect.
	 * Per SPEC section 1.3.F and 1.5:
	 * - Old /shared route redirects into the unified explorer state
	 * - After redirect, there is no separate Shared controller logic
	 * - The redirect lands at /files?root=shared
	 */
	
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';

	onMount(() => {
		if (browser) {
			// Redirect to the unified explorer with shared root
			goto('/files?root=shared', { replaceState: true });
		}
	});
</script>

<svelte:head>
	<title>Redirecting... - RustShare</title>
</svelte:head>

<div class="flex h-full items-center justify-center">
	<div class="text-center">
		<div class="inline-block animate-spin rounded-full h-8 w-8 border-2 border-brand-500 border-t-transparent mb-4"></div>
		<p class="text-base-content/60">Redirecting to Shared...</p>
	</div>
</div>
