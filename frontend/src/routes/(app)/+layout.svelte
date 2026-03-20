<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { authStore } from '$lib/stores/auth';
  import { showKeyboardShortcuts } from '$lib/stores/ui';
  import { searchQuery } from '$lib/stores/search';
  import Sidebar from '$lib/components/layout/Sidebar.svelte';
  import Header from '$lib/components/layout/Header.svelte';
  import KeyboardShortcuts from '$lib/components/common/KeyboardShortcuts.svelte';
  import ToastContainer from '$lib/components/common/ToastContainer.svelte';

  import { browser } from '$app/environment';

  let mobileMenuOpen = false;
  let checkComplete = false;

  // Check authentication on mount
  onMount(() => {
    console.log('[Layout] onMount - authStore:', $authStore);
  });

  $: if (!$authStore.isLoading) {
    checkComplete = true;
  }

  // Redirect if auth state changes (only after initial check)
  $: if (browser && checkComplete && !$authStore.isLoading && !$authStore.isAuthenticated) {
    console.log('[Layout] Reactive redirect - auth state changed to unauthenticated');
    goto('/login');
  }

  // Show search only on files page
  $: showSearch = $page.url.pathname === '/files';

  // Clear search when navigating away from files page
  $: if (!showSearch) {
    searchQuery.set('');
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

  function handleSearchChange(query: string) {
    searchQuery.set(query);
  }
</script>

{#if checkComplete}
  {#if $authStore.isAuthenticated}
    <div class="flex h-screen overflow-hidden">
      <Sidebar mobileOpen={mobileMenuOpen} onClose={closeMobileMenu} />

      <div class="flex-1 flex flex-col overflow-hidden">
        <Header
          onMenuClick={toggleMobileMenu}
          onHelpClick={showHelp}
          onSearchChange={showSearch ? handleSearchChange : null}
        >
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

    <!-- Global Toast Notifications -->
    <ToastContainer />
  {:else}
    <!-- Will redirect to login in onMount -->
  {/if}
{:else}
  <div class="flex items-center justify-center h-screen">
    <span class="loading loading-spinner loading-lg"></span>
  </div>
{/if}
