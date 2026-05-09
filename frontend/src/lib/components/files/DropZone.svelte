<script lang="ts">
	import { collectFilesFromDataTransfer } from '$lib/utils/directoryUpload';
	import UploadOverlay from '$lib/explorer/UploadOverlay.svelte';

	interface Props {
		disabled?: boolean;
		onFilesDropped?: (files: globalThis.File[]) => void;
		onDirectoryDropped?: (files: globalThis.File[]) => void;
		children?: import('svelte').Snippet;
	}

	let {
		disabled = false,
		onFilesDropped = () => {},
		onDirectoryDropped = () => {},
		children
	}: Props = $props();

	let isDragging = $state(false);
	let dragCounter = $state(0);

	function isFileDrag(event: DragEvent) {
		return event.dataTransfer?.types?.includes('Files') ?? false;
	}

	function containsDirectories(event: DragEvent): boolean {
		if (!event.dataTransfer?.items) return false;
		for (let i = 0; i < event.dataTransfer.items.length; i++) {
			const entry = (event.dataTransfer.items[i] as any).webkitGetAsEntry?.();
			if (entry?.isDirectory) return true;
		}
		return false;
	}

	function handleDragEnter(event: DragEvent) {
		dragCounter++;
		if (isFileDrag(event)) {
			isDragging = true;
		}
	}

	function handleDragLeave(event: DragEvent) {
		dragCounter--;
		if (dragCounter === 0) {
			isDragging = false;
		}
	}

	function handleDragOver(event: DragEvent) {
		if (!isFileDrag(event)) return;
		event.preventDefault();
		if (event.dataTransfer) {
			event.dataTransfer.dropEffect = disabled ? 'none' : 'copy';
		}
	}

	async function handleDrop(event: DragEvent) {
		isDragging = false;
		dragCounter = 0;

		if (!isFileDrag(event) || disabled) return;
		event.preventDefault();

		if (containsDirectories(event) && event.dataTransfer?.items) {
			const items = await collectFilesFromDataTransfer(event.dataTransfer.items);
			if (items.length > 0) {
				const files = items.map((i) => i.file);
				onDirectoryDropped(files);
			}
			return;
		}

		const files = event.dataTransfer?.files;
		if (files && files.length > 0) {
			onFilesDropped(Array.from(files));
		}
	}
</script>

<div
	class="relative"
	ondragenter={handleDragEnter}
	ondragleave={handleDragLeave}
	ondragover={handleDragOver}
	ondrop={handleDrop}
	role="region"
	aria-label="File drop zone"
>
	{@render children?.()}

	<UploadOverlay isDragging={isDragging && !disabled} />
</div>
