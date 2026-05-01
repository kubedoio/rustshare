<script lang="ts">
	import { page } from '$app/stores';
	import { authStore } from '$lib/stores/auth';
	import ModuleIcon from '$lib/components/dashboard/ModuleIcon.svelte';
	import { getSidebarModulesForUser } from '$lib/modules/registry';

	export let mobileOpen = false;
	export let onClose: () => void = () => {};

	$: sidebarModules = getSidebarModulesForUser($authStore.user);

	function handleLogout() {
		authStore.logout();
	}

	function handleNavClick() {
		onClose();
	}
</script>

<!-- Mobile overlay -->
{#if mobileOpen}
	<div
		class="fixed inset-0 z-40 bg-black/50 lg:hidden"
		on:click={onClose}
		on:keydown={(e) => e.key === 'Escape' && onClose()}
		role="button"
		tabindex="0"
	></div>
{/if}

<!-- Sidebar -->
<aside
	class="fixed z-50 flex h-screen w-64 flex-col border-r border-base-300 bg-base-100 transition-transform duration-300 lg:static {mobileOpen
		? 'translate-x-0'
		: '-translate-x-full lg:translate-x-0'}"
>
	<div class="flex items-center justify-between border-b border-base-300 p-4">
		<h1 class="text-2xl font-bold">RustShare</h1>

		<!-- Close button (mobile only) -->
		<button
			class="btn btn-circle btn-ghost btn-sm lg:hidden"
			aria-label="Close navigation menu"
			on:click={onClose}
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="h-6 w-6"
			>
				<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
			</svg>
		</button>
	</div>

	<nav class="flex-1 p-4 overflow-y-auto">
		<ul class="menu flex flex-col gap-1">
			<li>
				<a
					href="/dashboard"
					class:active={$page.url.pathname === '/dashboard'}
					class="flex items-center gap-2"
					on:click={handleNavClick}
				>
					<ModuleIcon name="layout-dashboard" size={18} />
					<span>Dashboard</span>
				</a>
			</li>
			<li>
				<a
					href="/files"
					class:active={$page.url.pathname.startsWith('/files')}
					class="flex items-center gap-2"
					on:click={handleNavClick}
				>
					<ModuleIcon name="folder" size={18} />
					<span>My Files</span>
				</a>
			</li>
			<li>
				<a
					href="/shared-with-me"
					class:active={$page.url.pathname.startsWith('/shared-with-me')}
					class="flex items-center gap-2"
					on:click={handleNavClick}
				>
					<ModuleIcon name="users" size={18} />
					<span>Shared with Me</span>
				</a>
			</li>

			<div class="divider my-1"></div>

			{#each sidebarModules as mod (mod.key)}
				<li>
					<a
						href={`/modules/${mod.key}`}
						class:active={$page.url.pathname.startsWith(`/modules/${mod.key}`)}
						class="flex items-center gap-2"
						on:click={handleNavClick}
					>
						<ModuleIcon name={mod.ui.sidebar.icon} size={18} />
						<span>{mod.ui.sidebar.label}</span>
					</a>
				</li>
			{/each}
		</ul>
	</nav>

	<div class="border-t border-base-300 p-4 flex flex-col gap-2">
		<ul class="menu w-full p-0">
			<li>
				<a
					href="/settings"
					class:active={$page.url.pathname.startsWith('/settings')}
					class="flex items-center gap-2"
					on:click={handleNavClick}
				>
					<ModuleIcon name="settings" size={18} />
					<span>Settings</span>
				</a>
			</li>
		</ul>
		<button class="btn btn-block btn-outline mt-2" on:click={handleLogout}> Logout </button>
	</div>
</aside>
