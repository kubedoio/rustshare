<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { authStore, isAuthenticated } from '$lib/stores/auth';
  import Sidebar from '$lib/components/layout/Sidebar.svelte';
  import Header from '$lib/components/layout/Header.svelte';

  // Check authentication on mount
  onMount(() => {
    if (!$isAuthenticated) {
      goto('/login');
    }
  });

  // Redirect if auth state changes using $effect
  $effect(() => {
    if (!$isAuthenticated) {
      goto('/login');
    }
  });
</script>

{#if $isAuthenticated}
  <div class="flex h-screen overflow-hidden">
    <Sidebar />

    <div class="flex-1 flex flex-col overflow-hidden">
      <Header>
        <slot slot="breadcrumbs" name="breadcrumbs" />
      </Header>

      <main class="flex-1 overflow-auto bg-base-200 p-6">
        <slot />
      </main>
    </div>
  </div>
{:else}
  <div class="flex items-center justify-center h-screen">
    <span class="loading loading-spinner loading-lg"></span>
  </div>
{/if}
