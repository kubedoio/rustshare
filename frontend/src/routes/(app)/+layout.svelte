<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { authStore } from '$lib/stores/auth';
	import { showKeyboardShortcuts } from '$lib/stores/ui';
	import { searchQuery } from '$lib/stores/search';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import { userModulePreferences } from '$lib/stores/userModulePreferences';
	import AppShell from '$lib/layout/AppShell.svelte';

	// Check auth on mount
	onMount(() => {
		if (browser) {
			console.log('[Layout] Checking auth state:', $authStore.isAuthenticated);
			userModulePreferences.load();
		}
	});

	// SvelteKit layout children snippet
	let { children } = $props();

	// Show search only on files page
	let showSearch = $derived($page.url.pathname === '/files');

	// Determine sidebar variant based on route
	let sidebarVariant: 'files' | 'default' = $derived(
		$page.url.pathname.startsWith('/files') ? 'files' : 'default'
	);

	// Check if this is the files page (needs full-height layout)
	let isFilesPage = $derived($page.url.pathname === '/files');

	// Sync search query with store
	$effect(() => {
		if (showSearch && $searchQuery !== $fileBrowserUi.searchQuery) {
			// Two-way sync would go here if needed
		}
	});

	// Clear search when navigating away from files page
	$effect(() => {
		if (!showSearch) {
			searchQuery.set('');
		}
	});

	function handleSearchChange(query: string) {
		searchQuery.set(query);
		fileBrowserUi.setSearchQuery(query);
	}

	function handleCreateFolder() {
		// Dispatch a custom event that the files page can listen to
		if (browser) {
			window.dispatchEvent(new CustomEvent('create-folder-requested'));
		}
	}
</script>

<AppShell
	{showSearch}
	onSearchChange={showSearch ? handleSearchChange : null}
	{sidebarVariant}
	onCreateFolder={handleCreateFolder}
>
	{#if isFilesPage}
		<!-- Files page uses full-height layout without padding -->
		<div class="h-full min-h-0">
			{@render children?.()}
		</div>
	{:else}
		<!-- Other pages use standard padding -->
		<div class="mx-auto w-full max-w-[88rem] p-4 md:p-6 lg:px-8 lg:py-7">
			{@render children?.()}
		</div>
	{/if}
</AppShell>
