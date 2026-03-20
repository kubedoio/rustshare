<script lang="ts">
  import { goto } from '$app/navigation';
  import { toastStore } from '$lib/stores/toast';

  $: toasts = $toastStore;

  function handleClose(id: string) {
    toastStore.dismiss(id);
  }

  function handleAction(id: string, href: string) {
    toastStore.dismiss(id);
    goto(href);
  }
</script>

{#if toasts.length > 0}
  <div class="toast toast-end toast-top z-50">
    {#each toasts as toast (toast.id)}
      <div class="alert {toast.type === 'success' ? 'alert-success' : toast.type === 'error' ? 'alert-error' : 'alert-info'} shadow-lg">
        <div class="flex items-center justify-between gap-2 w-full">
          <span class="flex-1">{toast.message}</span>
          <div class="flex items-center gap-2">
            {#if toast.actionHref}
              <button
                class="btn btn-xs btn-ghost"
                on:click={() => toast.actionHref && handleAction(toast.id, toast.actionHref)}
              >
                {toast.actionLabel || 'Open'}
              </button>
            {/if}
            <button
              class="btn btn-xs btn-ghost"
              on:click={() => handleClose(toast.id)}
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
