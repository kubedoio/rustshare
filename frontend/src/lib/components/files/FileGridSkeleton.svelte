<script lang="ts">
  export let count = 4;
</script>

<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
  {#each Array(count) as _}
    <div class="card bg-base-100 shadow-sm">
      <div class="card-body p-3 lg:p-4">
        <div class="flex items-center gap-2 lg:gap-3">
          <div class="w-8 h-8 lg:w-10 lg:h-10 bg-base-300 animate-pulse rounded"></div>
          <div class="flex-1 space-y-2">
            <div class="h-4 bg-base-300 animate-pulse rounded w-3/4"></div>
            <div class="h-3 bg-base-300 animate-pulse rounded w-1/2"></div>
          </div>
        </div>
      </div>
    </div>
  {/each}
</div>
