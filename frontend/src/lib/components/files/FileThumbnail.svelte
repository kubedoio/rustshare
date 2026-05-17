<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { File } from '$lib/api/types';
	import {
		FileText,
		FileVideoCamera as FileVideo,
		File as FileAudio,
		Package,
		FileType,
		FileSpreadsheet,
		Presentation,
		File as FileDefault
	} from 'lucide-svelte';

	let {
		file,
		size = 'md'
	}: {
		file: File;
		size?: 'sm' | 'md' | 'lg';
	} = $props();

	let thumbnailUrl: string | null = $state(null);
	let loading = $state(false);
	let error = $state(false);

	const sizeClasses = {
		sm: 'w-10 h-10',
		md: 'w-16 h-16',
		lg: 'w-24 h-24'
	};

	let sizeClass = $derived(sizeClasses[size]);

	const isPDF = (mimeType: string) => {
		return mimeType === 'application/pdf';
	};

	const isVideo = (mimeType: string) => {
		return mimeType.startsWith('video/');
	};

	const isThumbnailSupported = (mimeType: string, fileName: string) => {
		// Images - always supported
		if (mimeType.startsWith('image/')) {
			return (
				[
					'image/jpeg',
					'image/png',
					'image/gif',
					'image/webp',
					'image/bmp',
					'image/svg+xml'
				].includes(mimeType.toLowerCase()) || mimeType.startsWith('image/')
			); // Support all images
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
	$effect(() => {
		if (file?.id) {
			loadThumbnail();
		}
	});

	onDestroy(() => {
		if (thumbnailUrl) {
			URL.revokeObjectURL(thumbnailUrl);
		}
	});

	// Special file types detection
	function isSpecialFile(fileName: string): { type: string; icon: string } | null {
		const lowerName = fileName.toLowerCase();
		if (lowerName.endsWith('.excalidraw') || lowerName.endsWith('.excalidraw.json')) {
			return { type: 'excalidraw', icon: 'pencil' };
		}
		if (lowerName.endsWith('.drawio') || lowerName.endsWith('.dio')) {
			return { type: 'drawio', icon: 'diagram' };
		}
		return null;
	}

	function getFallbackIcon(mimeType: string) {
		const normalized = mimeType.toLowerCase();
		if (normalized.startsWith('video/')) return FileVideo;
		if (normalized.startsWith('audio/')) return FileAudio;
		if (normalized === 'application/pdf') return FileText;
		if (normalized.includes('zip') || normalized.includes('tar') || normalized.includes('archive'))
			return Package;
		if (normalized.includes('word') || normalized.includes('wordprocessingml')) return FileType;
		if (normalized.includes('excel') || normalized.includes('spreadsheetml'))
			return FileSpreadsheet;
		if (normalized.includes('powerpoint') || normalized.includes('presentationml'))
			return Presentation;
		if (normalized.startsWith('text/')) return FileText;
		return FileDefault;
	}
</script>

<div class={`${sizeClass} flex items-center justify-center overflow-hidden rounded bg-base-200`}>
	{#if loading}
		<span class="loading loading-xs loading-spinner"></span>
	{:else if error || !thumbnailUrl}
		<!-- Show file type icon -->
		{#if isSpecialFile(file.name)}
			{@const special = isSpecialFile(file.name)}
			{#if special?.type === 'excalidraw'}
				<svg
					class="h-10 w-10 text-base-content/50"
					xmlns="http://www.w3.org/2000/svg"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="1.5"
					stroke-linecap="round"
					stroke-linejoin="round"
				>
					<path d="M12 19l7-7 3 3-7 7-3-3z" />
					<path d="M18 13l-1.5-7.5L2 2l3.5 14.5L13 18l5-5z" />
					<path d="M2 2l7.586 7.586" />
					<circle cx="11" cy="11" r="2" />
				</svg>
			{:else}
				<svg
					class="h-10 w-10 text-base-content/50"
					xmlns="http://www.w3.org/2000/svg"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="1.5"
					stroke-linecap="round"
					stroke-linejoin="round"
				>
					<rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
					<line x1="3" y1="9" x2="21" y2="9" />
					<line x1="9" y1="21" x2="9" y2="9" />
				</svg>
			{/if}
		{:else}
			{@const FallbackIcon = getFallbackIcon(file.mime_type)}
			<FallbackIcon class="h-8 w-8 text-base-content/50" />
		{/if}
	{:else}
		<!-- Show thumbnail image -->
		<img src={thumbnailUrl} alt={file.name} class="h-full w-full object-cover" />
	{/if}
</div>
