<script lang="ts">
	import { goto } from '$app/navigation';
	import ThemeToggle from '$lib/components/common/ThemeToggle.svelte';
	import WebSocketStatus from '$lib/components/common/WebSocketStatus.svelte';
	import { currentUser, authStore } from '$lib/stores/auth';
	import { showKeyboardShortcuts } from '$lib/stores/ui';

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

	function handleClickOutside(event: MouseEvent) {
		const target = event.target as HTMLElement;
		if (!target.closest('.user-menu-container')) {
			userMenuOpen = false;
		}
	}
</script>

<svelte:window on:click={handleClickOutside} />

<header
	class="relative z-[90] flex h-16 items-center gap-3 border-b border-base-300/80 bg-base-100/90 px-4 backdrop-blur-xl lg:px-6"
>
	<div class="flex items-center gap-2">
		<button
			type="button"
			class="-ml-2 rounded-xl border border-transparent p-2 text-base-content/60 transition-colors hover:border-base-300/80 hover:bg-base-200/80 hover:text-base-content lg:hidden"
			on:click={onMenuClick}
			aria-label="Open menu"
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="2"
				stroke-linecap="round"
				stroke-linejoin="round"
				class="h-5 w-5"
			>
				<line x1="4" x2="20" y1="12" y2="12" />
				<line x1="4" x2="20" y1="6" y2="6" />
				<line x1="4" x2="20" y1="18" y2="18" />
			</svg>
		</button>

		{#if !hideSidebarToggle}
			<button
				type="button"
				class="hidden rounded-xl border border-transparent p-2 text-base-content/60 transition-colors hover:border-base-300/80 hover:bg-base-200/80 hover:text-base-content lg:flex"
				on:click={onSidebarToggle}
				aria-label={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
			>
				<svg
					xmlns="http://www.w3.org/2000/svg"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-linecap="round"
					stroke-linejoin="round"
					class="h-5 w-5"
				>
					{#if sidebarCollapsed}
						<path d="m9 18 6-6-6-6" />
					{:else}
						<path d="m15 18-6-6 6-6" />
					{/if}
				</svg>
			</button>
		{/if}

		<div class="hidden items-center text-sm text-base-content/65 sm:flex">
			<slot name="breadcrumbs" />
		</div>
	</div>

	{#if showSearch}
		<div class="mx-4 max-w-xl flex-1">
			<div class="group relative">
				<div class="pointer-events-none absolute inset-y-0 left-0 flex items-center pl-3">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
						class="h-4 w-4 text-base-content/35 transition-colors group-focus-within:text-brand-500"
					>
						<circle cx="11" cy="11" r="8" />
						<path d="m21 21-4.3-4.3" />
					</svg>
				</div>
				<input
					type="text"
					placeholder="Search files and folders..."
					class="w-full rounded-xl border border-base-300/80 bg-base-100 px-10 py-2.5 text-sm text-base-content shadow-sm shadow-black/5 transition-all placeholder:text-base-content/35 focus:border-brand-500/40 focus:outline-none focus:ring-4 focus:ring-brand-500/10"
					value={searchQuery}
					on:input={handleSearchInput}
				/>
				{#if searchQuery}
					<button
						type="button"
						class="absolute inset-y-0 right-0 flex items-center pr-3 text-base-content/35 transition-colors hover:text-base-content"
						on:click={clearSearch}
						aria-label="Clear search"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
							class="h-4 w-4"
						>
							<circle cx="12" cy="12" r="10" />
							<path d="m15 9-6 6" />
							<path d="m9 9 6 6" />
						</svg>
					</button>
				{/if}
			</div>
		</div>
	{:else}
		<div class="flex-1"></div>
	{/if}

	<div class="ml-auto flex items-center gap-2">
		<div class="hidden rounded-xl border border-base-300/70 bg-base-100 px-2.5 py-1.5 sm:block">
			<WebSocketStatus />
		</div>

		<ThemeToggle />

		<button
			type="button"
			class="rounded-xl border border-base-300/80 bg-base-100 p-2 text-base-content/60 transition-colors hover:border-brand-500/20 hover:bg-base-200 hover:text-base-content"
			on:click={() => showKeyboardShortcuts.set(true)}
			aria-label="Keyboard shortcuts"
			title="Keyboard shortcuts (?)"
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="2"
				stroke-linecap="round"
				stroke-linejoin="round"
				class="h-5 w-5"
			>
				<circle cx="12" cy="12" r="10" />
				<path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
				<path d="M12 17h.01" />
			</svg>
		</button>

		{#if $currentUser}
			<div class="user-menu-container relative ml-2">
				<button
					type="button"
					class="flex items-center gap-2 rounded-xl border border-base-300/80 bg-base-100 p-1 pr-3 transition-colors hover:border-brand-500/20 hover:bg-base-200"
					on:click={() => (userMenuOpen = !userMenuOpen)}
					aria-expanded={userMenuOpen}
					aria-haspopup="true"
				>
					<div
						class="flex h-8 w-8 items-center justify-center rounded-xl bg-gradient-to-br from-brand-500 to-brand-600 text-sm font-semibold text-white shadow-sm shadow-brand-500/25"
					>
						{getInitials($currentUser.display_name)}
					</div>
					<span class="hidden max-w-[140px] truncate text-sm font-medium text-base-content/80 md:block">
						{$currentUser.display_name}
					</span>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
						class="h-4 w-4 text-base-content/40"
					>
						<path d="m6 9 6 6 6-6" />
					</svg>
				</button>

				{#if userMenuOpen}
					<div
						class="animate-slide-in-up absolute right-0 z-50 mt-2 w-56 rounded-2xl border border-base-300/80 bg-base-100 py-1 shadow-lg shadow-black/20"
					>
						<div class="border-b border-base-300/70 px-4 py-3">
							<p class="truncate text-sm font-semibold text-base-content">
								{$currentUser.display_name}
							</p>
							<p class="truncate text-xs text-base-content/50">{$currentUser.email}</p>
						</div>

						<nav class="py-1">
							<a
								href="/profile"
								class="flex items-center gap-3 px-4 py-2 text-sm text-base-content/80 transition-colors hover:bg-base-200 hover:text-base-content"
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
									class="h-4 w-4"
								>
									<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
									<circle cx="12" cy="7" r="4" />
								</svg>
								Profile
							</a>
							<a
								href="/settings"
								class="flex items-center gap-3 px-4 py-2 text-sm text-base-content/80 transition-colors hover:bg-base-200 hover:text-base-content"
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
									class="h-4 w-4"
								>
									<path
										d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
									/>
									<circle cx="12" cy="12" r="3" />
								</svg>
								Settings
							</a>
							{#if $currentUser.is_admin}
								<a
									href="/admin"
									class="flex items-center gap-3 px-4 py-2 text-sm text-base-content/80 transition-colors hover:bg-base-200 hover:text-base-content"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										class="h-4 w-4"
									>
										<path
											d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
										/>
										<circle cx="12" cy="12" r="3" />
									</svg>
									Admin Panel
								</a>
							{/if}
						</nav>

						<div class="border-t border-base-200 py-1">
							<button
								type="button"
								class="flex w-full items-center gap-3 px-4 py-2 text-sm text-error transition-colors hover:bg-error/10"
								on:click={handleLogout}
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
									class="h-4 w-4"
								>
									<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
									<polyline points="16 17 21 12 16 7" />
									<line x1="21" x2="9" y1="12" y2="12" />
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
