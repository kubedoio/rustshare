<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { page } from '$app/stores';
	import { getUnreadNotificationCount } from '$lib/api/notifications';
	import { authStore } from '$lib/stores/auth';

	export let mobileOpen = false;
	export let onClose: () => void = () => {};

	const navItems = [
		{ href: '/dashboard', label: 'Dashboard', icon: '🏠' },
		{ href: '/files', label: 'My Files', icon: '📁' },
		{ href: '/shared-with-me', label: 'Shared with Me', icon: '👥' },
		{ href: '/notifications', label: 'Notifications', icon: '🔔' },
		{ href: '/settings', label: 'Settings', icon: '⚙️' }
	];

	const unreadNotificationsQuery = createQuery({
		queryKey: ['notifications', 'sidebar-unread-count'],
		queryFn: getUnreadNotificationCount
	});

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

	<nav class="flex-1 p-4">
		<ul class="menu">
			{#each navItems as item}
				<li>
					<a
						href={item.href}
						class:active={$page.url.pathname === item.href}
						class="flex items-center gap-2"
						on:click={handleNavClick}
					>
						<span>{item.icon}</span>
						<span>{item.label}</span>
						{#if item.href === '/notifications' && $unreadNotificationsQuery.data && $unreadNotificationsQuery.data.count > 0}
							<span class="ml-auto badge badge-sm badge-primary">
								{$unreadNotificationsQuery.data.count}
							</span>
						{/if}
					</a>
				</li>
			{/each}
		</ul>
	</nav>

	<div class="border-t border-base-300 p-4">
		<button class="btn btn-block btn-outline" on:click={handleLogout}> Logout </button>
	</div>
</aside>
