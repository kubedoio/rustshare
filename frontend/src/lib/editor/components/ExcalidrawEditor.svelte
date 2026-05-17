<script lang="ts">
	import { onDestroy, createEventDispatcher } from 'svelte';
	import React from 'react';
	import { createRoot } from 'react-dom/client';
	import { X, Save, Palette } from 'lucide-svelte';
	import { toastStore } from '$lib/stores/toast';

	let {
		open = false,
		initialData = null
	}: {
		open?: boolean;
		initialData?: {
			elements?: any[];
			appState?: any;
			files?: any;
		} | null;
	} = $props();

	const dispatch = createEventDispatcher<{
		save: { blob: Blob; filename: string };
		close: void;
	}>();

	let container: HTMLDivElement = $state() as unknown as HTMLDivElement;
	let root: any = $state(null);
	let excalidrawAPI: any = $state(null);
	let ExcalidrawComp: any = $state(null);
	let exportToBlobFn: any = $state(null);

	async function initExcalidraw() {
		try {
			// Dynamic imports for Excalidraw and React components
			const mod = await import('@excalidraw/excalidraw');
			ExcalidrawComp = mod.Excalidraw;
			exportToBlobFn = mod.exportToBlob;

			// Import styles
			await import('@excalidraw/excalidraw/index.css');

			if (container) {
				root = createRoot(container);
				render();
			}
		} catch (err) {
			console.error('Failed to load Excalidraw:', err);
		}
	}

	function render() {
		if (!root || !ExcalidrawComp) return;

		const props: any = {
			excalidrawAPI: (api: any) => (excalidrawAPI = api),
			UIOptions: {
				canvasActions: {
					toggleTheme: true,
					export: false,
					loadScene: false,
					saveToActiveFile: false
				}
			}
		};

		if (initialData) {
			props.initialData = initialData;
		}

		root.render(
			React.createElement(
				'div',
				{
					style: {
						height: '100%',
						width: '100%',
						display: 'flex',
						flexDirection: 'column'
					}
				},
				React.createElement(ExcalidrawComp, props)
			)
		);
	}

	async function handleSave() {
		if (!excalidrawAPI || !exportToBlobFn) return;

		const elements = excalidrawAPI.getSceneElements();
		const appState = excalidrawAPI.getAppState();
		const files = excalidrawAPI.getFiles();

		if (!elements || elements.length === 0) {
			toastStore.show('Please draw something first.', 'info');
			return;
		}

		try {
			const blob = await exportToBlobFn({
				elements,
				appState: {
					...appState,
					exportBackground: true,
					viewBackgroundColor: appState.viewBackgroundColor || '#ffffff',
					exportEmbedScene: true
				},
				files,
				mimeType: 'image/png',
				getDimensions: (width: number, height: number) => ({
					width: width * 2,
					height: height * 2
				})
			});

			const filename = `sketch-${Date.now()}.png`;
			dispatch('save', { blob, filename });
		} catch (err) {
			console.error('Failed to export drawing:', err);
			toastStore.show('Failed to save drawing. Please try again.', 'error');
		}
	}

	$effect(() => {
		if (open && !root && container) {
			initExcalidraw();
		}
	});

	// Re-render when initialData changes while open
	$effect(() => {
		if (open && root && ExcalidrawComp) {
			render();
		}
	});

	// Cleanup React root when modal closes so it reinitializes on next open.
	// The component itself is not destroyed (parent always renders it), so
	// onDestroy never runs when open toggles false.
	$effect(() => {
		if (!open && root) {
			root.unmount();
			root = null;
			excalidrawAPI = null;
		}
	});

	onDestroy(() => {
		if (root) {
			root.unmount();
			root = null;
		}
	});
</script>

{#if open}
	<div
		class="animate-fade-in fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-md"
	>
		<div
			class="rs-panel animate-slide-in-up flex h-[85vh] w-full max-w-6xl flex-col overflow-hidden shadow-2xl"
		>
			<!-- Toolbar -->
			<div
				class="flex items-center justify-between border-b border-base-300/50 bg-base-100/80 px-6 py-4 backdrop-blur-md"
			>
				<div class="flex items-center gap-3">
					<div
						class="bg-brand-soft text-brand flex h-10 w-10 items-center justify-center rounded-xl"
					>
						<Palette size={20} />
					</div>
					<div>
						<h2 class="text-lg font-bold tracking-tight">Excalidraw Sketch</h2>
						<p class="text-xs text-base-content/50">
							{initialData ? 'Edit your sketch' : 'Draw and insert into your note'}
						</p>
					</div>
				</div>

				<div class="flex gap-2">
					<button class="btn px-4 btn-ghost btn-sm" on:click={() => dispatch('close')}>
						<X size={16} />
						<span>Cancel</span>
					</button>
					<button class="btn px-6 btn-sm btn-primary" on:click={handleSave}>
						<Save size={16} />
						<span>{initialData ? 'Update Sketch' : 'Insert Sketch'}</span>
					</button>
				</div>
			</div>

			<!-- Canvas Container -->
			<div class="relative flex-1 bg-base-200/30">
				<div bind:this={container} class="absolute inset-0">
					{#if !ExcalidrawComp}
						<div class="flex h-full items-center justify-center">
							<div class="flex flex-col items-center gap-3">
								<div class="text-brand loading loading-lg loading-spinner"></div>
								<span class="text-sm font-medium text-base-content/40">Loading Excalidraw...</span>
							</div>
						</div>
					{/if}
				</div>
			</div>
		</div>
	</div>
{/if}

<style>
	:global(.excalidraw) {
		--rs-border-radius: 1rem;
	}

	:global(.excalidraw .App-menu_top) {
		margin-top: 1rem;
	}

	.border-b {
		border-bottom-width: 1px;
	}
</style>
