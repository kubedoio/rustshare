<script lang="ts">
  import { activityStore } from '$lib/stores/activity';
  import ActivityFeed from '$lib/components/activity/ActivityFeed.svelte';
</script>

<svelte:head>
  <title>Notifications - RustShare</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <h1 class="text-3xl font-bold">Notifications</h1>
  </div>

  <!-- Activity as Notifications -->
  <div class="card bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title">Recent Activity</h2>
      <p class="text-sm text-base-content/70 mb-4">
        Track your file operations and system events.
      </p>
      <ActivityFeed maxItems={50} showClearButton={true} />
    </div>
  </div>

  <!-- Info -->
  <div class="alert alert-info">
    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="stroke-current shrink-0 w-6 h-6">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
    </svg>
    <span>
      Real-time notifications for shares, collaborations, and system events will be added in a future update.
    </span>
  </div>
</div>
