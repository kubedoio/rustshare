<script lang="ts">
  import { goto } from '$app/navigation';
</script>

<svelte:head>
  <title>Shared with Me - RustShare</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <h1 class="text-3xl font-bold">Shared with Me</h1>
  </div>

  <!-- Coming Soon Message -->
  <div class="card bg-base-100 shadow-xl">
    <div class="card-body flex flex-col items-center justify-center py-16">
      <div class="text-6xl mb-4">👥</div>
      <h2 class="text-2xl font-bold mb-2">Coming Soon</h2>
      <p class="text-base-content/70 mb-6 text-center max-w-md">
        User-to-user file sharing will be available in a future update.
        Currently, you can create public share links from the Files page.
      </p>
      <button class="btn btn-primary" on:click={() => goto('/files')}>
        Go to My Files
      </button>
    </div>
  </div>
</div>
