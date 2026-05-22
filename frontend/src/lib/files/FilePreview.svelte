<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import type { File, Folder } from '$lib/api/types';
	import FileTypeIcon from './FileTypeIcon.svelte';
	import { Folder as FolderIcon, Settings, FileText } from 'lucide-svelte';

	interface Props {
		item: File | Folder;
		isFolder?: boolean;
		size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl';
		showThumbnail?: boolean;
		isSharedRoot?: boolean;
	}

	let {
		item,
		isFolder = false,
		size = 'md',
		showThumbnail = true,
		isSharedRoot = false
	}: Props = $props();

	let thumbnailUrl = $state<string | null>(null);
	let loading = $state(false);
	let error = $state(false);
	let currentRequestKey: string | null = null;

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

	let sizeClass = $derived(sizeClasses[size]);
	let iconSize = $derived(iconSizes[size]);
	let fileItem = $derived(isFolder ? null : (item as File));
	let mimeType = $derived(fileItem?.mime_type || '');
	let fileName = $derived(item?.name || '');
	let isRustshareSystemFolder = $derived(isFolder && item.name === '_rustshare');
	let isNoteBundle = $derived(isFolder && (item as any).note_bundle_file_id != null);

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

	type ThumbnailRequest = {
		fileId: string;
		name: string;
		mimeType: string;
		size: 'xs' | 'sm' | 'md' | 'lg' | 'xl';
		modifiedAt: string;
		key: string;
	};

	function getThumbnailRequest(): ThumbnailRequest | null {
		if (
			isFolder ||
			!showThumbnail ||
			!fileItem?.id ||
			!isThumbnailSupported(fileItem.mime_type, fileItem.name)
		) {
			return null;
		}

		return {
			fileId: fileItem.id,
			name: fileItem.name,
			mimeType: fileItem.mime_type,
			size,
			modifiedAt: fileItem.modified_at,
			key: `${fileItem.id}:${fileItem.modified_at}:${size}`
		};
	}

	function clearThumbnail() {
		if (thumbnailUrl) {
			URL.revokeObjectURL(thumbnailUrl);
			thumbnailUrl = null;
		}
	}

	async function loadThumbnail(request: ThumbnailRequest) {
		clearThumbnail();
		loading = true;
		error = false;

		try {
			const thumbSize =
				request.size === 'xs' || request.size === 'sm' ? 'sm' : request.size === 'md' ? 'md' : 'lg';
			const response = await fetch(`/api/v1/files/${request.fileId}/thumbnail?size=${thumbSize}`, {
				credentials: 'include'
			});

			if (currentRequestKey !== request.key) return;

			if (response.ok) {
				const blob = await response.blob();
				if (currentRequestKey !== request.key) return;
				thumbnailUrl = URL.createObjectURL(blob);
				error = false;
			} else {
				error = true;
			}
		} catch (err) {
			if (currentRequestKey !== request.key) return;
			console.error('Failed to load thumbnail:', err);
			error = true;
		} finally {
			if (currentRequestKey === request.key) {
				loading = false;
			}
		}
	}

	$effect(() => {
		const request = getThumbnailRequest();

		if (!request) {
			untrack(() => {
				currentRequestKey = null;
				clearThumbnail();
				loading = false;
				error = false;
			});
			return;
		}

		if (request.key === currentRequestKey) return;

		untrack(() => {
			currentRequestKey = request.key;
			void loadThumbnail(request);
		});
	});

	onDestroy(() => {
		clearThumbnail();
	});
</script>

<div
	class="{sizeClass} flex flex-shrink-0 items-center justify-center overflow-hidden rounded-lg {isFolder
		? isRustshareSystemFolder
			? 'bg-base-300/30'
			: 'bg-brand-500/10'
		: 'bg-base-200'}"
>
	{#if isFolder}
		{#if isRustshareSystemFolder}
			<Settings size={iconSize} class="text-base-content/40" />
		{:else if isNoteBundle}
			<div class="relative">
				<FolderIcon size={iconSize} class="text-brand-400" />
				<div
					class="absolute -right-0.5 -bottom-0.5 flex h-4 w-4 items-center justify-center rounded-full bg-base-100 shadow-sm"
				>
					<FileText size={10} class="text-brand-500" />
				</div>
			</div>
		{:else if isSharedRoot || item.is_shared}
			<!-- Shared Folder Icon -->
			<svg
				xmlns="http://www.w3.org/2000/svg"
				width={iconSize}
				height={iconSize}
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="2"
				stroke-linecap="round"
				stroke-linejoin="round"
				class="text-brand-400"
			>
				<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
				></path>
				<circle cx="10" cy="13" r="2"></circle>
				<path d="M14 19v-1a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v1"></path>
				<circle cx="16" cy="13" r="2"></circle>
				<path d="M18 19v-1a2 2 0 0 0-1.18-1.82"></path>
			</svg>
		{:else}
			<FolderIcon size={iconSize} class="text-brand-400" />
		{/if}
	{:else if loading}
		<div class="h-full w-full animate-pulse bg-base-300"></div>
	{:else if thumbnailUrl && !error}
		<img src={thumbnailUrl} alt={fileName} class="h-full w-full object-cover" />
	{:else}
		<FileTypeIcon {mimeType} {fileName} size={size === 'xs' ? 'sm' : size === 'xl' ? 'lg' : size} />
	{/if}
</div>
