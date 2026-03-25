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

  const isThumbnailSupported = (mimeType: string, fileName: string) => {
    // Images - always supported
    if (mimeType.startsWith('image/')) {
      return ['image/jpeg', 'image/png', 'image/gif', 'image/webp', 'image/bmp', 'image/svg+xml'].includes(mimeType.toLowerCase()) ||
             mimeType.startsWith('image/'); // Support all images
    }
    
    // PDF
    if (mimeType === 'application/pdf') return true;
    
    // Videos
    const videoTypes = ['video/mp4', 'video/quicktime', 'video/webm', 'video/avi', 'video/mpeg'];
    if (videoTypes.includes(mimeType.toLowerCase())) return true;
    
    // Special file types based on extension
    const lowerName = fileName.toLowerCase();
    if (lowerName.endsWith('.excalidraw') || lowerName.endsWith('.excalidraw.json')) return true;
    if (lowerName.endsWith('.drawio') || lowerName.endsWith('.dio')) return true;
    
    return false;
  };

  async function loadThumbnail() {
    if (!file?.id || !isThumbnailSupported(file.mime_type, file.name)) {
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
  function getFileIcon(mimeType: string, fileName: string): string {
    const lowerName = fileName.toLowerCase();
    
    // Special file types
    if (lowerName.endsWith('.excalidraw') || lowerName.endsWith('.excalidraw.json')) return '✏️';
    if (lowerName.endsWith('.drawio') || lowerName.endsWith('.dio')) return '📐';
    
    // Standard MIME types
    if (mimeType.startsWith('image/')) return '🖼️';
    if (isPDF(mimeType)) return '📄';
    if (isVideo(mimeType)) return '🎬';
    if (mimeType.startsWith('audio/')) return '🎵';
    if (mimeType.includes('text')) return '📝';
    if (mimeType.includes('zip') || mimeType.includes('archive') || mimeType.includes('compressed')) return '📦';
    if (mimeType.includes('word') || mimeType.includes('document')) return '📘';
    if (mimeType.includes('excel') || mimeType.includes('spreadsheet') || mimeType.includes('sheet')) return '📊';
    if (mimeType.includes('powerpoint') || mimeType.includes('presentation')) return '📽️';
    if (mimeType.includes('json') || mimeType.includes('xml') || mimeType.includes('yaml')) return '📋';
    if (mimeType.includes('javascript') || mimeType.includes('typescript') || mimeType.includes('python')) return '💻';
    return '📄';
  }
</script>

<div class={`${sizeClass} flex items-center justify-center bg-base-200 rounded overflow-hidden`}>
  {#if loading}
    <span class="loading loading-spinner loading-xs"></span>
  {:else if error || !thumbnailUrl}
    <!-- Show file type icon -->
    <span class="text-2xl">{getFileIcon(file.mime_type, file.name)}</span>
  {:else}
    <!-- Show thumbnail image -->
    <img
      src={thumbnailUrl}
      alt={file.name}
      class="w-full h-full object-cover"
    />
  {/if}
</div>
