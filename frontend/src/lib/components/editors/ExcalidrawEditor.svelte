<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { File } from '$lib/api/types';
	import { editFile, getFileContent } from '$lib/api/files';
	import BaseEditor from './BaseEditor.svelte';
	import { createEventDispatcher } from 'svelte';

	export let open = false;
	export let file: File | null = null;

	type DispatchEvents = {
		close: void;
		saved: { file: File };
	}
	const dispatch = createEventDispatcher<DispatchEvents>();

	let content = '';
	let originalContent = '';
	let isLoading = false;
	let isSaving = false;
	let error: string | null = null;
	let saveMode: 'overwrite' | 'new_version' = 'new_version';
	let hasChanges = false;
	let excalidrawContainer: HTMLDivElement;
	let excalidrawInstance: any = null;

	// Load Excalidraw dynamically
	async function loadExcalidraw() {
		if (typeof window === 'undefined') return null;
		
		try {
			const excalidrawModule = await import('@excalidraw/excalidraw');
			return excalidrawModule;
		} catch (err) {
			console.error('Failed to load Excalidraw:', err);
			return null;
		}
	}

	async function initExcalidraw() {
		if (!excalidrawContainer || !file) return;

		const excalidraw = await loadExcalidraw();
		if (!excalidraw) {
			error = 'Failed to load Excalidraw. Please check your internet connection.';
			return;
		}

		// Parse the existing content if any
		let initialData = {};
		try {
			if (content) {
				initialData = JSON.parse(content);
			}
		} catch (e) {
			console.warn('Failed to parse Excalidraw data, starting with empty canvas');
		}

		// Create Excalidraw element
		const { Excalidraw } = excalidraw;
		
		// Since Excalidraw is a React component, we need to mount it differently
		// For now, we'll use a simple approach with the Excalidraw component
		const React = await import('react');
		const ReactDOM = await import('react-dom/client');

		// Use a setter so React onChange callbacks can update Svelte's hasChanges reactively
		const setHasChanges = (val: boolean) => { hasChanges = val; };

		const App = () => {
			return React.createElement(Excalidraw, {
				initialData,
				excalidrawAPI: (api: any) => {
					excalidrawInstance = api;
				},
				onChange: (_elements: readonly any[], _state: any) => {
					// Excalidraw only fires onChange on real user edits, never on initial mount
					setHasChanges(true);
				}
			});
		};

		const root = ReactDOM.createRoot(excalidrawContainer);
		root.render(React.createElement(App));
	}

	async function loadContent() {
		if (!file) return;

		isLoading = true;
		error = null;

		try {
			const loadedContent = await getFileContent(file.id);
			content = loadedContent;
			originalContent = loadedContent;
			hasChanges = false;

			// Initialize Excalidraw after content is loaded
			setTimeout(() => initExcalidraw(), 0);
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to load file content';
		} finally {
			isLoading = false;
		}
	}

	async function handleSave(event: CustomEvent<{ saveMode: 'overwrite' | 'new_version'; changeDescription?: string }>) {
		if (!file) return;

		// Get current content from Excalidraw
		if (!excalidrawInstance) {
			error = 'Excalidraw is not initialized';
			return;
		}

		isSaving = true;
		error = null;

		try {
			const elements = excalidrawInstance.getSceneElements();
			const appState = excalidrawInstance.getAppState();
			
			const exportData = {
				type: 'excalidraw',
				version: 2,
				source: window.location.origin,
				elements,
				appState: {
					viewBackgroundColor: appState.viewBackgroundColor,
					gridSize: appState.gridSize
				}
			};

			const newContent = JSON.stringify(exportData);

			const result = await editFile(
				file.id,
				newContent,
				event.detail.saveMode,
				event.detail.changeDescription
			);

			file = {
				...file,
				size: result.size,
				current_version: result.current_version,
				modified_at: result.modified_at
			};

			originalContent = newContent;
			hasChanges = false;

			dispatch('saved', { file });
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to save file';
		} finally {
			isSaving = false;
		}
	}

	function handleClose() {
		dispatch('close');
	}

	$: if (open && file) {
		loadContent();
	}

	$: if (!open) {
		// Cleanup
		if (excalidrawContainer) {
			excalidrawContainer.innerHTML = '';
		}
		excalidrawInstance = null;
	}

	onDestroy(() => {
		if (excalidrawContainer) {
			excalidrawContainer.innerHTML = '';
		}
	});
</script>

<BaseEditor
	{open}
	{file}
	{isLoading}
	{isSaving}
	{error}
	{saveMode}
	{hasChanges}
	title="Edit Excalidraw Diagram"
	on:close={handleClose}
	on:save={handleSave}
>
	<div bind:this={excalidrawContainer} class="w-full h-full bg-white"></div>
</BaseEditor>

<style>
	/* Excalidraw specific styles */
	:global(.excalidraw) {
		--ui-font: inherit;
	}
</style>
