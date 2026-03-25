<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { previewFile } from '$lib/api/files';
	import type { File } from '$lib/api/types';
	import { formatFileSize } from '$lib/utils/format';

	export let open = false;
	export let file: File | null = null;

	const dispatch = createEventDispatcher<{
		close: void;
	}>();

	let previewUrl: string | null = null;
	let isLoading = false;
	let error: string | null = null;

	$: if (open && file) {
		loadPreview();
	}

	$: if (!open) {
		cleanup();
	}

	async function loadPreview() {
		if (!file) return;

		isLoading = true;
		error = null;
		previewUrl = null;

		try {
			const response = await previewFile(file.id);
			previewUrl = response.url;
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to load preview';
		} finally {
			isLoading = false;
		}
	}

	function cleanup() {
		previewUrl = null;
		error = null;
		isLoading = false;
	}

	function handleClose() {
		dispatch('close');
	}

	function isImage(mimeType: string): boolean {
		return mimeType.startsWith('image/');
	}

	function isPdf(mimeType: string): boolean {
		return mimeType === 'application/pdf';
	}

	function isVideo(mimeType: string): boolean {
		return mimeType.startsWith('video/');
	}

	function isAudio(mimeType: string): boolean {
		return mimeType.startsWith('audio/');
	}

	function isText(mimeType: string): boolean {
		return (
			mimeType.startsWith('text/') ||
			mimeType === 'application/json' ||
			mimeType === 'application/xml'
		);
	}

	function canPreview(mimeType: string): boolean {
		return isImage(mimeType) || isPdf(mimeType) || isVideo(mimeType) || isAudio(mimeType);
	}

	async function handleDownload() {
		if (!file || !previewUrl) return;
		window.open(previewUrl, '_blank');
	}
</script>

<dialog class="modal" class:modal-open={open}>
	<div class="modal-box max-w-6xl flex h-[90vh] flex-col">
		<div class="mb-4 flex items-center justify-between">
			<div class="min-w-0 flex-1">
				<h3 class="font-bold text-lg truncate">{file?.name || ''}</h3>
				{#if file}
					<p class="text-sm text-base-content/60">
						{formatFileSize(file.size)} • {file.mime_type}
					</p>
				{/if}
			</div>

			<button
				type="button"
				class="btn btn-ghost btn-sm btn-circle"
				aria-label="Close file preview"
				on:click={handleClose}
			>
				<svg
					xmlns="http://www.w3.org/2000/svg"
					fill="none"
					viewBox="0 0 24 24"
					stroke-width="1.5"
					stroke="currentColor"
					class="w-6 h-6"
				>
					<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
				</svg>
			</button>
		</div>

		<div class="bg-base-300 rounded-lg flex flex-1 items-center justify-center overflow-auto">
			{#if isLoading}
				<div class="gap-4 flex flex-col items-center">
					<span class="loading loading-spinner loading-lg"></span>
					<p class="text-sm text-base-content/60">Loading preview...</p>
				</div>
			{:else if error}
				<div class="p-8 text-center">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="w-16 h-16 text-error mb-4 mx-auto"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"
						/>
					</svg>
					<p class="text-error font-semibold mb-2">Preview failed</p>
					<p class="text-sm text-base-content/60">{error}</p>
				</div>
			{:else if file && previewUrl}
				{#if isImage(file.mime_type)}
					<img src={previewUrl} alt={file.name} class="max-h-full max-w-full object-contain" />
				{:else if isPdf(file.mime_type)}
					<iframe src={previewUrl} title={file.name} class="h-full w-full" frameborder="0"></iframe>
				{:else if isVideo(file.mime_type)}
					<video src={previewUrl} controls class="max-h-full max-w-full">
						<track kind="captions" />
						Your browser doesn't support video playback.
					</video>
				{:else if isAudio(file.mime_type)}
					<div class="p-8">
						<audio src={previewUrl} controls class="w-full">
							Your browser doesn't support audio playback.
						</audio>
					</div>
				{:else}
					<div class="p-8 text-center">
						<svg
							xmlns="http://www.w3.org/2000/svg"
							fill="none"
							viewBox="0 0 24 24"
							stroke-width="1.5"
							stroke="currentColor"
							class="w-16 h-16 text-base-content/40 mb-4 mx-auto"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
							/>
						</svg>
						<p class="text-base-content/60 mb-4">Preview not available for this file type</p>
						<button class="btn btn-primary" on:click={handleDownload}> Download File </button>
					</div>
				{/if}
			{/if}
		</div>

		<div class="mt-4 gap-2 flex justify-end">
			{#if file && previewUrl && canPreview(file.mime_type)}
				<button class="btn btn-outline" on:click={handleDownload}>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="w-5 h-5"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3"
						/>
					</svg>
					Download
				</button>
			{/if}
			<button class="btn" on:click={handleClose}>Close</button>
		</div>
	</div>

	<form method="dialog" class="modal-backdrop">
		<button type="button" on:click={handleClose}>close</button>
	</form>
</dialog>
