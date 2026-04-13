<script lang="ts">
  interface Props {
    open: boolean;
    title: string;
    onClose: () => void;
    showCloseButton?: boolean;
    class?: string;
  }

  let {
    open,
    title,
    onClose,
    showCloseButton = true,
    class: className = ''
  }: Props = $props();

  let dialogRef: HTMLDivElement | undefined = $state();
  let titleId = $derived(`modal-title-${Math.random().toString(36).slice(2)}`);

  // Prevent body scroll when open
  $effect(() => {
    if (open) {
      const originalOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      return () => {
        document.body.style.overflow = originalOverflow;
      };
    }
  });

  // Focus management: focus first focusable element when opened
  $effect(() => {
    if (open && dialogRef) {
      requestAnimationFrame(() => {
        const focusable = dialogRef?.querySelector<HTMLElement>(
          'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
        );
        focusable?.focus();
      });
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      onClose();
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm"
    onclick={handleBackdropClick}
  >
    <div
      bind:this={dialogRef}
      class="relative bg-base-100 rounded-xl shadow-2xl w-full max-w-md overflow-hidden {className}"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      tabindex="-1"
    >
      {#if showCloseButton}
        <button
          type="button"
          class="absolute top-3 right-3 text-base-content/50 hover:text-base-content p-1 rounded-lg hover:bg-base-200 transition-colors"
          aria-label="Close"
          onclick={onClose}
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
        </button>
      {/if}

      <div class="px-5 py-4 border-b border-base-300/50">
        <h3 id={titleId} class="text-lg font-semibold text-base-content pr-8">{title}</h3>
      </div>

      <div class="p-5">
        <slot />
      </div>
    </div>
  </div>
{/if}
