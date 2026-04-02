<script lang="ts">
	import { goto } from '$app/navigation';
	import ThemeToggle from '$lib/components/common/ThemeToggle.svelte';
	import { currentUser, authStore } from '$lib/stores/auth';
	import { searchQuery as globalSearchQuery } from '$lib/stores/search';
	import { createQuery } from '@tanstack/svelte-query';
	import { getUnreadNotificationCount } from '$lib/api/notifications';
	import { 
		Plus, 
		UserPlus, 
		Search, 
		X, 
		ChevronDown, 
		File, 
		Folder, 
		FileText, 
		Upload, 
		Edit3, 
		PenTool,
		LogOut,
		User,
		Settings,
		Shield,
		Bell
	} from 'lucide-svelte';

	export let onMenuClick: () => void = () => {};
	export let onSidebarToggle: () => void = () => {};
	export let sidebarCollapsed = false;
	export let hideSidebarToggle = false;

	let userMenuOpen = false;
	let newMenuOpen = false;

	$: unreadCountQuery = createQuery({
		queryKey: ['notifications-unread-count'],
		queryFn: () => getUnreadNotificationCount(),
		enabled: !!$currentUser,
		refetchInterval: 30000
	});

	function handleSearchInput(event: Event) {
		const target = event.target as HTMLInputElement;
		globalSearchQuery.set(target.value);
	}

	function clearSearch() {
		globalSearchQuery.set('');
	}

	async function handleLogout() {
		await authStore.logout();
		goto('/login');
	}

	function getInitials(name: string): string {
		return name?.charAt(0).toUpperCase() || '?';
	}

	function executeGlobalAction(action: string) {
		newMenuOpen = false;
		goto('/files');
		setTimeout(() => {
			window.dispatchEvent(new CustomEvent(action));
		}, 100);
	}

	function handleClickOutside(event: MouseEvent) {
		const target = event.target as HTMLElement;
		if (!target.closest('.user-menu-container')) {
			userMenuOpen = false;
		}
		if (!target.closest('.new-menu-container')) {
			newMenuOpen = false;
		}
	}
</script>

<svelte:window on:click={handleClickOutside} />

<header
	class="relative z-[95] flex h-16 items-center border-b border-base-300/60 bg-base-100/80 px-4 backdrop-blur-xl lg:px-6"
