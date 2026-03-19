<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { authStore, isAuthenticated } from '$lib/stores/auth';
  import { showKeyboardShortcuts } from '$lib/stores/ui';
  import Sidebar from '$lib/components/layout/Sidebar.svelte';
  import Header from '$lib/components/layout/Header.svelte';
  import KeyboardShortcuts from '$lib/components/common/KeyboardShortcuts.svelte';

  import { browser } from '$app/environment';

  let mobileMenuOpen = false;
  let mounted = false;

  // Check authentication on mount
  onMount(() => {
    mounted = true;
    if (!$isAuthenticated) {
      goto('/login');
    }
  });

  // Redirect if auth state changes (only in browser after mount)
  $: if (browser && mounted && !$isAuthenticated) {
    goto('/login');
  }

  function toggleMobileMenu() {
    mobileMenuOpen = !mobileMenuOpen;
  }

  function closeMobileMenu() {
    mobileMenuOpen = false;
  }

  function showHelp() {
    showKeyboardShortcuts.set(true);
  }
</script>

{#if $isAuthenticated}
  <div class="flex h-screen overflow-hidden">
    <Sidebar mobileOpen={mobileMenuOpen} onClose={closeMobileMenu} />

    <div class="flex-1 flex flex-col overflow-hidden">
      <Header onMenuClick={toggleMobileMenu} onHelpClick={showHelp}>
        <slot slot="breadcrumbs" name="breadcrumbs" />
      </Header>

      <main class="flex-1 overflow-auto bg-base-200 p-4 lg:p-6">
        <slot />
      </main>
    </div>
  </div>

  <KeyboardShortcuts
    open={$showKeyboardShortcuts}
    on:close={() => showKeyboardShortcuts.set(false)}
  />
{:else}
  <div class="flex items-center justify-center h-screen">
    <span class="loading loading-spinner loading-lg"></span>
  </div>
{/if}
