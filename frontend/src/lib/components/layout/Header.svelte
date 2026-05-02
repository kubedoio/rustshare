<script lang="ts">
	import { currentUser, authStore } from '$lib/stores/auth';
	import ThemeToggle from '$lib/components/common/ThemeToggle.svelte';
	import WebSocketStatus from '$lib/components/common/WebSocketStatus.svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { getAvatarUrl } from '$lib/api/users';
	import { createQuery } from '$lib/query-compat';
	import { getUnreadNotificationCount } from '$lib/api/notifications';

	interface Props {
		onMenuClick?: () => void;
		onHelpClick?: () => void;
		onSearchChange?: ((query: string) => void) | null;
		searchQuery?: string;
		onSearch?: (payload: { query: string }) => void;
		breadcrumbs?: import('svelte').Snippet;
	}

	let {
		onMenuClick = () => {},
		onHelpClick = () => {},
		onSearchChange = null,
		searchQuery = '',
		onSearch = () => {},
		breadcrumbs
	}: Props = $props();

	const unreadNotificationsQuery = createQuery({
		queryKey: ['notifications', 'sidebar-unread-count'],
		queryFn: getUnreadNotificationCount
	});

	// Cache buster for avatar - updates when user or avatar changes
	let lastUserId = $state($currentUser?.id);
	let lastAvatarPath = $state($currentUser?.avatar_path);
	let avatarTimestamp = $state(Date.now());
	let avatarError = $state(false);

	$effect(() => {
		if ($currentUser?.id !== lastUserId || $currentUser?.avatar_path !== lastAvatarPath) {
			lastUserId = $currentUser?.id;
			lastAvatarPath = $currentUser?.avatar_path;
			avatarTimestamp = Date.now();
			avatarError = false;
		}
	});

	function handleSearchInput(event: Event) {
		const target = event.target as HTMLInputElement;
		searchQuery = target.value;
		if (onSearchChange) {
			onSearchChange(searchQuery);
		}
		onSearch({ query: searchQuery });
	}

	function clearSearch() {
		searchQuery = '';
		if (onSearchChange) {
			onSearchChange('');
		}
		onSearch({ query: '' });
	}

	async function handleLogout() {
		await authStore.logout();
		goto('/login');
	}
</script>

<header
	class="flex h-16 items-center justify-between gap-4 border-b border-base-300 bg-base-100 px-4 lg:px-6"
