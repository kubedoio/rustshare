<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount, onDestroy } from 'svelte';
	import { createQuery, createMutation } from '$lib/query-compat';
	import {
		getBrainstormBoard,
		getBrainstormBoardSource,
		saveBrainstormBoardSource,
		updateBrainstormBoardPreview
	} from '$lib/api/brainstorming';
	import { ArrowLeft, Save, AlertCircle, CheckCircle2, Loader2 } from 'lucide-svelte';

	let boardId = $derived($page.params.boardId || '');

	// -------------------------------------------------------------------------
	// Queries
	// -------------------------------------------------------------------------

	const boardQuery = createQuery({
		queryKey: ['brainstorm-board', $page.params.boardId || ''],
		queryFn: () => getBrainstormBoard($page.params.boardId || ''),
		enabled: !!$page.params.boardId
	});

	const sourceQuery = createQuery({
		queryKey: ['brainstorm-board-source', $page.params.boardId || ''],
		queryFn: () => getBrainstormBoardSource($page.params.boardId || ''),
		enabled: !!$page.params.boardId
	});

	// -------------------------------------------------------------------------
	// State
	// -------------------------------------------------------------------------

	let excalidrawContainer: HTMLDivElement | null = $state(null);
	let excalidrawInstance: any = null;
	let hasChanges = $state(false);
	let isSaving = $state(false);
	let saveError = $state<string | null>(null);
	let saveStatus = $state<'saved' | 'saving' | 'unsaved' | 'error'>('saved');
	let autoSaveTimer: ReturnType<typeof setTimeout> | null = null;
	let isLoadingEditor = $state(true);
	let editorError = $state<string | null>(null);
	let editorInitialized = $state(false);

	// -------------------------------------------------------------------------
	// Mutations
	// -------------------------------------------------------------------------

	const saveSourceMutation = createMutation({
		mutationFn: (source: string) => saveBrainstormBoardSource(boardId, source),
		onMutate: () => {
			isSaving = true;
			saveStatus = 'saving';
			saveError = null;
		},
		onSuccess: async () => {
			hasChanges = false;
			saveStatus = 'saved';
			// Generate and upload preview
			try {
				await generateAndUploadPreview();
			} catch (previewErr) {
				console.warn('Preview generation failed:', previewErr);
				// Non-blocking warning
			}
		},
		onError: (err: Error) => {
			saveError = err.message || 'Failed to save board';
			saveStatus = 'error';
		},
		onSettled: () => {
			isSaving = false;
		}
	});

	// -------------------------------------------------------------------------
	// Excalidraw Initialization
	// -------------------------------------------------------------------------

	async function initExcalidraw(source: string) {
		if (!excalidrawContainer || typeof window === 'undefined' || editorInitialized) return;

		try {
			const excalidrawModule = await import('@excalidraw/excalidraw');
			const React = await import('react');
			const ReactDOM = await import('react-dom/client');

			const { Excalidraw } = excalidrawModule;

			let initialData: any = {};
			try {
				if (source) {
					initialData = JSON.parse(source);
				}
			} catch {
				console.warn('Failed to parse Excalidraw data, starting with empty canvas');
			}

			const App = () =>
				React.createElement(Excalidraw, {
					initialData,
					excalidrawAPI: (api: any) => {
						excalidrawInstance = api;
					},
					onChange: () => {
						hasChanges = true;
						saveStatus = 'unsaved';
						scheduleAutoSave();
					}
				});

			const root = ReactDOM.createRoot(excalidrawContainer);
			root.render(React.createElement(App));
			editorInitialized = true;
			isLoadingEditor = false;
		} catch (err) {
			editorError = 'Failed to load Excalidraw editor';
			isLoadingEditor = false;
			console.error(err);
		}
	}

	// -------------------------------------------------------------------------
	// Save Logic
	// -------------------------------------------------------------------------

	function scheduleAutoSave() {
		if (autoSaveTimer) {
			clearTimeout(autoSaveTimer);
		}
		autoSaveTimer = setTimeout(() => {
			if (hasChanges && !isSaving) {
				handleSave();
			}
		}, 1500);
	}

	async function handleSave() {
		if (!excalidrawInstance || isSaving) return;

		const elements = excalidrawInstance.getSceneElements();
		const appState = excalidrawInstance.getAppState();
		const files = excalidrawInstance.getFiles();

		const exportData = {
			type: 'excalidraw',
			version: 2,
			source: window.location.origin,
			elements,
			appState: {
				viewBackgroundColor: appState.viewBackgroundColor,
				gridSize: appState.gridSize
			},
			files
		};

		const source = JSON.stringify(exportData);
		saveSourceMutation.mutate(source);
	}

	async function generateAndUploadPreview() {
		if (!excalidrawInstance) return;

		const excalidrawModule = await import('@excalidraw/excalidraw');
		const { exportToBlob } = excalidrawModule;

		const blob = await exportToBlob({
			elements: excalidrawInstance.getSceneElements(),
			appState: excalidrawInstance.getAppState(),
			files: excalidrawInstance.getFiles(),
			mimeType: 'image/png'
		});

		await updateBrainstormBoardPreview(boardId, blob);
	}

	// -------------------------------------------------------------------------
	// Navigation
	// -------------------------------------------------------------------------

	function handleBack() {
		goto('/modules/brainstorming');
	}

	// -------------------------------------------------------------------------
	// Lifecycle
	// -------------------------------------------------------------------------

	$effect(() => {
		const source = $sourceQuery.data;
		if (source !== undefined && excalidrawContainer && !editorInitialized) {
			initExcalidraw(source);
		}
	});

	onDestroy(() => {
		if (autoSaveTimer) {
			clearTimeout(autoSaveTimer);
		}
		if (excalidrawContainer) {
			excalidrawContainer.innerHTML = '';
		}
		excalidrawInstance = null;
		editorInitialized = false;
	});
