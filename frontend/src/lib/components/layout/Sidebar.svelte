<script lang="ts">
  import { page } from '$app/stores';
  import { authStore } from '$lib/stores/auth';

  const navItems = [
    { href: '/files', label: 'My Files', icon: '📁' },
    { href: '/shared-with-me', label: 'Shared with Me', icon: '👥' },
    { href: '/notifications', label: 'Notifications', icon: '🔔' }
  ];

  function handleLogout() {
    authStore.logout();
  }
</script>

<aside class="w-64 bg-base-100 h-screen flex flex-col border-r border-base-300">
  <div class="p-4 border-b border-base-300">
    <h1 class="text-2xl font-bold">RustShare</h1>
  </div>

  <nav class="flex-1 p-4">
    <ul class="menu">
      {#each navItems as item}
        <li>
          <a
            href={item.href}
            class:active={$page.url.pathname === item.href}
            class="flex items-center gap-2"
          >
            <span>{item.icon}</span>
            <span>{item.label}</span>
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
