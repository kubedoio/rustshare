<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { authStore } from '$lib/stores/auth';
  import { showKeyboardShortcuts } from '$lib/stores/ui';
  import Sidebar from '$lib/components/layout/Sidebar.svelte';
  import Header from '$lib/components/layout/Header.svelte';
  import KeyboardShortcuts from '$lib/components/common/KeyboardShortcuts.svelte';

  import { browser } from '$app/environment';

  let mobileMenuOpen = false;
  let checkComplete = false;

  // Check authentication on mount
  onMount(() => {
    // Give the auth store a moment to initialize from localStorage
    setTimeout(() => {
      checkComplete = true;
      if (!$authStore.isAuthenticated) {
        goto('/login');
      }
    }, 0);
  });

  // Redirect if auth state changes (only after initial check)
  $: if (browser && checkComplete && !$authStore.isAuthenticated) {
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

{#if checkComplete}
  {#if $authStore.isAuthenticated}
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
    <!-- Will redirect to login in onMount -->
  {/if}
{:else}
  <div class="flex items-center justify-center h-screen">
    <span class="loading loading-spinner loading-lg"></span>
  </div>
{/if}
