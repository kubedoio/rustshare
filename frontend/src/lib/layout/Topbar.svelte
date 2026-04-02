<script lang="ts">
	import { goto } from '$app/navigation';
	import ThemeToggle from '$lib/components/common/ThemeToggle.svelte';
	import { currentUser, authStore } from '$lib/stores/auth';
	import { searchQuery as globalSearchQuery } from '$lib/stores/search';
	import { createQuery } from '@tanstack/svelte-query';
	import { getUnreadNotificationCount } from '$lib/api/notifications';
	import { listAllFiles } from '$lib/api/files';
	import { getFolderTree, type FolderTree } from '$lib/api/folders';
	import { getFeatures } from '$lib/api/features';
	import { createInvite } from '$lib/api/invites';
	import { onMount } from 'svelte';
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

	let userMenuOpen = false;
	let newMenuOpen = false;
	let inviteOpen = false;
	let inviteEmail = '';
	let inviteState: 'idle' | 'done' = 'idle';
	let inviteLink = '';
	let inviteEnabled = false;
	let inviteLoading = false;
	let inviteErrorMsg = '';

	onMount(async () => {
		if ($currentUser) {
			try {
				const res = await getFeatures();
				inviteEnabled = res.invite_enabled;
			} catch {
				inviteEnabled = false;
			}
		}
	});

	async function handleSendInvite() {
		if (!inviteEmail.trim()) return;
		inviteLoading = true;
		inviteErrorMsg = '';
		try {
			const res = await createInvite({
				recipient_email: inviteEmail.trim(),
				origin: window.location.origin
			});
			inviteLink = res.invite_link;
			inviteState = 'done';
		} catch (err: any) {
			inviteErrorMsg = err?.message || 'Failed to send invite';
		} finally {
			inviteLoading = false;
		}
	}

	function resetInvite() {
		inviteEmail = '';
		inviteState = 'idle';
		inviteLink = '';
		inviteOpen = false;
	}

	$: unreadCountQuery = createQuery({
		queryKey: ['notifications-unread-count'],
		queryFn: () => getUnreadNotificationCount(),
		enabled: !!$currentUser,
		refetchInterval: 30000
	});

	$: allFilesQuery = createQuery({
		queryKey: ['all-files'],
		queryFn: () => listAllFiles(),
		enabled: !!$currentUser
	});

	$: folderTreeQuery = createQuery<FolderTree>({
		queryKey: ['folder-tree'],
		queryFn: () => getFolderTree(),
		enabled: !!$currentUser
	});

	function flattenFolderTree(node: FolderTree | undefined): Array<{ id: string; name: string; path: string }> {
		if (!node) return [];
		const result = [];
		if (node.folder) {
			result.push({ id: node.folder.id, name: node.folder.name, path: node.folder.path });
		}
		if (node.subfolders) {
			for (const sub of node.subfolders) {
				result.push(...flattenFolderTree(sub));
			}
		}
		return result;
	}

	$: searchResults = (() => {
		const q = $globalSearchQuery.toLowerCase().trim();
		if (!q) return { files: [], folders: [] };

		const allFiles = $allFilesQuery.data || [];
		const allFolders = flattenFolderTree($folderTreeQuery.data);

		const files = allFiles.filter(f => f.name.toLowerCase().includes(q) && !f.deleted_at).slice(0, 10);
		const folders = allFolders.filter(f => f.name.toLowerCase().includes(q)).slice(0, 5);

		return { files, folders };
	})();

	function handleSearchInput(event: Event) {
		const target = event.target as HTMLInputElement;
		globalSearchQuery.set(target.value);
	}

	function clearSearch() {
		globalSearchQuery.set('');
	}

	function navigateToSearchResult(type: 'file' | 'folder', id: string) {
		clearSearch();
		if (type === 'file') {
			goto(`/files?preview=${id}`);
		} else {
			goto(`/files?folder=${id}`);
		}
	}

	function handleSearchKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			clearSearch();
		}
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
		if (!target.closest('.user-menu-container')) userMenuOpen = false;
		if (!target.closest('.new-menu-container')) newMenuOpen = false;
		if (!target.closest('.invite-container')) { inviteOpen = false; }
		if (!target.closest('.global-search-container')) { if ($globalSearchQuery) clearSearch(); }
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
		<div class="w-full max-w-xl global-search-container">
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
					on:keydown={handleSearchKeydown}
				/>
				{#if $globalSearchQuery}
					<button
						type="button"
						class="absolute inset-y-0 right-0 flex items-center pr-3 text-base-content/30 hover:text-base-content"
						on:click={clearSearch}
					>
						<X size={16} />
					</button>

					<!-- Global Search Results Dropdown -->
					<div class="absolute top-full left-0 right-0 mt-2 rounded-2xl border border-base-300 bg-base-100 p-2 shadow-2xl ring-1 ring-black/5 animate-in fade-in zoom-in duration-100 z-50 overflow-hidden">
						{#if searchResults.folders.length === 0 && searchResults.files.length === 0}
							<div class="text-center py-6">
								<p class="text-sm font-medium text-base-content/60">No results found for "{$globalSearchQuery}"</p>
							</div>
						{:else}
							{#if searchResults.folders.length > 0}
								<div class="mb-1 px-2 py-1.5 text-xs font-bold uppercase tracking-wider text-base-content/50">
									Folders
								</div>
								{#each searchResults.folders as folder (folder.id)}
									<button 
										class="flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-sm hover:bg-base-200 transition-colors"
										on:click={() => navigateToSearchResult('folder', folder.id)}
									>
										<Folder size={16} class="text-amber-500 shrink-0" />
										<div class="flex flex-col items-start truncate leading-tight">
											<span class="font-medium text-base-content truncate">{folder.name}</span>
											<span class="text-[10px] text-base-content/50 mt-0.5 truncate">{folder.path}</span>
										</div>
									</button>
								{/each}
							{/if}

							{#if searchResults.folders.length > 0 && searchResults.files.length > 0}
								<div class="h-px bg-base-200 w-full my-2"></div>
							{/if}

							{#if searchResults.files.length > 0}
								<div class="mb-1 px-2 py-1.5 text-xs font-bold uppercase tracking-wider text-base-content/50">
									Files
								</div>
								{#each searchResults.files as file (file.id)}
									<button 
										class="flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-sm hover:bg-base-200 transition-colors"
										on:click={() => navigateToSearchResult('file', file.id)}
									>
										<FileText size={16} class="text-brand-500 shrink-0" />
										<div class="flex flex-col items-start truncate leading-tight">
											<span class="font-medium text-base-content truncate">{file.name}</span>
											<span class="text-[10px] text-base-content/50 mt-0.5 truncate">{file.path}</span>
										</div>
									</button>
								{/each}
							{/if}
						{/if}
					</div>
				{/if}
			</div>
		</div>
	</div>

	<!-- Right Side: User, Theme, Invite -->
	<div class="flex items-center gap-2 min-w-[240px] justify-end">
		<!-- Invite Button + Popup -->
		{#if inviteEnabled}
		<div class="invite-container relative">
			<button
				type="button"
				class="hidden items-center gap-2 rounded-xl border border-base-300/60 px-3 py-2 text-xs font-bold text-base-content/70 transition-all hover:bg-base-200 sm:flex"
				on:click={() => { inviteOpen = !inviteOpen; if (inviteOpen) { inviteEmail = ''; inviteState = 'idle'; inviteLink = ''; inviteErrorMsg = ''; } }}
			>
				<UserPlus size={16} />
				<span>Invite</span>
			</button>

			{#if inviteOpen}
				<div class="absolute right-0 mt-2 w-80 origin-top-right rounded-2xl border border-base-300 bg-base-100 p-4 shadow-2xl ring-1 ring-black/5 animate-in fade-in zoom-in duration-100 z-[200]">
					<div class="flex items-center justify-between mb-3">
						<div>
							<h3 class="text-sm font-bold text-base-content">Send an Invitation</h3>
							<p class="text-xs text-base-content/50 mt-0.5">Share a unique signup link</p>
						</div>
						<button type="button" class="p-1 rounded-lg hover:bg-base-200 text-base-content/40 hover:text-base-content" on:click={resetInvite}>
							<X size={16} />
						</button>
					</div>

					{#if inviteState === 'idle'}
						<div class="space-y-3">
							<div>
								<label class="text-xs font-semibold text-base-content/70 mb-1 block" for="invite-email">Recipient email</label>
								<input
									id="invite-email"
									type="email"
									bind:value={inviteEmail}
									placeholder="colleague@company.com"
									class="w-full rounded-xl border border-base-300/60 bg-base-200/50 px-3 py-2 text-sm text-base-content placeholder:text-base-content/30 focus:border-brand-500/50 focus:bg-base-100 focus:outline-none focus:ring-2 focus:ring-brand-500/10"
									on:keydown={(e) => e.key === 'Enter' && handleSendInvite()}
								/>
							</div>
							<p class="text-[10px] text-base-content/40 leading-relaxed">
								This will generate a unique invite link powered by the <a href="/admin/workflows" class="text-brand-500 hover:underline">Invite Email workflow</a>.
							</p>
							<button
								type="button"
								class="w-full rounded-xl bg-brand-500 px-4 py-2 text-sm font-bold text-white shadow-sm transition-all hover:bg-brand-600 active:scale-[0.98] disabled:opacity-50"
								disabled={!inviteEmail.trim() || inviteLoading}
								on:click={handleSendInvite}
							>
								{inviteLoading ? 'Sending...' : 'Generate Invite Link'}
							</button>
							{#if inviteErrorMsg}
								<p class="text-xs text-red-500 mt-2">{inviteErrorMsg}</p>
							{/if}
						</div>
					{:else}
						<div class="space-y-3">
							<div class="flex items-center gap-2 rounded-xl bg-green-500/10 border border-green-500/20 px-3 py-2">
								<div class="h-2 w-2 rounded-full bg-green-500 shrink-0"></div>
								<p class="text-xs font-medium text-green-700 dark:text-green-400">Invite link ready for <span class="font-bold">{inviteEmail}</span></p>
							</div>
							<div class="rounded-xl border border-base-300/50 bg-base-200/50 p-2">
								<p class="text-[10px] text-base-content/50 mb-1 font-semibold uppercase tracking-wider">Invite Link</p>
								<p class="text-[11px] text-base-content/80 break-all font-mono leading-relaxed">{inviteLink}</p>
							</div>
							<button
								type="button"
								class="w-full rounded-xl bg-brand-500 px-4 py-2 text-sm font-bold text-white shadow-sm transition-all hover:bg-brand-600 active:scale-[0.98]"
								on:click={() => navigator.clipboard.writeText(inviteLink)}
							>Copy Link</button>
							<button type="button" class="w-full text-xs text-base-content/50 hover:text-base-content" on:click={() => { inviteState = 'idle'; inviteEmail = ''; }}>Invite someone else</button>
						</div>
					{/if}
				</div>
			{/if}
		</div>

		<div class="h-6 w-px bg-base-300/60 mx-1 hidden sm:block"></div>

		{/if}
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
