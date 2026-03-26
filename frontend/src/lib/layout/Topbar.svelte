<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { currentUser, authStore } from '$lib/stores/auth';
	import { showKeyboardShortcuts } from '$lib/stores/ui';
	import WebSocketStatus from '$lib/components/common/WebSocketStatus.svelte';

	export let onMenuClick: () => void = () => {};
	export let onSidebarToggle: () => void = () => {};
	export let showSearch = false;
	export let onSearchChange: ((query: string) => void) | null = null;
	export let sidebarCollapsed = false;
	export let hideSidebarToggle = false;

	let searchQuery = '';
	let userMenuOpen = false;

	function handleSearchInput(event: Event) {
		const target = event.target as HTMLInputElement;
		searchQuery = target.value;
		onSearchChange?.(searchQuery);
	}

	function clearSearch() {
		searchQuery = '';
		onSearchChange?.('');
	}

	async function handleLogout() {
		await authStore.logout();
		goto('/login');
	}

	function getInitials(name: string): string {
		return name?.charAt(0).toUpperCase() || '?';
	}

	// Close user menu when clicking outside
	function handleClickOutside(event: MouseEvent) {
		const target = event.target as HTMLElement;
		if (!target.closest('.user-menu-container')) {
			userMenuOpen = false;
		}
	}
</script>

<svelte:window on:click={handleClickOutside} />

<header class="h-14 bg-base-100 border-b border-base-300 flex items-center px-4 gap-3">
	<!-- Left section -->
	<div class="flex items-center gap-2">
		<!-- Mobile menu button -->
		<button
			type="button"
			class="lg:hidden p-2 -ml-2 text-base-content/60 hover:text-base-content hover:bg-base-200 rounded-lg transition-colors"
			on:click={onMenuClick}
			aria-label="Open menu"
		>
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<line x1="4" x2="20" y1="12" y2="12"/>
				<line x1="4" x2="20" y1="6" y2="6"/>
				<line x1="4" x2="20" y1="18" y2="18"/>
			</svg>
		</button>

		<!-- Sidebar toggle (desktop) -->
		{#if !hideSidebarToggle}
			<button
				type="button"
				class="hidden lg:flex p-2 -ml-2 text-base-content/60 hover:text-base-content hover:bg-base-200 rounded-lg transition-colors"
				on:click={onSidebarToggle}
				aria-label={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
			>
				<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
					{#if sidebarCollapsed}
						<path d="m9 18 6-6-6-6"/>
					{:else}
						<path d="m15 18-6-6 6-6"/>
					{/if}
				</svg>
			</button>
		{/if}

		<!-- Page title / breadcrumbs -->
		<div class="hidden sm:flex items-center text-sm">
			<slot name="breadcrumbs" />
		</div>
	</div>

	<!-- Center section - Search -->
	{#if showSearch}
		<div class="flex-1 max-w-xl mx-4">
			<div class="relative group">
				<div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 text-base-content/40 group-focus-within:text-brand-400 transition-colors">
						<circle cx="11" cy="11" r="8"/>
						<path d="m21 21-4.3-4.3"/>
					</svg>
				</div>
				<input
					type="text"
					placeholder="Search files and folders..."
					class="w-full pl-10 pr-10 py-2 bg-base-200 border border-transparent rounded-lg text-sm text-base-content placeholder:text-base-content/40 focus:outline-none focus:bg-base-100 focus:border-brand-500/50 transition-all"
					value={searchQuery}
					on:input={handleSearchInput}
				/>
				{#if searchQuery}
					<button
						type="button"
						class="absolute inset-y-0 right-0 pr-3 flex items-center text-base-content/40 hover:text-base-content transition-colors"
						on:click={clearSearch}
						aria-label="Clear search"
					>
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
							<circle cx="12" cy="12" r="10"/>
							<path d="m15 9-6 6"/>
							<path d="m9 9 6 6"/>
						</svg>
					</button>
				{/if}
			</div>
		</div>
	{:else}
		<div class="flex-1"></div>
	{/if}

	<!-- Right section -->
	<div class="flex items-center gap-1">
		<!-- WebSocket Status -->
		<div class="hidden sm:block">
			<WebSocketStatus />
		</div>

		<!-- Help button -->
		<button
			type="button"
			class="p-2 text-base-content/60 hover:text-base-content hover:bg-base-200 rounded-lg transition-colors"
			on:click={() => showKeyboardShortcuts.set(true)}
			aria-label="Keyboard shortcuts"
			title="Keyboard shortcuts (?)">
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<circle cx="12" cy="12" r="10"/>
				<path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/>
				<path d="M12 17h.01"/>
			</svg>
		</button>

		<!-- User menu -->
		{#if $currentUser}
			<div class="relative user-menu-container ml-2">
				<button
					type="button"
					class="flex items-center gap-2 p-1 pr-3 rounded-lg hover:bg-base-200 transition-colors"
					on:click={() => userMenuOpen = !userMenuOpen}
					aria-expanded={userMenuOpen}
					aria-haspopup="true"
				>
					<div class="w-8 h-8 rounded-lg bg-gradient-to-br from-brand-500 to-brand-600 flex items-center justify-center text-white font-semibold text-sm">
						{getInitials($currentUser.display_name)}
					</div>
					<span class="hidden md:block text-sm font-medium text-base-content/80 truncate max-w-[120px]">
						{$currentUser.display_name}
					</span>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 text-base-content/40">
						<path d="m6 9 6 6 6-6"/>
					</svg>
				</button>

				{#if userMenuOpen}
					<div class="absolute right-0 mt-2 w-56 bg-base-100 rounded-xl shadow-lg shadow-black/20 border border-base-300 py-1 z-50 animate-slide-in-up">
						<div class="px-4 py-3 border-b border-base-200">
							<p class="text-sm font-semibold text-base-content truncate">{$currentUser.display_name}</p>
							<p class="text-xs text-base-content/50 truncate">{$currentUser.email}</p>
						</div>
						
						<nav class="py-1">
							<a href="/profile" class="flex items-center gap-3 px-4 py-2 text-sm text-base-content/80 hover:text-base-content hover:bg-base-200 transition-colors">
								<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
									<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/>
									<circle cx="12" cy="7" r="4"/>
								</svg>
								Profile
							</a>
							<a href="/settings" class="flex items-center gap-3 px-4 py-2 text-sm text-base-content/80 hover:text-base-content hover:bg-base-200 transition-colors">
								<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
									<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>
									<circle cx="12" cy="12" r="3"/>
								</svg>
								Settings
							</a>
							{#if $currentUser.is_admin}
								<a href="/admin" class="flex items-center gap-3 px-4 py-2 text-sm text-base-content/80 hover:text-base-content hover:bg-base-200 transition-colors">
									<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
										<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>
										<circle cx="12" cy="12" r="3"/>
									</svg>
									Admin Panel
								</a>
							{/if}
						</nav>

						<div class="border-t border-base-200 py-1">
							<button
								type="button"
								class="w-full flex items-center gap-3 px-4 py-2 text-sm text-error hover:bg-error/10 transition-colors"
								on:click={handleLogout}
							>
								<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
									<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>
									<polyline points="16 17 21 12 16 7"/>
									<line x1="21" x2="9" y1="12" y2="12"/>
								</svg>
								Sign out
							</button>
						</div>
					</div>
				{/if}
			</div>
		{/if}
	</div>
</header>
