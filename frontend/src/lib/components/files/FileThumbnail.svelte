<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { File } from '$lib/api/types';

  export let file: File;
  export let size: 'sm' | 'md' | 'lg' = 'md';

  let thumbnailUrl: string | null = null;
  let loading = false;
  let error = false;

  const sizeClasses = {
    sm: 'w-10 h-10',
    md: 'w-16 h-16',
    lg: 'w-24 h-24'
  };

  $: sizeClass = sizeClasses[size];

  const isPDF = (mimeType: string) => {
    return mimeType === 'application/pdf';
  };

  const isVideo = (mimeType: string) => {
    return mimeType.startsWith('video/');
  };

  const isThumbnailSupported = (mimeType: string) => {
    const supported = [
      'image/jpeg', 'image/png', 'image/gif', 'image/webp', 'image/bmp',
      'application/pdf',
      'video/mp4', 'video/quicktime', 'video/webm'
    ];
    return supported.includes(mimeType.toLowerCase());
  };

  async function loadThumbnail() {
    if (!file?.id || !isThumbnailSupported(file.mime_type)) {
      loading = false;
      return;
    }

    // Clean up old thumbnail URL before loading new one
    if (thumbnailUrl) {
      URL.revokeObjectURL(thumbnailUrl);
      thumbnailUrl = null;
    }

    loading = true;

    try {
      const response = await fetch(`/api/v1/files/${file.id}/thumbnail?size=${size}`, {
        credentials: 'include'
      });

      if (response.ok) {
        const blob = await response.blob();
        thumbnailUrl = URL.createObjectURL(blob);
        error = false;
      } else {
        // 404, 415, 413 - show fallback icon
        error = true;
      }
    } catch (err) {
      console.error('Failed to load thumbnail:', err);
      error = true;
    } finally {
      loading = false;
    }
  }

  // Reactive: reload thumbnail when file changes
  $: if (file?.id) {
    loadThumbnail();
  }

  onDestroy(() => {
    if (thumbnailUrl) {
      URL.revokeObjectURL(thumbnailUrl);
    }
  });

  // Get file type icon emoji
  function getFileIcon(mimeType: string): string {
    if (mimeType.startsWith('image/')) return '🖼️';
    if (isPDF(mimeType)) return '📄';
    if (isVideo(mimeType)) return '🎬';
    if (mimeType.includes('text')) return '📝';
    if (mimeType.includes('audio')) return '🎵';
    if (mimeType.includes('zip') || mimeType.includes('archive')) return '📦';
    if (mimeType.includes('word')) return '📘';
    if (mimeType.includes('excel') || mimeType.includes('spreadsheet')) return '📊';
    if (mimeType.includes('powerpoint') || mimeType.includes('presentation')) return '📽️';
    return '📄';
  }
</script>

<div class={`${sizeClass} flex items-center justify-center bg-base-200 rounded overflow-hidden`}>
  {#if loading}
    <span class="loading loading-spinner loading-xs"></span>
  {:else if error || !thumbnailUrl}
    <!-- Show file type icon -->
    <span class="text-2xl">{getFileIcon(file.mime_type)}</span>
  {:else}
    <!-- Show thumbnail image -->
    <img
      src={thumbnailUrl}
      alt={file.name}
      class="w-full h-full object-cover"
    />
  {/if}
</div>
