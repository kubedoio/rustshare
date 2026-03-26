<script lang="ts">
	import { browser } from '$app/environment';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { authStore } from '$lib/stores/auth';
	import { showKeyboardShortcuts } from '$lib/stores/ui';
	import LeftRail from './LeftRail.svelte';
	import SidebarNav from './SidebarNav.svelte';
	import Topbar from './Topbar.svelte';
	import KeyboardShortcuts from '$lib/components/common/KeyboardShortcuts.svelte';
	import ToastContainer from '$lib/components/common/ToastContainer.svelte';

	export let showSearch = false;
	export let onSearchChange: ((query: string) => void) | null = null;
	export let sidebarVariant: 'files' | 'default' = 'default';

	let checkComplete = false;
	let mobileMenuOpen = false;
	let sidebarCollapsed = false;

	// Check if we're on the files page - hide secondary sidebar there
	$: isFilesPage = $page.url.pathname === '/files';

	onMount(() => {
		// Check if sidebar should be collapsed (saved preference)
		if (browser) {
			const saved = localStorage.getItem('sidebar-collapsed');
			sidebarCollapsed = saved === 'true';
		}
	});

	$: if (!$authStore.isLoading) {
		checkComplete = true;
	}

	$: if (browser && checkComplete && !$authStore.isLoading && !$authStore.isAuthenticated) {
		window.location.href = '/login';
	}

	function toggleMobileMenu() {
		mobileMenuOpen = !mobileMenuOpen;
	}

	function closeMobileMenu() {
		mobileMenuOpen = false;
	}

	function toggleSidebar() {
		sidebarCollapsed = !sidebarCollapsed;
		if (browser) {
			localStorage.setItem('sidebar-collapsed', String(sidebarCollapsed));
		}
	}
</script>

{#if checkComplete}
	{#if $authStore.isAuthenticated}
		<div class="flex h-screen bg-base-100 overflow-hidden">
			<!-- Far Left Icon Rail -->
			<LeftRail />

			<!-- Secondary Sidebar - Hidden on files page -->
			{#if !isFilesPage}
				<SidebarNav 
					variant={sidebarVariant}
					collapsed={sidebarCollapsed}
					mobileOpen={mobileMenuOpen}
					onClose={closeMobileMenu}
				/>
			{/if}

			<!-- Main Content Area -->
			<div class="flex-1 flex flex-col min-w-0">
				<Topbar
					onMenuClick={toggleMobileMenu}
					onSidebarToggle={toggleSidebar}
					{showSearch}
					{onSearchChange}
					sidebarCollapsed={sidebarCollapsed}
					hideSidebarToggle={isFilesPage}
				/>

				<main class="flex-1 overflow-auto bg-base-100">
					<slot />
				</main>
			</div>
		</div>

		<KeyboardShortcuts
			open={$showKeyboardShortcuts}
			on:close={() => showKeyboardShortcuts.set(false)}
		/>

		<ToastContainer />
	{:else}
		<!-- Redirecting... -->
	{/if}
{:else}
	<div class="flex items-center justify-center h-screen bg-base-100">
		<div class="flex flex-col items-center gap-4">
			<div class="animate-spin h-8 w-8 border-2 border-brand-500 border-t-transparent rounded-full"></div>
			<span class="text-sm text-base-content/60">Loading...</span>
		</div>
	</div>
{/if}
