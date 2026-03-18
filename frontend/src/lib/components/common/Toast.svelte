<script lang="ts">
  import { onMount } from 'svelte';

  export let message: string;
  export let type: 'success' | 'error' | 'info' = 'info';
  export let duration: number = 3000;
  export let onClose: () => void = () => {};

  let visible = true;

  onMount(() => {
    if (duration > 0) {
      const timer = setTimeout(() => {
        visible = false;
        setTimeout(onClose, 300); // Wait for fade out
      }, duration);

      return () => clearTimeout(timer);
    }
  });

  const alertClass = {
    success: 'alert-success',
    error: 'alert-error',
    info: 'alert-info'
  }[type];
</script>

{#if visible}
  <div class="toast toast-end toast-top z-50">
    <div class="alert {alertClass} shadow-lg">
      <span>{message}</span>
    </div>
  </div>
{/if}
