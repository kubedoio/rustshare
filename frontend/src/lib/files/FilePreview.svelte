<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { File } from '$lib/api/types';
	import FileTypeIcon from './FileTypeIcon.svelte';
	import { Folder } from 'lucide-svelte';

	export let item: File | { id: string; name: string; mime_type?: string; updated_at?: string };
	export let isFolder: boolean = false;
	export let size: 'xs' | 'sm' | 'md' | 'lg' | 'xl' = 'md';
	export let showThumbnail: boolean = true;

	let thumbnailUrl: string | null = null;
	let loading = false;
	let error = false;

	const sizeClasses = {
		xs: 'w-6 h-6',
		sm: 'w-8 h-8',
		md: 'w-10 h-10',
		lg: 'w-16 h-16',
		xl: 'w-24 h-24'
	};

	const iconSizes = {
		xs: 12,
		sm: 16,
		md: 20,
		lg: 28,
		xl: 40
	};

	$: sizeClass = sizeClasses[size];
	$: iconSize = iconSizes[size];
	$: fileItem = isFolder ? null : (item as File);
	$: mimeType = fileItem?.mime_type || '';
	$: fileName = item?.name || '';

	const isPDF = (mime: string) => mime === 'application/pdf';
	const isVideo = (mime: string) => mime.startsWith('video/');
	const isImage = (mime: string) => mime.startsWith('image/');

	const isThumbnailSupported = (mime: string, name: string) => {
		if (isImage(mime)) return true;
		if (isPDF(mime)) return true;
		if (isVideo(mime)) return true;
		const lowerName = name.toLowerCase();
		if (lowerName.endsWith('.excalidraw') || lowerName.endsWith('.excalidraw.json')) return true;
		if (lowerName.endsWith('.drawio') || lowerName.endsWith('.dio')) return true;
		return false;
	};

	async function loadThumbnail() {
		if (isFolder || !showThumbnail || !item?.id || !isThumbnailSupported(mimeType, fileName)) {
			loading = false;
			return;
		}

		if (thumbnailUrl) {
			URL.revokeObjectURL(thumbnailUrl);
			thumbnailUrl = null;
		}

		loading = true;
		try {
			const thumbSize = size === 'xs' || size === 'sm' ? 'sm' : size === 'md' ? 'md' : 'lg';
			const response = await fetch(`/api/v1/files/${item.id}/thumbnail?size=${thumbSize}`, {
				credentials: 'include'
			});

			if (response.ok) {
				const blob = await response.blob();
				thumbnailUrl = URL.createObjectURL(blob);
				error = false;
			} else {
				error = true;
			}
		} catch (err) {
			console.error('Failed to load thumbnail:', err);
			error = true;
		} finally {
			loading = false;
		}
	}

	$: if (!isFolder && item?.id && showThumbnail) {
		loadThumbnail();
	}

	onDestroy(() => {
		if (thumbnailUrl) {
			URL.revokeObjectURL(thumbnailUrl);
		}
	});
</script>

<div class="{sizeClass} flex items-center justify-center rounded-lg overflow-hidden flex-shrink-0 {isFolder ? 'bg-brand-500/10' : 'bg-base-200'}">
	{#if isFolder}
		<Folder size={iconSize} class="text-brand-400" />
	{:else if loading}
		<div class="animate-pulse bg-base-300 w-full h-full"></div>
	{:else if thumbnailUrl && !error}
		<img
			src={thumbnailUrl}
			alt={fileName}
			class="w-full h-full object-cover"
		/>
	{:else}
		<FileTypeIcon {mimeType} {fileName} size={size === "xs" ? "sm" : size === "xl" ? "lg" : size} />
	{/if}
</div>
