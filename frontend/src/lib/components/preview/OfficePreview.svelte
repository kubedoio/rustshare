<script lang="ts">
	import { formatFileSize } from '$lib/utils/format';
	import { getOfficeFileType } from '$lib/utils/format';
	import { FileText, FileSpreadsheet, Presentation } from 'lucide-svelte';
	import type { PreviewableFile } from '$lib/components/modals/FilePreviewModal.svelte';

	let {
		file = null
	}: {
		file?: PreviewableFile | null;
	} = $props();

	let officeType = $derived(file ? getOfficeFileType(file.mime_type, file.name) : null);

	const officeConfig = {
		word: {
			icon: FileText,
			label: 'Microsoft Word Document',
			color: 'text-blue-500',
			bgColor: 'bg-blue-50'
		},
		excel: {
			icon: FileSpreadsheet,
			label: 'Microsoft Excel Spreadsheet',
			color: 'text-green-500',
			bgColor: 'bg-green-50'
		},
		powerpoint: {
			icon: Presentation,
			label: 'Microsoft PowerPoint Presentation',
			color: 'text-orange-500',
			bgColor: 'bg-orange-50'
		}
	};

	let config = $derived(officeType ? officeConfig[officeType] : null);
</script>

<div class="flex flex-col items-center justify-center p-12 text-center">
	{#if file && config}
		<div
			class="h-24 w-24 rounded-2xl {config.bgColor} mb-6 flex items-center justify-center"
			aria-label="{config.label} icon"
		>
			<svelte:component this={config.icon} size={48} class={config.color} />
		</div>
		<h3 class="mb-2 text-xl font-semibold text-base-content">{file.name}</h3>
		<p class="mb-1 text-base-content/60">{config.label}</p>
		<p class="mb-6 text-sm text-base-content/40">
			{formatFileSize(file.size)}{#if file.modified_at} • {new Date(file.modified_at).toLocaleDateString()}{/if}
		</p>
		<div class="flex flex-col items-center gap-4">
			<p class="max-w-sm text-sm text-base-content/60">
				This file type cannot be previewed in the browser. Please download the file to view it.
			</p>
			<slot name="download-button" />
		</div>
	{:else if file}
		<div
			class="mb-4 flex h-20 w-20 items-center justify-center rounded-2xl bg-base-300"
			aria-label="Office document"
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				class="h-10 w-10 text-base-content/40"
				fill="none"
				viewBox="0 0 24 24"
				stroke="currentColor"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					stroke-width="2"
					d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
				/>
			</svg>
		</div>
		<h3 class="mb-2 text-lg font-semibold text-base-content">{file.name}</h3>
		<p class="text-sm text-base-content/60">Office Document</p>
	{:else}
		<div>No file selected</div>
	{/if}
</div>
