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
		<!-- Files page needs full-height layout -->
		<div class="h-[calc(100vh-3.5rem)]">
			<slot />
		</div>
	{:else}
		<!-- Other pages use standard padding -->
		<div class="p-4 lg:p-6 max-w-7xl mx-auto">
			<slot />
		</div>
	{/if}
</AppShell>
