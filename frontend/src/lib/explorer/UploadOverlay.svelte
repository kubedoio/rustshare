<script lang="ts">
	interface Props {
		isDragging?: boolean;
		isUploading?: boolean;
		uploadProgress?: number;
		uploadCount?: number;
		uploadTotal?: number;
	}

	let {
		isDragging = false,
		isUploading = false,
		uploadProgress = 0,
		uploadCount = 0,
		uploadTotal = 0
	}: Props = $props();
</script>

{#if isDragging}
	<div
		class="absolute inset-0 z-40 flex items-center justify-center rounded-lg border-4 border-dashed border-primary bg-primary/10"
	>
		<div class="pointer-events-none text-center">
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="mx-auto h-16 w-16 text-primary"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"
				/>
			</svg>
			<p class="mt-4 text-lg font-semibold text-primary">Drop files to upload</p>
		</div>
	</div>
{/if}

{#if isUploading && !isDragging}
	<div
		class="absolute inset-x-0 bottom-0 z-40 rounded-b-lg border-t border-primary/20 bg-primary/5 px-4 py-2"
	>
		<div class="flex items-center gap-3">
			<div class="h-5 w-5 animate-spin rounded-full border-2 border-primary/30 border-t-primary"></div>
			<div class="flex-1">
				<div class="flex items-center justify-between">
					<span class="text-sm font-medium text-primary">Uploading {uploadCount} of {uploadTotal}</span>
					<span class="text-sm text-primary/70">{Math.round(uploadProgress)}%</span>
				</div>
				<progress
					class="progress mt-1 w-full progress-primary"
					value={uploadProgress}
					max="100"
				></progress>
			</div>
		</div>
	</div>
{/if}
