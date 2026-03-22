<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { browser } from '$app/environment';
	import { authStore } from '$lib/stores/auth';

	const navItems = [
		{ href: '/admin/users', label: 'Users', icon: 'users' },
		{ href: '/admin/groups', label: 'Groups', icon: 'group' },
		{ href: '/admin/oidc', label: 'OIDC / SSO', icon: 'shield' },
		{ href: '/admin/integrations', label: 'Integrations', icon: 'plug' },
		{ href: '/admin/audit', label: 'Audit Log', icon: 'list' }
	];

	$: if (browser && !$authStore.isLoading && !$authStore.user?.is_admin) {
		goto('/dashboard');
	}
</script>

{#if $authStore.isLoading}
  <div class="flex items-center justify-center h-screen">
    <span class="loading loading-spinner loading-lg"></span>
  </div>
{:else if $authStore.user?.is_admin}
<div class="flex h-screen overflow-hidden">
	<!-- Admin Sidebar -->
	<aside class="w-64 bg-neutral text-neutral-content flex h-screen flex-col flex-shrink-0">
		<div class="p-4 border-b border-neutral-700 flex items-center gap-2">
			<a href="/files" class="text-xl font-bold hover:opacity-80 transition-opacity">RustShare</a>
			<span class="badge badge-warning badge-sm ml-auto">Admin</span>
		</div>

		<nav class="p-4 flex-1 overflow-y-auto">
			<p class="text-xs font-semibold uppercase tracking-wider text-neutral-400 mb-3 px-2">
				Administration
			</p>
			<ul class="menu menu-sm p-0">
				{#each navItems as item}
					<li>
						<a
							href={item.href}
							class="gap-2 flex items-center rounded-lg py-2 px-3 hover:bg-neutral-700 transition-colors"
							class:bg-neutral-600={$page.url.pathname.startsWith(item.href)}
							class:font-semibold={$page.url.pathname.startsWith(item.href)}
						>
							{#if item.icon === 'users'}
								<svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
									<path stroke-linecap="round" stroke-linejoin="round" d="M15 19.128a9.38 9.38 0 002.625.372 9.337 9.337 0 004.121-.952 4.125 4.125 0 00-7.533-2.493M15 19.128v-.003c0-1.113-.285-2.16-.786-3.07M15 19.128v.106A12.318 12.318 0 018.624 21c-2.331 0-4.512-.645-6.374-1.766l-.001-.109a6.375 6.375 0 0111.964-3.07M12 6.375a3.375 3.375 0 11-6.75 0 3.375 3.375 0 016.75 0zm8.25 2.25a2.625 2.625 0 11-5.25 0 2.625 2.625 0 015.25 0z" />
								</svg>
							{:else if item.icon === 'group'}
								<svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
									<path stroke-linecap="round" stroke-linejoin="round" d="M18 18.72a9.094 9.094 0 003.741-.479 3 3 0 00-4.682-2.72m.94 3.198l.001.031c0 .225-.012.447-.037.666A11.944 11.944 0 0112 21c-2.17 0-4.207-.576-5.963-1.584A6.062 6.062 0 016 18.719m12 0a5.971 5.971 0 00-.941-3.197m0 0A5.995 5.995 0 0012 12.75a5.995 5.995 0 00-5.058 2.772m0 0a3 3 0 00-4.681 2.72 8.986 8.986 0 003.74.477m.94-3.197a5.971 5.971 0 00-.94 3.197M15 6.75a3 3 0 11-6 0 3 3 0 016 0zm6 3a2.25 2.25 0 11-4.5 0 2.25 2.25 0 014.5 0zm-13.5 0a2.25 2.25 0 11-4.5 0 2.25 2.25 0 014.5 0z" />
								</svg>
							{:else if item.icon === 'shield'}
								<svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
									<path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z" />
								</svg>
							{:else if item.icon === 'plug'}
								<svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
									<path stroke-linecap="round" stroke-linejoin="round" d="M13.19 8.688a4.5 4.5 0 011.242 7.244l-4.5 4.5a4.5 4.5 0 01-6.364-6.364l1.757-1.757m13.35-.622l1.757-1.757a4.5 4.5 0 00-6.364-6.364l-4.5 4.5a4.5 4.5 0 001.242 7.244" />
								</svg>
							{:else}
								<svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
									<path stroke-linecap="round" stroke-linejoin="round" d="M8.25 6.75h12M8.25 12h12m-12 5.25h12M3.75 6.75h.007v.008H3.75V6.75zm.375 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zM3.75 12h.007v.008H3.75V12zm.375 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zm-.375 5.25h.007v.008H3.75v-.008zm.375 0a.375.375 0 11-.75 0 .375.375 0 01.75 0z" />
								</svg>
							{/if}
							{item.label}
						</a>
					</li>
				{/each}
			</ul>
		</nav>

		<div class="p-4 border-t border-neutral-700">
			<a href="/files" class="btn btn-outline btn-block btn-sm text-neutral-content border-neutral-500 hover:bg-neutral-700 hover:border-neutral-400">
				Back to App
			</a>
		</div>
	</aside>

	<!-- Main Content -->
	<div class="flex-1 flex flex-col overflow-hidden">
		<!-- Admin Header -->
		<header class="bg-base-100 border-b border-base-300 px-6 py-3 flex items-center gap-3 flex-shrink-0">
			<h1 class="text-lg font-semibold text-base-content">Admin Panel</h1>
			<span class="badge badge-warning">Admin</span>
			<div class="ml-auto flex items-center gap-2 text-sm text-base-content/60">
				<svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
					<path stroke-linecap="round" stroke-linejoin="round" d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z" />
				</svg>
				{$authStore.user?.email ?? ''}
			</div>
		</header>

		<main class="flex-1 overflow-auto bg-base-200 p-6">
			<slot />
		</main>
	</div>
</div>
{/if}
