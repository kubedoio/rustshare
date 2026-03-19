<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { authStore, currentUser } from '$lib/stores/auth';
  import { get } from 'svelte/store';

  onMount(() => {
    void (async () => {
      await authStore.initialize();
      const user = get(currentUser);
      if (user) {
        goto('/dashboard');
      } else {
        goto('/login');
      }
    })();
  });
</script>

<!-- Show nothing, just redirect -->
<div class="min-h-screen flex items-center justify-center">
  <div class="loading loading-spinner loading-lg"></div>
</div>
