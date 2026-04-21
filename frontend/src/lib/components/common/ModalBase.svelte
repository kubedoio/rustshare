<script lang="ts">
  import { X } from 'lucide-svelte';

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

  let dialogRef: HTMLDialogElement | undefined = $state();
  let titleId = $derived(`modal-title-${Math.random().toString(36).slice(2)}`);
  let isProgrammaticClose = false;

  // Show/close dialog based on open prop
  $effect(() => {
    if (open) {
      if (dialogRef && !dialogRef.open) {
        dialogRef.showModal();
      }
    } else {
      if (dialogRef?.open) {
        isProgrammaticClose = true;
        dialogRef.close();
      }
    }
  });

  // Backdrop click to close
  $effect(() => {
    if (!dialogRef) return;
    const handler = (e: MouseEvent) => {
      if (e.target === dialogRef) {
        dialogRef?.close();
      }
    };
    dialogRef.addEventListener('click', handler);
    return () => dialogRef?.removeEventListener('click', handler);
  });

  // Handle close event (Escape or close())
  function handleClose() {
    if (isProgrammaticClose) {
      isProgrammaticClose = false;
      return;
    }
    onClose();
  }

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
</script>

<dialog
  bind:this={dialogRef}
  class="fixed inset-0 z-50 m-0 p-4 bg-black/60 backdrop-blur-sm max-w-none max-h-none w-full h-full open:flex open:items-center open:justify-center"
  onclose={handleClose}
  aria-modal="true"
  aria-labelledby={titleId}
>
  <div
    class="relative bg-base-100 rounded-xl shadow-2xl w-full max-w-md overflow-hidden {className}"
  >
    {#if showCloseButton}
      <button
        type="button"
        class="absolute top-3 right-3 text-base-content/50 hover:text-base-content p-1 rounded-lg hover:bg-base-200 transition-colors"
        aria-label="Close"
        onclick={() => dialogRef?.close()}
      >
        <X size={20} />
      </button>
    {/if}

    <div class="px-5 py-4 border-b border-base-300/50">
      <h3 id={titleId} class="text-lg font-semibold text-base-content pr-8">{title}</h3>
    </div>

    <div class="p-5">
      <slot />
    </div>
  </div>
</dialog>
