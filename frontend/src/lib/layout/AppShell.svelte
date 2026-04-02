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

	// Props
	interface Props {
		showSearch?: boolean;
		onSearchChange?: ((query: string) => void) | null;
		sidebarVariant?: 'files' | 'default';
		onCreateFolder?: () => void;
	}

	let { 
		showSearch = false, 
		onSearchChange = null,
		sidebarVariant = 'default',
		onCreateFolder = () => {}
	}: Props = $props();

	let checkComplete = $state(false);
	let mobileMenuOpen = $state(false);

	onMount(() => {
		// Any initialization
	});

	$effect(() => {
		if (!$authStore.isLoading) {
			checkComplete = true;
		}
	});

	$effect(() => {
		if (browser && checkComplete && !$authStore.isLoading && !$authStore.isAuthenticated) {
			window.location.href = '/login';
		}
	});

	function toggleMobileMenu() {
		mobileMenuOpen = !mobileMenuOpen;
	}

	function closeMobileMenu() {
		mobileMenuOpen = false;
	}
</script>

{#if checkComplete}
	{#if $authStore.isAuthenticated}
		<div class="flex h-screen bg-base-100 overflow-hidden">
			<!-- Far Left Icon Rail -->
			<LeftRail />

			<!-- Main Layout Area (Topbar + Content Wrapper) -->
			<div class="flex-1 flex flex-col min-w-0">
				<Topbar
					onMenuClick={toggleMobileMenu}
				/>

				<!-- Wrapper for Secondary Sidebar + Main Content -->
				<div class="flex-1 flex min-h-0 min-w-0">
					<!-- Secondary Sidebar (Folder Tree for files variant) -->
					{#if sidebarVariant === 'files'}
						<SidebarNav 
							variant="files"
							mobileOpen={mobileMenuOpen}
							onClose={closeMobileMenu}
							onCreateFolder={onCreateFolder}
						/>
					{/if}

					<main class="flex-1 overflow-auto bg-base-100">
						<slot />
					</main>
				</div>
			</div>
		</div>

		<KeyboardShortcuts
			open={$showKeyboardShortcuts}
			on:close={() => showKeyboardShortcuts.set(false)}
		/>

		<ToastContainer />
	{:else}
		<!-- Redirecting... -->
		<div class="flex items-center justify-center h-screen bg-base-100">
			<div class="flex flex-col items-center gap-4">
				<div class="animate-spin h-8 w-8 border-2 border-brand-500 border-t-transparent rounded-full"></div>
				<span class="text-sm text-base-content/60">Redirecting to login...</span>
			</div>
		</div>
	{/if}
{:else}
	<div class="flex items-center justify-center h-screen bg-base-100">
		<div class="flex flex-col items-center gap-4">
			<div class="animate-spin h-8 w-8 border-2 border-brand-500 border-t-transparent rounded-full"></div>
			<span class="text-sm text-base-content/60">Loading...</span>
		</div>
	</div>
{/if}
