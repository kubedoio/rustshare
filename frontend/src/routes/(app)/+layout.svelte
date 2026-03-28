<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { authStore } from '$lib/stores/auth';
	import { showKeyboardShortcuts } from '$lib/stores/ui';
	import { searchQuery } from '$lib/stores/search';
	import AppShell from '$lib/layout/AppShell.svelte';

	// Check auth on mount
	onMount(() => {
		if (browser) {
			console.log('[Layout] Checking auth state:', $authStore.isAuthenticated);
		}
	});

	// Show search only on files page
	$: showSearch = $page.url.pathname === '/files';

	// Determine sidebar variant based on route
	$: sidebarVariant = $page.url.pathname.startsWith('/files') ? 'files' : 'default';

	// Check if this is the files page (needs full-height layout)
	$: isFilesPage = $page.url.pathname === '/files';

	// Clear search when navigating away from files page
	$: if (!showSearch) {
		searchQuery.set('');
	}

	function handleSearchChange(query: string) {
		searchQuery.set(query);
	}
</script>

<AppShell 
	{showSearch} 
	onSearchChange={showSearch ? handleSearchChange : null}
	sidebarVariant={sidebarVariant}
>
	{#if isFilesPage}
		<!-- Files page uses full-height layout without padding -->
		<div class="h-full min-h-0">
			<slot />
		</div>
	{:else}
		<!-- Other pages use standard padding -->
		<div class="mx-auto w-full max-w-[88rem] p-4 md:p-6 lg:px-8 lg:py-7">
			<slot />
		</div>
	{/if}
</AppShell>