>
	<!-- Left Side: Toggle & [+ New] -->
	<div class="flex items-center gap-4 min-w-[240px]">
		<button
			type="button"
			class="-ml-2 flex items-center justify-center rounded-xl border border-transparent p-2 text-base-content/60 transition-colors hover:border-base-300/80 hover:bg-base-200/80 hover:text-base-content lg:hidden"
			on:click={onMenuClick}
			aria-label="Open menu"
		>
			<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="12" x2="21" y2="12"></line><line x1="3" y1="6" x2="21" y2="6"></line><line x1="3" y1="18" x2="21" y2="18"></line></svg>
		</button>

		{#if !hideSidebarToggle}
			<button
				type="button"
				class="hidden rounded-xl border border-transparent p-2 text-base-content/60 transition-colors hover:border-base-300/80 hover:bg-base-200/80 hover:text-base-content lg:flex"
				on:click={onSidebarToggle}
			>
				{#if sidebarCollapsed}
					<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
				{:else}
					<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"></polyline></svg>
				{/if}
			</button>
		{/if}

		<div class="new-menu-container relative">
			<button
				type="button"
				class="flex items-center gap-2 rounded-xl bg-brand-500 px-4 py-2 text-sm font-bold text-white shadow-lg shadow-brand-500/30 transition-all hover:bg-brand-600 hover:shadow-brand-500/40 active:scale-95"
				on:click={() => (newMenuOpen = !newMenuOpen)}
			>
				<Plus size={18} />
				<span>New</span>
				<ChevronDown size={14} class="opacity-60" />
			</button>

			{#if newMenuOpen}
				<div class="absolute left-0 mt-2 w-56 origin-top-left rounded-2xl border border-base-300 bg-base-100 p-1 shadow-xl ring-1 ring-black/5 animate-in fade-in zoom-in duration-100">
					<button class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium hover:bg-base-200 transition-colors" on:click={() => executeGlobalAction('create-file-requested')}>
						<File size={16} class="text-blue-500" /> File
					</button>
					<button class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium hover:bg-base-200 transition-colors" on:click={() => executeGlobalAction('create-folder-requested')}>
						<Folder size={16} class="text-amber-500" /> Folder
					</button>
					<button class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium hover:bg-base-200 transition-colors border-t border-base-200 mt-1 pt-2.5" on:click={() => executeGlobalAction('create-document-requested')}>
						<FileText size={16} class="text-emerald-500" /> Document
					</button>
					<button class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium hover:bg-base-200 transition-colors" on:click={() => executeGlobalAction('upload-requested')}>
						<Upload size={16} class="text-indigo-500" /> Upload
					</button>
					<button class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium hover:bg-base-200 transition-colors border-t border-base-200 mt-1 pt-2.5" on:click={() => executeGlobalAction('create-file-requested')}>
						<Edit3 size={16} class="text-rose-500" /> Edit
					</button>
					<button class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium hover:bg-base-200 transition-colors" on:click={() => executeGlobalAction('create-canvas-requested')}>
						<PenTool size={16} class="text-cyan-500" /> Sign
					</button>
				</div>
			{/if}
		</div>
	</div>

	<!-- Center: Global Search -->
	<div class="flex flex-1 justify-center px-4">
		<div class="w-full max-w-xl">
			<div class="group relative">
				<div class="pointer-events-none absolute inset-y-0 left-0 flex items-center pl-3">
					<Search size={16} class="text-base-content/30 transition-colors group-focus-within:text-brand-500" />
				</div>
				<input
					type="text"
					placeholder="Search files, folders, or activity..."
					class="w-full rounded-2xl border border-base-300/50 bg-base-200/50 px-10 py-2 text-sm text-base-content transition-all placeholder:text-base-content/30 focus:border-brand-500/50 focus:bg-base-100 focus:outline-none focus:ring-4 focus:ring-brand-500/10"
					value={$globalSearchQuery}
					on:input={handleSearchInput}
				/>
				{#if $globalSearchQuery}
					<button
						type="button"
						class="absolute inset-y-0 right-0 flex items-center pr-3 text-base-content/30 hover:text-base-content"
						on:click={clearSearch}
					>
						<X size={16} />
					</button>
				{/if}
			</div>
		</div>
	</div>

	<!-- Right Side: User, Theme, Invite -->
	<div class="flex items-center gap-2 min-w-[240px] justify-end">
		<button
			type="button"
			class="hidden items-center gap-2 rounded-xl border border-base-300/60 px-3 py-2 text-xs font-bold text-base-content/70 transition-all hover:bg-base-200 sm:flex"
		>
			<UserPlus size={16} />
			<span>Invite Members</span>
		</button>

		<div class="h-6 w-px bg-base-300/60 mx-1 hidden sm:block"></div>

		<a 
			href="/notifications" 
			class="relative flex items-center justify-center p-2 rounded-xl text-base-content/60 hover:bg-base-200 hover:text-base-content transition-colors"
			aria-label="Notifications"
		>
			<Bell size={18} />
			{#if $unreadCountQuery.data && $unreadCountQuery.data.count > 0}
				<span class="absolute top-[6px] right-[6px] flex h-[8px] w-[8px]">
					<span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-error opacity-75"></span>
					<span class="relative inline-flex rounded-full h-[8px] w-[8px] bg-error"></span>
				</span>
			{/if}
		</a>

		<ThemeToggle />

		{#if $currentUser}
			<div class="user-menu-container relative ml-1">
				<button
					type="button"
					class="flex items-center gap-2 rounded-xl border border-base-300/60 bg-base-100/50 p-1 pr-3 transition-all hover:border-brand-500/20 hover:bg-base-200"
					on:click={() => (userMenuOpen = !userMenuOpen)}
				>
					<div class="flex h-8 w-8 items-center justify-center rounded-xl bg-brand-500 font-bold text-white shadow-sm">
						{getInitials($currentUser.display_name)}
					</div>
					<span class="hidden max-w-[120px] truncate text-[13px] font-semibold text-base-content/80 md:block">
						{$currentUser.display_name}
					</span>
					<ChevronDown size={14} class="opacity-40" />
				</button>

				{#if userMenuOpen}
					<div class="absolute right-0 mt-2 w-56 origin-top-right rounded-2xl border border-base-300 bg-base-100 py-1.5 shadow-xl ring-1 ring-black/5 animate-in fade-in slide-in-from-top-2 duration-100">
						<div class="border-b border-base-200 px-4 py-3 mb-1">
							<p class="truncate text-sm font-bold text-base-content">{$currentUser.display_name}</p>
							<p class="truncate text-[11px] font-medium text-base-content/50 uppercase tracking-wider">{$currentUser.email}</p>
						</div>

						<a href="/profile" class="flex items-center gap-3 px-4 py-2 text-sm font-medium hover:bg-base-200 transition-colors">
							<User size={16} class="text-base-content/60" /> Profile
						</a>
						<a href="/notifications" class="flex items-center justify-between px-4 py-2 text-sm font-medium hover:bg-base-200 transition-colors">
							<div class="flex items-center gap-3">
								<Bell size={16} class="text-base-content/60" /> Notifications
							</div>
							{#if $unreadCountQuery.data && $unreadCountQuery.data.count > 0}
								<span class="badge badge-error badge-sm">{$unreadCountQuery.data.count}</span>
							{/if}
						</a>
						<a href="/settings" class="flex items-center gap-3 px-4 py-2 text-sm font-medium hover:bg-base-200 transition-colors">
							<Settings size={16} class="text-base-content/60" /> Settings
						</a>
						{#if $currentUser.is_admin}
							<a href="/admin" class="flex items-center gap-3 px-4 py-2 text-sm font-medium hover:bg-base-200 transition-colors">
								<Shield size={16} class="text-brand-500" /> Admin Panel
							</a>
						{/if}

						<div class="border-t border-base-200 mt-1 pt-1.5">
							<button
								on:click={handleLogout}
								class="flex w-full items-center gap-3 px-4 py-2 text-sm font-bold text-error hover:bg-error/10 transition-colors"
							>
								<LogOut size={16} /> Sign out
							</button>
						</div>
					</div>
				{/if}
			</div>
		{/if}
	</div>
</header>
