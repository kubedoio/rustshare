<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { page } from '$app/stores';
  import { listNotifications } from '$lib/api/notifications';
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
    queryFn: () =>
      listNotifications({
        unreadOnly: true,
        limit: 20
      })
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
    class="fixed inset-0 bg-black/50 z-40 lg:hidden"
    on:click={onClose}
    on:keydown={(e) => e.key === 'Escape' && onClose()}
    role="button"
    tabindex="0"
  ></div>
{/if}

<!-- Sidebar -->
<aside
  class="w-64 bg-base-100 h-screen flex flex-col border-r border-base-300 fixed lg:static z-50 transition-transform duration-300 {mobileOpen
    ? 'translate-x-0'
    : '-translate-x-full lg:translate-x-0'}"
>
  <div class="p-4 border-b border-base-300 flex items-center justify-between">
    <h1 class="text-2xl font-bold">RustShare</h1>

    <!-- Close button (mobile only) -->
    <button
      class="btn btn-ghost btn-sm btn-circle lg:hidden"
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
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M6 18L18 6M6 6l12 12"
        />
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
            {#if item.href === '/notifications' && $unreadNotificationsQuery.data && $unreadNotificationsQuery.data.total > 0}
              <span class="badge badge-primary badge-sm ml-auto">
                {$unreadNotificationsQuery.data.total}
              </span>
            {/if}
          </a>
        </li>
      {/each}
    </ul>
  </nav>

  <div class="p-4 border-t border-base-300">
    <button class="btn btn-outline btn-block" on:click={handleLogout}>
      Logout
    </button>
  </div>
</aside>
