<script lang="ts">
  import { toastStore } from '$lib/stores/toast';

  $: toasts = $toastStore;

  function handleClose(id: string) {
    toastStore.dismiss(id);
  }
</script>

{#if toasts.length > 0}
  <div class="toast toast-end toast-top z-50">
    {#each toasts as toast (toast.id)}
      <div class="alert {toast.type === 'success' ? 'alert-success' : toast.type === 'error' ? 'alert-error' : 'alert-info'} shadow-lg">
        <div class="flex items-center justify-between gap-2 w-full">
          <span>{toast.message}</span>
          <button
            class="btn btn-xs btn-ghost"
            on:click={() => handleClose(toast.id)}
            aria-label="Close notification"
          >
            ✕
          </button>
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