</script>

<svelte:head>
	<title>{$boardQuery.data?.title ?? 'Brainstorming Board'} — RustShare</title>
</svelte:head>

<div class="brainstorm-editor">
	<!-- Header -->
	<header class="editor-header">
		<div class="header-left">
			<button class="btn btn-ghost btn-sm btn-square" onclick={handleBack} aria-label="Back to gallery">
				<ArrowLeft size={20} />
			</button>
			<div class="title-block">
				<h1 class="board-title">
					{$boardQuery.data?.title ?? 'Untitled Board'}
				</h1>
				{#if $boardQuery.data}
					<span class="path-info">{$boardQuery.data.path}</span>
				{/if}
			</div>
		</div>

		<div class="header-right">
			{#if saveStatus === 'saving'}
				<span class="status-badge status-saving">
					<Loader2 size={14} class="animate-spin" />
					Saving...
				</span>
			{:else if saveStatus === 'unsaved'}
				<span class="status-badge status-unsaved">
					Unsaved changes
				</span>
			{:else if saveStatus === 'error'}
				<span class="status-badge status-error">
					<AlertCircle size={14} />
					Error
				</span>
			{:else}
				<span class="status-badge status-saved">
					<CheckCircle2 size={14} />
					Saved
				</span>
			{/if}

			<button
				class="btn btn-primary btn-sm gap-2"
				onclick={handleSave}
				disabled={isSaving || !hasChanges}
			>
				<Save size={14} />
				<span>{isSaving ? 'Saving...' : 'Save'}</span>
			</button>
		</div>
	</header>

	<!-- Editor -->
	<main class="editor-content">
		{#if $boardQuery.isLoading || $sourceQuery.isLoading}
			<div class="flex h-full items-center justify-center">
				<div class="loading loading-lg loading-spinner text-brand-500"></div>
			</div>
		{:else if $boardQuery.error}
			<div class="flex h-full flex-col items-center justify-center gap-4">
				<p class="text-error">Failed to load board.</p>
				<button class="btn btn-ghost" onclick={() => $boardQuery.refetch()}>Retry</button>
			</div>
		{:else if editorError}
			<div class="flex h-full flex-col items-center justify-center gap-4">
				<p class="text-error">{editorError}</p>
			</div>
		{:else}
			<div bind:this={excalidrawContainer} class="excalidraw-wrapper"></div>
		{/if}
	</main>

	{#if saveError}
		<div class="toast toast-bottom toast-center z-50">
			<div class="alert alert-error shadow-lg">
				<AlertCircle size={16} />
				<span>{saveError}</span>
			</div>
		</div>
	{/if}
</div>

<style>
	.brainstorm-editor {
		display: flex;
		flex-direction: column;
		height: calc(100vh - 64px); /* Adjust for app header if present */
		overflow: hidden;
	}

	.editor-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 1.25rem;
		border-bottom: 1px solid color-mix(in oklab, var(--base-300) 40%, transparent);
		background: var(--base-100);
	}

	.header-left {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex: 1;
		min-width: 0;
	}

	.title-block {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.board-title {
		font-size: 1.1rem;
		font-weight: 700;
		color: var(--base-content);
		margin: 0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.path-info {
		font-size: 0.75rem;
		color: var(--base-content);
		opacity: 0.5;
		font-family: monospace;
	}

	.header-right {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.status-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.8rem;
		padding: 0.25rem 0.6rem;
		border-radius: 999px;
		font-weight: 500;
	}

	.status-saved {
		color: var(--success);
		background: color-mix(in oklab, var(--success) 10%, transparent);
	}

	.status-saving {
		color: var(--info);
		background: color-mix(in oklab, var(--info) 10%, transparent);
	}

	.status-unsaved {
		color: var(--warning);
		background: color-mix(in oklab, var(--warning) 10%, transparent);
	}

	.status-error {
		color: var(--error);
		background: color-mix(in oklab, var(--error) 10%, transparent);
	}

	.editor-content {
		flex: 1;
		position: relative;
		overflow: hidden;
		background: #ffffff;
	}

	.excalidraw-wrapper {
		width: 100%;
		height: 100%;
	}

	.excalidraw-wrapper :global(.excalidraw) {
		--ui-font: inherit;
	}
</style>
