<script lang="ts">
	import { goto } from '$app/navigation';
	import { previewFile, downloadFile, getFileContent } from '$lib/api/files';
	import { formatFileSize } from '$lib/utils/format';
	import { detectEditorType, detectFileCapabilities } from '$lib/utils/editor';
	import OfficePreview from '$lib/components/preview/OfficePreview.svelte';
	import { renderMarkdown } from '$lib/utils/markdown';

	import type { File } from '$lib/api/types';

	export interface PreviewableFile {
		id: string;
		name: string;
		mime_type: string;
		size: number;
		modified_at?: string;
	}

	interface Props {
		open?: boolean;
		file?: PreviewableFile | null;
		onClose?: () => void;
		onEdit?: (payload: { file: File }) => void;
	}

	let { open = false, file = null, onClose = () => {}, onEdit = () => {} }: Props = $props();

	let previewUrl: string | null = $state(null);
	let textContent: string | null = $state(null);
	let isLoading = $state(false);
	let error: string | null = $state(null);

	$effect(() => {
		if (open && file) {
			loadPreview();
		}
	});

	$effect(() => {
		if (!open) {
			cleanup();
		}
	});

	// Determine if the file can be edited
	let capabilities = $derived(file ? detectFileCapabilities(file.name, file.mime_type) : null);
	let canEdit = $derived(capabilities?.canEdit ?? false);

	async function loadPreview() {
		if (!file) return;

		isLoading = true;
		error = null;
		previewUrl = null;
		textContent = null;

		try {
			// For text-based files, we fetch the content directly
			if (
				isText(file.mime_type) ||
				isMarkdown(file.name) ||
				isExcalidraw(file.name) ||
				isDrawio(file.name)
			) {
				textContent = await getFileContent(file.id);
				// Still get preview URL for download/misc
				try {
					const response = await previewFile(file.id);
					previewUrl = response.url;
				} catch (e) {
					// Ignore if preview API fails for text files
				}
			} else {
				const response = await previewFile(file.id);
				previewUrl = response.url;
			}
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to load preview';
		} finally {
			isLoading = false;
		}
	}

	function cleanup() {
		previewUrl = null;
		textContent = null;
		error = null;
		isLoading = false;
	}

	function handleClose() {
		onClose();
	}

	function handleEdit() {
		if (!file || !canEdit) return;

		// Route based on editor type
		if (capabilities?.editorType === 'image') {
			goto(`/files/edit/${file.id}`);
			onClose();
		} else {
			onEdit({ file: file as File });
		}
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

	function isMarkdown(fileName: string): boolean {
		return fileName.toLowerCase().endsWith('.md') || fileName.toLowerCase().endsWith('.mdx');
	}

	function isExcalidraw(fileName: string): boolean {
		return (
			fileName.toLowerCase().endsWith('.excalidraw') ||
			fileName.toLowerCase().endsWith('.excalidraw.json')
		);
	}

	function isDrawio(fileName: string): boolean {
		return fileName.toLowerCase().endsWith('.drawio') || fileName.toLowerCase().endsWith('.dio');
	}

	function canPreview(file: PreviewableFile): boolean {
		const caps = detectFileCapabilities(file.name, file.mime_type);
		return caps.previewType !== 'none';
	}

	async function handleDownload() {
		if (!file) return;
		const response = await downloadFile(file.id);
		let downloadUrl = response.url;
		// Handle storage URL rewrite if needed
		if (downloadUrl.includes('/rustshare-files/')) {
			const path = downloadUrl.split('/rustshare-files/')[1];
			downloadUrl = `/storage/${path}`;
		}
		window.open(downloadUrl, '_blank');
	}
</script>

<dialog class="modal" class:modal-open={open}>
	<div class="modal-box flex h-[90vh] max-w-6xl flex-col">
		<div class="mb-4 flex items-center justify-between">
			<div class="min-w-0 flex-1">
				<h3 class="truncate text-lg font-bold">{file?.name || ''}</h3>
				{#if file}
					<p class="text-sm text-base-content/60">
						{formatFileSize(file.size)} &bull; {file.mime_type}
					</p>
				{/if}
			</div>

			<button
				type="button"
				class="btn btn-circle btn-ghost btn-sm"
				aria-label="Close file preview"
				onclick={handleClose}
			>
				<svg
					xmlns="http://www.w3.org/2000/svg"
					fill="none"
					viewBox="0 0 24 24"
					stroke-width="1.5"
					stroke="currentColor"
					class="h-6 w-6"
				>
					<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
				</svg>
			</button>
		</div>

		<div class="flex flex-1 items-center justify-center overflow-auto rounded-lg bg-base-300">
			{#if isLoading}
				<div class="flex flex-col items-center gap-4">
					<span class="loading loading-lg loading-spinner"></span>
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
						class="mx-auto mb-4 h-16 w-16 text-error"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"
						/>
					</svg>
					<p class="mb-2 font-semibold text-error">Preview failed</p>
					<p class="text-sm text-base-content/60">{error}</p>
				</div>
			{:else if file}
				{#if capabilities?.previewType === 'image' && previewUrl}
					<img src={previewUrl} alt={file.name} class="max-h-full max-w-full object-contain" />
				{:else if capabilities?.previewType === 'pdf' && previewUrl}
					<iframe src={previewUrl} title={file.name} class="h-full w-full" frameborder="0"></iframe>
				{:else if capabilities?.previewType === 'video' && previewUrl}
					<video src={previewUrl} controls class="max-h-full max-w-full">
						<track kind="captions" />
						Your browser does not support video playback.
					</video>
				{:else if capabilities?.previewType === 'audio' && previewUrl}
					<div class="p-8">
						<audio src={previewUrl} controls class="w-full">
							Your browser does not support audio playback.
						</audio>
					</div>
				{:else if capabilities?.previewType === 'office' && file}
					<OfficePreview {file}>
						<button slot="download-button" class="btn btn-primary" onclick={handleDownload}>
							<svg
								xmlns="http://www.w3.org/2000/svg"
								class="mr-2 h-5 w-5"
								fill="none"
								viewBox="0 0 24 24"
								stroke="currentColor"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
								/>
							</svg>
							Download to View
						</button>
					</OfficePreview>
				{:else if capabilities?.previewType === 'code' && textContent !== null}
					<div class="h-full w-full overflow-auto bg-base-100">
						<pre class="p-6 font-mono text-sm"><code>{textContent}</code></pre>
					</div>
				{:else if file.name.toLowerCase().endsWith('.md') && textContent !== null}
					<div class="h-full w-full overflow-auto bg-base-100 p-8">
						<article class="prose max-w-none">
							{@html renderMarkdown(textContent)}
						</article>
					</div>
				{:else if (isExcalidraw(file.name) || isDrawio(file.name)) && textContent !== null}
					<div class="flex flex-col items-center gap-4 p-12 text-center">
						<div
							class="mb-2 flex h-20 w-20 items-center justify-center rounded-2xl bg-primary/10 text-primary"
						>
							<svg
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke-width="1.5"
								stroke="currentColor"
								class="h-10 w-10"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10"
								/>
							</svg>
						</div>
						<h4 class="text-xl font-bold">
							{isExcalidraw(file.name) ? 'Excalidraw' : 'Draw.io'} Diagram
						</h4>
						<p class="max-w-md text-base-content/60">
							This file is a diagram that can be viewed and edited in the specialized editor.
						</p>
						<button class="btn mt-4 btn-primary" onclick={handleEdit}> Open in Editor </button>
					</div>
				{:else}
					<div class="p-8 text-center">
						<svg
							xmlns="http://www.w3.org/2000/svg"
							fill="none"
							viewBox="0 0 24 24"
							stroke-width="1.5"
							stroke="currentColor"
							class="mx-auto mb-4 h-16 w-16 text-base-content/40"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
							/>
						</svg>
						<p class="mb-4 text-base-content/60">Preview not available for this file type</p>
						<button class="btn btn-primary" onclick={handleDownload}>Download File</button>
					</div>
				{/if}
			{/if}
		</div>

		<div class="mt-4 flex justify-end gap-2">
			{#if canEdit}
				<button class="btn btn-primary" onclick={handleEdit}>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="h-5 w-5"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10"
						/>
					</svg>
					Edit
				</button>
			{/if}
			{#if file && (previewUrl || textContent !== null) && canPreview(file)}
				<button class="btn btn-outline" onclick={handleDownload}>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="h-5 w-5"
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
			<button class="btn" onclick={handleClose}>Close</button>
		</div>
	</div>

	<form method="dialog" class="modal-backdrop">
		<button type="button" onclick={handleClose}>close</button>
	</form>
</dialog>
