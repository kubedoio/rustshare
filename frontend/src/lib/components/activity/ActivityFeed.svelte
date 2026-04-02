<script lang="ts">
  import { activityStore, getActivityDisplay, getRelativeTime } from '$lib/stores/activity';
  import type { Activity } from '$lib/stores/activity';

  export let maxItems = 10;
  export let showClearButton = true;
  export let showHeader = true;

  $: recentActivities = $activityStore.slice(0, maxItems);

  function handleClearHistory() {
    if (confirm('Clear all activity history?')) {
      activityStore.clearHistory();
    }
  }

  function handleRemoveActivity(id: string) {
    activityStore.removeActivity(id);
  }
</script>

<div class="space-y-4">
  <!-- Header -->
  {#if showHeader}
    <div class="flex items-center justify-between">
      <h3 class="text-lg font-semibold">Recent Activity</h3>
      {#if showClearButton && $activityStore.length > 0}
        <button
          class="btn btn-ghost btn-xs text-[11px]"
          on:click={handleClearHistory}
          title="Clear all history"
        >
          Clear All
        </button>
      {/if}
    </div>
  {/if}

  <!-- Activity List -->
  {#if recentActivities.length === 0}
    <div class="text-center py-8 text-base-content/60">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        fill="none"
        viewBox="0 0 24 24"
        stroke-width="1.5"
        stroke="currentColor"
        class="w-12 h-12 mx-auto mb-2 opacity-50"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
        />
      </svg>
      <p class="text-[12px]">No recent activity</p>
    </div>
  {:else}
    <div class="space-y-2">
      {#each recentActivities as activity (activity.id)}
        {@const display = getActivityDisplay(activity)}
        <div class="flex items-start gap-3 p-3 rounded-lg hover:bg-base-200/50 transition-colors group">
          <div class="text-lg flex-shrink-0 mt-0.5">
            {display.icon}
          </div>
          <div class="flex-1 min-w-0">
            <p class="text-[13px] font-medium leading-tight {display.color}">
              {display.title}
            </p>
            <p class="text-[12px] text-base-content/70 truncate mt-0.5">
              {display.description}
            </p>
            <p class="text-[10px] text-base-content/40 mt-1 uppercase tracking-wider font-semibold">
              {getRelativeTime(activity.timestamp)}
            </p>
          </div>
          <button
            class="btn btn-ghost btn-xs btn-circle opacity-0 group-hover:opacity-100 transition-opacity"
            on:click={() => handleRemoveActivity(activity.id)}
            title="Remove"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="1.5"
              stroke="currentColor"
              class="w-4 h-4 text-base-content/50 hover:text-error"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      {/each}
    </div>

    {#if $activityStore.length > maxItems}
      <div class="text-center pt-2">
        <p class="text-[10px] font-medium uppercase tracking-wider text-base-content/40">
          Showing {maxItems} of {$activityStore.length} activities
        </p>
      </div>
    {/if}
  {/if}
</div>
