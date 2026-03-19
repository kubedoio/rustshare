<script lang="ts">
  import { onMount } from 'svelte';
  import type { File } from '$lib/api/types';

  export let file: File;
  export let size: 'sm' | 'md' | 'lg' = 'md';

  const isImage = (mimeType: string) => {
    return mimeType.startsWith('image/');
  };

  // Initialize loading based on whether this is an image
  let thumbnailUrl: string | null = null;
  let loading = isImage(file.mime_type);
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

  async function generateThumbnail() {
    if (!isImage(file.mime_type)) {
      loading = false;
      return;
    }

    try {
      const apiBase = import.meta.env.VITE_API_URL || '/api/v1';

      // Get download URL from backend
      const response = await fetch(`${apiBase}/files/${file.id}/download`, {
        credentials: 'include'
      });

      if (!response.ok) {
        throw new Error('Failed to get download URL');
      }

      const { url } = await response.json();

      // Convert MinIO URL to nginx proxy path
      // MinIO returns: http://rustfs:9000/rustshare-files/path/to/file
      // We want: /storage/path/to/file
      let imageUrl = url;
      if (url.includes('/rustshare-files/')) {
        const path = url.split('/rustshare-files/')[1];
        imageUrl = `/storage/${path}`;
      }

      // Load image
      const img = new Image();
      img.crossOrigin = 'anonymous';

      img.onload = () => {
        // Create canvas
        const canvas = document.createElement('canvas');
        const ctx = canvas.getContext('2d');
        if (!ctx) {
          error = true;
          loading = false;
          return;
        }

        // Calculate thumbnail size (maintaining aspect ratio)
        const maxSize = size === 'lg' ? 96 : size === 'md' ? 64 : 40;
        let width = img.width;
        let height = img.height;

        if (width > height) {
          if (width > maxSize) {
            height = (height * maxSize) / width;
            width = maxSize;
          }
        } else {
          if (height > maxSize) {
            width = (width * maxSize) / height;
            height = maxSize;
          }
        }

        canvas.width = width;
        canvas.height = height;

        // Draw image
        ctx.drawImage(img, 0, 0, width, height);

        // Convert to data URL
        thumbnailUrl = canvas.toDataURL('image/jpeg', 0.8);
        loading = false;
      };

      img.onerror = () => {
        error = true;
        loading = false;
      };

      img.src = imageUrl;
    } catch (err) {
      console.error('Failed to generate thumbnail:', err);
      error = true;
      loading = false;
    }
  }

  onMount(() => {
    if (isImage(file.mime_type)) {
      generateThumbnail();
    }
  });

  // Get file type icon emoji
  function getFileIcon(mimeType: string): string {
    if (isImage(mimeType)) return '🖼️';
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
  {:else if error || !isImage(file.mime_type)}
    <!-- Show file type icon -->
    <span class="text-2xl">{getFileIcon(file.mime_type)}</span>
  {:else if thumbnailUrl}
    <!-- Show thumbnail image -->
    <img
      src={thumbnailUrl}
      alt={file.name}
      class="w-full h-full object-cover"
    />
  {:else}
    <!-- Fallback icon -->
    <span class="text-2xl">{getFileIcon(file.mime_type)}</span>
  {/if}
</div>