>
	<div class="flex min-w-0 flex-1 items-center gap-4">
		<!-- Hamburger menu (mobile only) -->
		<button
			class="btn btn-square flex-shrink-0 btn-ghost lg:hidden"
			aria-label="Open navigation menu"
			onclick={onMenuClick}
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="h-6 w-6"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5"
				/>
			</svg>
		</button>

		<div class="min-w-0 flex-shrink overflow-x-auto">
			{@render breadcrumbs?.()}
		</div>

		<!-- Search bar (desktop) -->
		{#if onSearchChange !== null}
			<div class="hidden max-w-md flex-1 lg:flex">
				<div class="form-control w-full">
					<div class="input-group">
						<span class="bg-base-200">
							<svg
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke-width="1.5"
								stroke="currentColor"
								class="h-5 w-5"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z"
								/>
							</svg>
						</span>
						<input
							type="text"
							placeholder="Search files and folders..."
							class="input-bordered input input-sm w-full"
							bind:value={searchQuery}
							oninput={handleSearchInput}
						/>
						{#if searchQuery}
							<button class="btn btn-square btn-sm" aria-label="Clear search" onclick={clearSearch}>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="h-5 w-5"
								>
									<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
								</svg>
							</button>
						{/if}
					</div>
				</div>
			</div>
		{/if}
	</div>

	<div class="flex flex-shrink-0 items-center gap-2 lg:gap-4">
		<!-- WebSocket status indicator -->
		<WebSocketStatus />

		<!-- Notifications -->
		<a href="/notifications" class="btn relative btn-circle btn-ghost btn-sm" title="Notifications">
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="h-5 w-5"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M14.857 17.082a23.848 23.848 0 005.454-1.31A8.967 8.967 0 0118 9.75v-.7V9A6 6 0 006 9v.75a8.967 8.967 0 01-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 01-5.714 0m5.714 0a3 3 0 11-5.714 0"
				/>
			</svg>
			{#if $unreadNotificationsQuery.data && $unreadNotificationsQuery.data.count > 0}
				<span class="indicator-item absolute top-0 right-0 badge badge-xs badge-primary">
					{$unreadNotificationsQuery.data.count}
				</span>
			{/if}
		</a>

		<!-- Theme toggle -->
		<ThemeToggle />

		<!-- Search button (mobile) -->
		{#if onSearchChange !== null}
			<div class="dropdown dropdown-end lg:hidden">
				<button
					type="button"
					class="btn btn-circle btn-ghost btn-sm"
					aria-label="Open mobile search"
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="h-5 w-5"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z"
						/>
					</svg>
				</button>
				<div class="dropdown-content card-compact card z-[1] mt-3 w-64 bg-base-100 p-2 shadow">
					<div class="form-control">
						<div class="input-group">
							<input
								type="text"
								placeholder="Search..."
								class="input-bordered input input-sm w-full"
								bind:value={searchQuery}
								oninput={handleSearchInput}
							/>
							{#if searchQuery}
								<button
									class="btn btn-square btn-sm"
									aria-label="Clear search"
									onclick={clearSearch}
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										fill="none"
										viewBox="0 0 24 24"
										stroke-width="1.5"
										stroke="currentColor"
										class="h-5 w-5"
									>
										<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
									</svg>
								</button>
							{/if}
						</div>
					</div>
				</div>
			</div>
		{/if}

		<!-- Help button -->
		<button
			class="btn btn-circle btn-ghost btn-sm"
			onclick={onHelpClick}
			title="Keyboard shortcuts"
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="h-5 w-5"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9 5.25h.008v.008H12v-.008z"
				/>
			</svg>
		</button>

		{#if $currentUser}
			<div class="dropdown dropdown-end">
				<button type="button" class="btn avatar btn-circle btn-ghost">
					<div
						class="flex w-8 items-center justify-center overflow-hidden rounded-full bg-primary text-primary-content lg:w-10"
					>
						{#if $currentUser.avatar_path && !avatarError}
							<img
								src={`${getAvatarUrl($currentUser.id)}?t=${avatarTimestamp}`}
								alt="Avatar"
								class="h-full w-full object-cover"
								onerror={() => (avatarError = true)}
							/>
						{:else}
							<span class="text-lg font-semibold lg:text-xl"
								>{$currentUser.display_name[0].toUpperCase()}</span
							>
						{/if}
					</div>
				</button>
				<ul
					class="menu-compact dropdown-content menu z-[100] mt-3 w-52 rounded-box bg-base-100 p-2 shadow"
				>
					<li class="menu-title">
						<span class="truncate">{$currentUser.display_name}</span>
						<span class="truncate text-xs text-base-content/60">{$currentUser.email}</span>
					</li>
					<li>
						<a href="/profile">
							<svg
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke-width="1.5"
								stroke="currentColor"
								class="h-4 w-4"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z"
								/>
							</svg>
							Profile
						</a>
					</li>
					{#if $currentUser.is_admin}
						<li>
							<a href="/admin">
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="h-4 w-4"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M10.343 3.94c.09-.542.56-.94 1.11-.94h1.093c.55 0 1.02.398 1.11.94l.149.894c.07.424.384.764.78.93.398.164.855.142 1.205-.108l.737-.527a1.125 1.125 0 011.45.12l.773.774c.39.389.44 1.002.12 1.45l-.527.737c-.25.35-.272.806-.107 1.204.165.397.505.71.93.78l.893.15c.543.09.94.56.94 1.109v1.094c0 .55-.397 1.02-.94 1.11l-.893.149c-.425.07-.765.383-.93.78-.165.398-.143.854.107 1.204l.527.738c.32.447.269 1.06-.12 1.45l-.774.773a1.125 1.125 0 01-1.449.12l-.738-.527c-.35.25-.806.272-1.203.107-.397.165-.71.505-.781.929l-.149.894c-.09.542-.56.94-1.11.94h-1.094c-.55 0-1.019-.398-1.11-.94l-.148-.894c-.071-.424-.384-.764-.781-.93-.398-.164-.854-.142-1.204.108l-.738.527c-.447.32-1.06.269-1.45-.12l-.773-.774a1.125 1.125 0 01-.12-1.45l.527-.737c.25-.35.273-.806.108-1.204-.165-.397-.505-.71-.93-.78l-.894-.15c-.542-.09-.94-.56-.94-1.109v-1.094c0-.55.398-1.02.94-1.11l.894-.149c.424-.07.765-.383.93-.78.165-.398.143-.854-.107-1.204l-.527-.738a1.125 1.125 0 01.12-1.45l.773-.773a1.125 1.125 0 011.45-.12l.737.527c.35.25.807.272 1.204.107.397-.165.71-.505.78-.929l.15-.894zM15 12a3 3 0 11-6 0 3 3 0 016 0z"
									/>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
									/>
								</svg>
								Admin
							</a>
						</li>
					{/if}
					<li>
						<a href="/settings">
							<svg
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke-width="1.5"
								stroke="currentColor"
								class="h-4 w-4"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									d="M9.594 3.94c.09-.542.56-.94 1.11-.94h1.093c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l.459 1.782c.139.534-.097 1.1-.51 1.384l-1.036.724c-.308.215-.49.564-.49.94 0 .376.182.725.49.94l1.036.724c.413.284.65.85.51 1.384l-.459 1.782a1.125 1.125 0 01-1.37.49l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 01-.22.127c-.331.183-.581.495-.644.87l-.212 1.28c-.09.543-.56.941-1.11.941h-1.094c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-.46-1.782a1.122 1.122 0 01.511-1.384l1.036-.724c.308-.215.49-.564.49-.94 0-.376-.182-.725-.49-.94l-1.036-.724a1.122 1.122 0 01-.511-1.384l.46-1.782a1.122 1.122 0 011.37-.49l1.217.456c.355.133.75.072 1.076-.124.072-.044.146-.087.22-.127.332-.184.582-.496.645-.87l.212-1.28zM15 12a3 3 0 11-6 0 3 3 0 016 0z"
								/>
							</svg>
							Settings
						</a>
					</li>
					<li class="divider"></li>
					<li>
						<button onclick={handleLogout} class="text-error">
							<svg
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke-width="1.5"
								stroke="currentColor"
								class="h-4 w-4"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									d="M15.75 9V5.25A2.25 2.25 0 0013.5 3h-6a2.25 2.25 0 00-2.25 2.25v13.5A2.25 2.25 0 007.5 21h6a2.25 2.25 0 002.25-2.25V15M12 9l-3 3m0 0l3 3m-3-3h12.75"
								/>
							</svg>
							Logout
						</button>
					</li>
				</ul>
			</div>
		{/if}
	</div>
</header>
