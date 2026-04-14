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
		class="inset-0 bg-black/50 lg:hidden fixed z-40"
		on:click={onClose}
		on:keydown={(e) => e.key === 'Escape' && onClose()}
		role="button"
		tabindex="0"
	></div>
{/if}

<!-- Sidebar -->
<aside
	class="w-64 bg-base-100 border-base-300 lg:static fixed z-50 flex h-screen flex-col border-r transition-transform duration-300 {mobileOpen
		? 'translate-x-0'
		: 'lg:translate-x-0 -translate-x-full'}"
>
	<div class="p-4 border-base-300 flex items-center justify-between border-b">
		<h1 class="text-2xl font-bold">RustShare</h1>

		<!-- Close button (mobile only) -->
		<button
			class="btn btn-ghost btn-sm btn-circle lg:hidden"
			aria-label="Close navigation menu"
			on:click={onClose}
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="w-6 h-6"
			>
				<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
			</svg>
		</button>
	</div>

	<nav class="p-4 flex-1">
		<ul class="menu">
			{#each navItems as item}
				<li>
					<a
						href={item.href}
						class:active={$page.url.pathname === item.href}
						class="gap-2 flex items-center"
						on:click={handleNavClick}
					>
						<span>{item.icon}</span>
						<span>{item.label}</span>
						{#if item.href === '/notifications' && $unreadNotificationsQuery.data && $unreadNotificationsQuery.data.count > 0}
							<span class="badge badge-primary badge-sm ml-auto">
								{$unreadNotificationsQuery.data.count}
							</span>
						{/if}
					</a>
				</li>
			{/each}
		</ul>
	</nav>

	<div class="p-4 border-base-300 border-t">
		<button class="btn btn-outline btn-block" on:click={handleLogout}> Logout </button>
	</div>
</aside>
