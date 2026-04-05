<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { File, Folder } from '$lib/api/types';
	import FileTypeIcon from './FileTypeIcon.svelte';
	import { Folder as FolderIcon } from 'lucide-svelte';

	export let item: File | Folder;
	export let isFolder: boolean = false;
	export let size: 'xs' | 'sm' | 'md' | 'lg' | 'xl' = 'md';
	export let showThumbnail: boolean = true;
	export let isSharedRoot: boolean = false;

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
		{#if isSharedRoot || item.is_shared}
			<!-- Shared Folder Icon -->
			<svg xmlns="http://www.w3.org/2000/svg" width={iconSize} height={iconSize} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-brand-400">
				<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
				<circle cx="10" cy="13" r="2"></circle>
				<path d="M14 19v-1a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v1"></path>
				<circle cx="16" cy="13" r="2"></circle>
				<path d="M18 19v-1a2 2 0 0 0-1.18-1.82"></path>
			</svg>
		{:else}
			<FolderIcon size={iconSize} class="text-brand-400" />
		{/if}
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
