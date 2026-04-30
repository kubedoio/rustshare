<script lang="ts">
	import { onMount, createEventDispatcher, tick } from 'svelte';
	import { ImageEditor } from '$lib/utils/imageEditor';
	import type { CropSelection } from '$lib/utils/imageEditor';
	import {
		RotateCw,
		RotateCcw,
		FlipHorizontal,
		FlipVertical,
		Scissors,
		Undo,
		Redo,
		X,
		Check
	} from 'lucide-svelte';

	export let imageUrl: string;
	export let fileName: string;

	const dispatch = createEventDispatcher<{
		save: { blob: Blob; fileName: string };
		cancel: void;
	}>();

	let canvas: HTMLCanvasElement;
	let editor: ImageEditor;
	let loading = true;
	let error: string | null = null;

	// Toolbar state
	let canUndo = false;
	let canRedo = false;

	// Crop mode
	let isCropping = false;
	let cropStart: { x: number; y: number } | null = null;
	let cropSelection: CropSelection | null = null;
	let isDragging = false;

	// Resize modal
	let showResizeModal = false;
	let resizeWidth = 0;
	let resizeHeight = 0;
	let maintainAspectRatio = true;
	let aspectRatio = 1;

	onMount(() => {
		void (async () => {
			// Wait for canvas to be available
			await tick();

			if (!canvas) {
				error = 'Canvas element not found';
				loading = false;
				return;
			}

			try {
				editor = new ImageEditor(canvas);
				await editor.loadImage(imageUrl);
				updateToolbarState();
				const dims = editor.getDimensions();
				resizeWidth = dims.width;
				resizeHeight = dims.height;
				aspectRatio = dims.width / dims.height;
				loading = false;
			} catch (err) {
				error = err instanceof Error ? err.message : 'Failed to load image';
				loading = false;
			}
		})();
	});

	function updateToolbarState() {
		canUndo = editor?.canUndo() ?? false;
		canRedo = editor?.canRedo() ?? false;
	}

	function handleRotateCw() {
		editor.rotateClockwise();
		updateToolbarState();
	}

	function handleRotateCcw() {
		editor.rotateCounterClockwise();
		updateToolbarState();
	}

	function handleFlipH() {
		editor.flipHorizontal();
		updateToolbarState();
	}

	function handleFlipV() {
		editor.flipVertical();
		updateToolbarState();
	}

	function handleUndo() {
		editor.undo();
		updateToolbarState();
	}

	function handleRedo() {
		editor.redo();
		updateToolbarState();
	}

	// Crop handlers
	function startCropMode() {
		isCropping = true;
		cropSelection = null;
	}

	function cancelCrop() {
		isCropping = false;
		cropSelection = null;
		cropStart = null;
	}

	function applyCrop() {
		if (cropSelection && editor) {
			editor.crop(cropSelection);
			updateToolbarState();
		}
		isCropping = false;
		cropSelection = null;
		cropStart = null;
	}

	function handleCanvasMouseDown(e: MouseEvent) {
		if (!isCropping) return;

		const rect = canvas.getBoundingClientRect();
		const scaleX = canvas.width / rect.width;
		const scaleY = canvas.height / rect.height;

		cropStart = {
			x: (e.clientX - rect.left) * scaleX,
			y: (e.clientY - rect.top) * scaleY
		};
		isDragging = true;
	}

	function handleCanvasMouseMove(e: MouseEvent) {
		if (!isCropping || !isDragging || !cropStart) return;

		const rect = canvas.getBoundingClientRect();
		const scaleX = canvas.width / rect.width;
		const scaleY = canvas.height / rect.height;

		const currentX = (e.clientX - rect.left) * scaleX;
		const currentY = (e.clientY - rect.top) * scaleY;

		const x = Math.min(cropStart.x, currentX);
		const y = Math.min(cropStart.y, currentY);
		const width = Math.abs(currentX - cropStart.x);
		const height = Math.abs(currentY - cropStart.y);

		cropSelection = { x, y, width, height };
	}

	function handleCanvasMouseUp() {
		isDragging = false;
	}

	// Resize handlers
	function openResizeModal() {
		const dims = editor.getDimensions();
		resizeWidth = dims.width;
		resizeHeight = dims.height;
		showResizeModal = true;
	}

	function handleWidthChange(e: Event) {
		const input = e.target as HTMLInputElement;
		resizeWidth = parseInt(input.value) || 0;
		if (maintainAspectRatio) {
			resizeHeight = Math.round(resizeWidth / aspectRatio);
		}
	}

	function handleHeightChange(e: Event) {
		const input = e.target as HTMLInputElement;
		resizeHeight = parseInt(input.value) || 0;
		if (maintainAspectRatio) {
			resizeWidth = Math.round(resizeHeight * aspectRatio);
		}
	}

	function applyResize() {
		if (editor && resizeWidth > 0 && resizeHeight > 0) {
			editor.resize(resizeWidth, resizeHeight);
			updateToolbarState();
		}
		showResizeModal = false;
	}

	// Save handlers
	async function handleSave() {
		if (!editor) return;

		try {
			const ext = fileName.split('.').pop()?.toLowerCase() || 'png';
			const mimeType =
				ext === 'jpg' || ext === 'jpeg'
					? 'image/jpeg'
					: ext === 'webp'
						? 'image/webp'
						: 'image/png';

			const blob = await editor.toBlob(mimeType, 0.92);
			dispatch('save', { blob, fileName });
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to save image';
		}
	}

	function handleCancel() {
		dispatch('cancel');
	}
