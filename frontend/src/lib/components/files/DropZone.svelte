<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let disabled = false;

  type DispatchEvents = { filesDropped: globalThis.File[] }
  const dispatch = createEventDispatcher<DispatchEvents>();

  let isDragging = false;
  let dragCounter = 0;

  function isFileDrag(event: DragEvent) {
    return event.dataTransfer?.types?.includes('Files') ?? false;
  }

  function handleDragEnter(event: DragEvent) {
    dragCounter++;
    if (isFileDrag(event)) {
      isDragging = true;
    }
  }

  function handleDragLeave(event: DragEvent) {
    dragCounter--;
    if (dragCounter === 0) {
      isDragging = false;
    }
  }

  function handleDragOver(event: DragEvent) {
    if (!isFileDrag(event)) return;
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = disabled ? 'none' : 'copy';
    }
  }

  function handleDrop(event: DragEvent) {
    event.preventDefault();
    isDragging = false;
    dragCounter = 0;

    if (disabled) return;

    const files = event.dataTransfer?.files;
    if (files && files.length > 0) {
      dispatch('filesDropped', Array.from(files));
    }
  }
</script>

<div
  class="relative"
  on:dragenter={handleDragEnter}
  on:dragleave={handleDragLeave}
  on:dragover={handleDragOver}
  on:drop={handleDrop}
  role="region"
  aria-label="File drop zone"
>
  <slot />

  {#if isDragging && !disabled}
    <div
      class="absolute inset-0 bg-primary/10 border-4 border-dashed border-primary rounded-lg flex items-center justify-center z-40"
    >
      <div class="text-center pointer-events-none">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
          stroke-width="1.5"
          stroke="currentColor"
          class="w-16 h-16 mx-auto text-primary"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"
          />
        </svg>
        <p class="mt-4 text-lg font-semibold text-primary">Drop files to upload</p>
      </div>
    </div>
  {/if}
</div>
