<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { browser } from '$app/environment';
	import { authStore } from '$lib/stores/auth';

	const navItems = [
		{ href: '/admin/users', label: 'Users', icon: 'users' },
		{ href: '/admin/groups', label: 'Groups', icon: 'group' },
		{ href: '/admin/modules', label: 'Modules', icon: 'grid' },
		{ href: '/admin/templates', label: 'Templates', icon: 'template' },
		{ href: '/admin/workflows', label: 'Workflows', icon: 'workflow' },
		{ href: '/admin/oidc', label: 'OIDC / SSO', icon: 'shield' },
		{ href: '/admin/integrations', label: 'Integrations', icon: 'plug' },
		{ href: '/admin/security', label: 'Security', icon: 'lock' },
		{ href: '/admin/audit', label: 'Audit Log', icon: 'list' }
	];

	$: if (browser && !$authStore.isLoading && !$authStore.user?.is_admin) {
		goto('/dashboard');
	}
</script>

{#if $authStore.isLoading}
	<div class="flex h-screen items-center justify-center">
		<span class="loading loading-lg loading-spinner"></span>
	</div>
{:else if $authStore.user?.is_admin}
	<div class="flex h-screen overflow-hidden">
		<!-- Admin Sidebar -->
		<aside class="flex h-screen w-64 flex-shrink-0 flex-col bg-neutral text-neutral-content">
			<div class="flex items-center gap-2 border-b border-neutral-700 p-4">
				<a href="/files" class="text-xl font-bold transition-opacity hover:opacity-80">RustShare</a>
				<span class="ml-auto badge badge-sm badge-warning">Admin</span>
			</div>

			<nav class="flex-1 overflow-y-auto p-4">
				<p class="mb-3 px-2 text-xs font-semibold tracking-wider text-neutral-400 uppercase">
					Administration
				</p>
				<ul class="menu menu-sm p-0">
					{#each navItems as item}
						<li>
							<a
								href={item.href}
								class="flex items-center gap-2 rounded-lg px-3 py-2 transition-colors hover:bg-neutral-700"
								class:bg-neutral-600={$page.url.pathname.startsWith(item.href)}
								class:font-semibold={$page.url.pathname.startsWith(item.href)}
							>
								{#if item.icon === 'users'}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										class="h-4 w-4 shrink-0"
										fill="none"
										viewBox="0 0 24 24"
										stroke="currentColor"
										stroke-width="1.5"
									>
										<path
											stroke-linecap="round"
											stroke-linejoin="round"
											d="M15 19.128a9.38 9.38 0 002.625.372 9.337 9.337 0 004.121-.952 4.125 4.125 0 00-7.533-2.493M15 19.128v-.003c0-1.113-.285-2.16-.786-3.07M15 19.128v.106A12.318 12.318 0 018.624 21c-2.331 0-4.512-.645-6.374-1.766l-.001-.109a6.375 6.375 0 0111.964-3.07M12 6.375a3.375 3.375 0 11-6.75 0 3.375 3.375 0 016.75 0zm8.25 2.25a2.625 2.625 0 11-5.25 0 2.625 2.625 0 015.25 0z"
										/>
									</svg>
								{:else if item.icon === 'group'}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										class="h-4 w-4 shrink-0"
										fill="none"
										viewBox="0 0 24 24"
										stroke="currentColor"
										stroke-width="1.5"
									>
										<path
											stroke-linecap="round"
											stroke-linejoin="round"
											d="M18 18.72a9.094 9.094 0 003.741-.479 3 3 0 00-4.682-2.72m.94 3.198l.001.031c0 .225-.012.447-.037.666A11.944 11.944 0 0112 21c-2.17 0-4.207-.576-5.963-1.584A6.062 6.062 0 016 18.719m12 0a5.971 5.971 0 00-.941-3.197m0 0A5.995 5.995 0 0012 12.75a5.995 5.995 0 00-5.058 2.772m0 0a3 3 0 00-4.681 2.72 8.986 8.986 0 003.74.477m.94-3.197a5.971 5.971 0 00-.94 3.197M15 6.75a3 3 0 11-6 0 3 3 0 016 0zm6 3a2.25 2.25 0 11-4.5 0 2.25 2.25 0 014.5 0zm-13.5 0a2.25 2.25 0 11-4.5 0 2.25 2.25 0 014.5 0z"
										/>
									</svg>
								{:else if item.icon === 'shield'}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										class="h-4 w-4 shrink-0"
										fill="none"
										viewBox="0 0 24 24"
										stroke="currentColor"
										stroke-width="1.5"
									>
										<path
											stroke-linecap="round"
											stroke-linejoin="round"
											d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z"
										/>
									</svg>
								{:else if item.icon === 'plug'}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										class="h-4 w-4 shrink-0"
										fill="none"
										viewBox="0 0 24 24"
										stroke="currentColor"
										stroke-width="1.5"
									>
										<path
											stroke-linecap="round"
											stroke-linejoin="round"
											d="M13.19 8.688a4.5 4.5 0 011.242 7.244l-4.5 4.5a4.5 4.5 0 01-6.364-6.364l1.757-1.757m13.35-.622l1.757-1.757a4.5 4.5 0 00-6.364-6.364l-4.5 4.5a4.5 4.5 0 001.242 7.244"
										/>
									</svg>
								{:else if item.icon === 'workflow'}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										class="h-4 w-4 shrink-0"
										fill="none"
										viewBox="0 0 24 24"
										stroke="currentColor"
										stroke-width="1.5"
									>
										<path
											stroke-linecap="round"
											stroke-linejoin="round"
											d="M7.5 21 3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5"
										/>
									</svg>
								{:else if item.icon === 'lock'}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										class="h-4 w-4 shrink-0"
										fill="none"
										viewBox="0 0 24 24"
										stroke="currentColor"
										stroke-width="1.5"
									>
										<path
											stroke-linecap="round"
											stroke-linejoin="round"
											d="M16.5 10.5V6.75a4.5 4.5 0 10-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 002.25-2.25v-6.75a2.25 2.25 0 00-2.25-2.25H6.75a2.25 2.25 0 00-2.25 2.25v6.75a2.25 2.25 0 002.25 2.25z"
										/>
									</svg>
							{:else if item.icon === 'grid'}
								<svg
									xmlns="http://www.w3.org/2000/svg"
									class="h-4 w-4 shrink-0"
									fill="none"
									viewBox="0 0 24 24"
									stroke="currentColor"
									stroke-width="1.5"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M3.75 6A2.25 2.25 0 016 3.75h2.25A2.25 2.25 0 0110.5 6v2.25a2.25 2.25 0 01-2.25 2.25H6a2.25 2.25 0 01-2.25-2.25V6zM3.75 15.75A2.25 2.25 0 016 13.5h2.25a2.25 2.25 0 012.25 2.25V18a2.25 2.25 0 01-2.25 2.25H6A2.25 2.25 0 013.75 18v-2.25zM13.5 6a2.25 2.25 0 012.25-2.25H18A2.25 2.25 0 0120.25 6v2.25A2.25 2.25 0 0118 10.5h-2.25a2.25 2.25 0 01-2.25-2.25V6zM13.5 15.75a2.25 2.25 0 012.25-2.25H18a2.25 2.25 0 012.25 2.25V18A2.25 2.25 0 0118 20.25h-2.25A2.25 2.25 0 0113.5 18v-2.25z"
									/>
								</svg>
							{:else if item.icon === 'template'}
								<svg
									xmlns="http://www.w3.org/2000/svg"
									class="h-4 w-4 shrink-0"
									fill="none"
									viewBox="0 0 24 24"
									stroke="currentColor"
									stroke-width="1.5"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
									/>
								</svg>
								{:else}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										class="h-4 w-4 shrink-0"
										fill="none"
										viewBox="0 0 24 24"
										stroke="currentColor"
										stroke-width="1.5"
									>
										<path
											stroke-linecap="round"
											stroke-linejoin="round"
											d="M8.25 6.75h12M8.25 12h12m-12 5.25h12M3.75 6.75h.007v.008H3.75V6.75zm.375 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zM3.75 12h.007v.008H3.75V12zm.375 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zm-.375 5.25h.007v.008H3.75v-.008zm.375 0a.375.375 0 11-.75 0 .375.375 0 01.75 0z"
										/>
									</svg>
								{/if}
								{item.label}
							</a>
						</li>
					{/each}
				</ul>
			</nav>

			<div class="border-t border-neutral-700 p-4">
				<a
					href="/files"
					class="btn btn-block border-neutral-500 text-neutral-content btn-outline btn-sm hover:border-neutral-400 hover:bg-neutral-700"
				>
					Back to App
				</a>
			</div>
		</aside>

		<!-- Main Content -->
		<div class="flex flex-1 flex-col overflow-hidden">
			<!-- Admin Header -->
			<header
				class="flex flex-shrink-0 items-center gap-3 border-b border-base-300 bg-base-100 px-6 py-3"
			>
				<h1 class="text-lg font-semibold text-base-content">Admin Panel</h1>
				<span class="badge badge-warning">Admin</span>
				<div class="ml-auto flex items-center gap-2 text-sm text-base-content/60">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						class="h-4 w-4"
						fill="none"
						viewBox="0 0 24 24"
						stroke="currentColor"
						stroke-width="1.5"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z"
						/>
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