</script>

<div class="flex h-full flex-col bg-base-100">
	<!-- Toolbar -->
	<div class="flex flex-wrap items-center gap-2 border-b border-base-300 p-3">
		<div class="flex items-center gap-1">
			<button class="btn btn-ghost btn-sm" onclick={handleRotateCw} title="Rotate 90° clockwise">
				<RotateCw size={18} />
			</button>
			<button
				class="btn btn-ghost btn-sm"
				onclick={handleRotateCcw}
				title="Rotate 90° counter-clockwise"
			>
				<RotateCcw size={18} />
			</button>
		</div>

		<div class="divider divider-horizontal"></div>

		<div class="flex items-center gap-1">
			<button class="btn btn-ghost btn-sm" onclick={handleFlipH} title="Flip horizontal">
				<FlipHorizontal size={18} />
			</button>
			<button class="btn btn-ghost btn-sm" onclick={handleFlipV} title="Flip vertical">
				<FlipVertical size={18} />
			</button>
		</div>

		<div class="divider divider-horizontal"></div>

		<div class="flex items-center gap-1">
			<button class="btn btn-ghost btn-sm" onclick={openResizeModal} title="Resize">
				<svg
					xmlns="http://www.w3.org/2000/svg"
					width="18"
					height="18"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-linecap="round"
					stroke-linejoin="round"
					><path d="m21 21-6-6m6 6v-4.8m0 4.8h-4.8" /><path d="M3 16.2V21m0 0h4.8M3 21l6-6" /><path
						d="M21 7.8V3m0 0h-4.8M21 3l-6 6"
					/><path d="M3 7.8V3m0 0h4.8M3 3l6 6" /></svg
				>
			</button>
			<button
				class="btn btn-ghost btn-sm"
				class:btn-active={isCropping}
				onclick={startCropMode}
				title="Crop"
			>
				<Scissors size={18} />
			</button>
		</div>

		<div class="divider divider-horizontal"></div>

		<div class="flex items-center gap-1">
			<button class="btn btn-ghost btn-sm" onclick={handleUndo} disabled={!canUndo} title="Undo">
				<Undo size={18} />
			</button>
			<button class="btn btn-ghost btn-sm" onclick={handleRedo} disabled={!canRedo} title="Redo">
				<Redo size={18} />
			</button>
		</div>

		{#if isCropping}
			<div class="ml-auto flex items-center gap-1">
				<button class="btn btn-ghost btn-sm btn-error" onclick={cancelCrop}>
					<X size={18} />
					Cancel
				</button>
				<button
					class="btn btn-ghost btn-sm btn-success"
					onclick={applyCrop}
					disabled={!cropSelection}
				>
					<Check size={18} />
					Apply
				</button>
			</div>
		{/if}
	</div>

	<!-- Canvas Area -->
	<div class="relative flex flex-1 items-center justify-center overflow-auto bg-base-300 p-4">
		{#if loading}
			<div class="flex flex-col items-center gap-4">
				<span class="loading loading-lg loading-spinner"></span>
				<p class="text-base-content/60">Loading image...</p>
			</div>
		{:else if error}
			<div class="text-error">
				<p>{error}</p>
			</div>
		{/if}

		<!-- Canvas always rendered but hidden when loading/error -->
		<div class="relative" class:hidden={loading || error}>
			<canvas
				bind:this={canvas}
				class="max-h-full max-w-full shadow-lg"
				class:cursor-crosshair={isCropping}
				onmousedown={handleCanvasMouseDown}
				onmousemove={handleCanvasMouseMove}
				onmouseup={handleCanvasMouseUp}
				onmouseleave={handleCanvasMouseUp}
			></canvas>

			{#if isCropping && cropSelection}
				<div
					class="pointer-events-none absolute border-2 border-primary bg-primary/20"
					style="left: {(cropSelection.x / canvas.width) * 100}%; top: {(cropSelection.y /
						canvas.height) *
						100}%; width: {(cropSelection.width / canvas.width) *
						100}%; height: {(cropSelection.height / canvas.height) * 100}%"
				></div>
			{/if}
		</div>
	</div>

	<!-- Footer -->
	<div class="flex items-center justify-end gap-2 border-t border-base-300 p-4">
		<button class="btn btn-ghost" onclick={handleCancel}>Cancel</button>
		<button class="btn btn-primary" onclick={handleSave}> Save as New... </button>
	</div>
</div>

<!-- Resize Modal -->
{#if showResizeModal}
	<div class="modal-open modal">
		<div class="modal-box">
			<h3 class="mb-4 text-lg font-bold">Resize Image</h3>

			<div class="mb-4 flex items-center gap-4">
				<div class="form-control flex-1">
					<label class="label" for="resize-width">
						<span class="label-text">Width (px)</span>
					</label>
					<input
						id="resize-width"
						type="number"
						class="input-bordered input"
						bind:value={resizeWidth}
						oninput={handleWidthChange}
						min="1"
						max="10000"
					/>
				</div>

				<div class="pt-8">
					<button
						class="btn btn-ghost btn-sm"
						class:btn-active={maintainAspectRatio}
						onclick={() => (maintainAspectRatio = !maintainAspectRatio)}
						title="Lock aspect ratio"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							width="16"
							height="16"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
							><rect x="3" y="11" width="18" height="11" rx="2" ry="2" /><path
								d="M7 11V7a5 5 0 0 1 10 0v4"
							/></svg
						>
					</button>
				</div>

				<div class="form-control flex-1">
					<label class="label" for="resize-height">
						<span class="label-text">Height (px)</span>
					</label>
					<input
						id="resize-height"
						type="number"
						class="input-bordered input"
						bind:value={resizeHeight}
						oninput={handleHeightChange}
						min="1"
						max="10000"
					/>
				</div>
			</div>

			<label class="label mb-4 cursor-pointer justify-start gap-2">
				<input type="checkbox" class="checkbox" bind:checked={maintainAspectRatio} />
				<span class="label-text">Maintain aspect ratio</span>
			</label>

			<div class="modal-action">
				<button class="btn btn-ghost" onclick={() => (showResizeModal = false)}>Cancel</button>
				<button class="btn btn-primary" onclick={applyResize}>Apply</button>
			</div>
		</div>
		<div
			class="modal-backdrop"
			role="presentation"
			tabindex="-1"
			onclick={() => (showResizeModal = false)}
			onkeydown={(e) => {
				if (e.key === 'Escape') showResizeModal = false;
			}}
		></div>
	</div>
{/if}
