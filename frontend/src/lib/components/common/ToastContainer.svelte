<script lang="ts">
  import { toastStore } from '$lib/stores/toast';

  function handleClose(id: string) {
    toastStore.dismiss(id);
  }
</script>

{#if $toastStore.length > 0}
  <div class="toast toast-end toast-top z-[100] mt-16" role="region" aria-label="Notifications">
    {#each $toastStore as toast (toast.id)}
      <div
        class="alert {toast.type === 'success' ? 'alert-success' : toast.type === 'error' ? 'alert-error' : 'alert-info'} shadow-lg"
        role={toast.type === 'error' ? 'alert' : 'status'}
        aria-live={toast.type === 'error' ? 'assertive' : 'polite'}
      >
        <div class="flex items-center justify-between gap-2 w-full">
          <span class="flex-1">{toast.message}</span>
          <div class="flex items-center gap-2">
            {#if toast.actionHref}
              <a
                class="btn btn-xs btn-ghost"
                href={toast.actionHref}
                onclick={() => toastStore.dismiss(toast.id)}
              >
                {toast.actionLabel || 'Open'}
              </a>
            {/if}
            <button
              class="btn btn-xs btn-ghost"
              onclick={() => handleClose(toast.id)}
              aria-label="Close notification"
            >
              ✕
            </button>
          </div>
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast {
    position: fixed;
  }
</style>
