<script lang="ts">
	import { page } from '$app/stores';
	import { goto, beforeNavigate } from '$app/navigation';
	import { onMount, onDestroy } from 'svelte';
	import { queryClient } from '$lib/query-client';
	import { createQuery, createMutation } from '$lib/query-compat';
	import {
		getBrainstormBoard,
		getBrainstormBoardSource,
		saveBrainstormBoardSource,
		updateBrainstormBoardPreview
	} from '$lib/api/brainstorming';
	import ShareModal from '$lib/components/modals/ShareModal.svelte';
	import { toastStore } from '$lib/stores/toast';
	import { ArrowLeft, Save, AlertCircle, CheckCircle2, Loader2, ChevronRight, Share2 } from 'lucide-svelte';

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
	let reactRoot: any = null;
	let showShareModal = $state(false);

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
			// Invalidate cache so returning users get fresh data
			queryClient.invalidateQueries({ queryKey: ['brainstorm-board-source', boardId] });
			queryClient.invalidateQueries({ queryKey: ['brainstorm-board', boardId] });
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
			await import('@excalidraw/excalidraw/index.css');
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
					UIOptions: {
						welcomeScreen: false
					},
					excalidrawAPI: (api: any) => {
						excalidrawInstance = api;
					},
					onChange: () => {
						hasChanges = true;
						saveStatus = 'unsaved';
						scheduleAutoSave();
					}
				});

			reactRoot = ReactDOM.createRoot(excalidrawContainer);
			reactRoot.render(React.createElement(App));
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

		// Defer to avoid blocking the main thread / save response
		const blob = await new Promise<Blob>((resolve, reject) => {
			setTimeout(async () => {
				try {
					const result = await exportToBlob({
						elements: excalidrawInstance.getSceneElements(),
						appState: excalidrawInstance.getAppState(),
						files: excalidrawInstance.getFiles(),
						mimeType: 'image/png'
					});
					resolve(result);
				} catch (err) {
					reject(err);
				}
			}, 0);
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
		if (source === undefined || !excalidrawContainer) return;

		if (!editorInitialized) {
			initExcalidraw(source);
		} else if (excalidrawInstance && !hasChanges) {
			// Source data changed (e.g., returned after editing elsewhere)
			// Update scene without overwriting local unsaved changes
			try {
				const data = JSON.parse(source);
				excalidrawInstance.updateScene({
					elements: data.elements || [],
					appState: data.appState || {},
					files: data.files || {},
					commitToHistory: false
				});
			} catch {
				console.warn('Failed to update Excalidraw scene with new data');
			}
		}
	});

	function handleBeforeUnload(event: BeforeUnloadEvent) {
		if (hasChanges && !isSaving) {
			event.preventDefault();
			event.returnValue = '';
		}
	}

	onMount(() => {
		window.addEventListener('beforeunload', handleBeforeUnload);
		beforeNavigate((navigation) => {
			if (hasChanges && !isSaving) {
				if (!confirm('You have unsaved changes. Leave without saving?')) {
					navigation.cancel();
				}
			}
		});
	});

	onDestroy(() => {
		window.removeEventListener('beforeunload', handleBeforeUnload);
		if (autoSaveTimer) {
			clearTimeout(autoSaveTimer);
			autoSaveTimer = null;
		}
		// Flush pending save before tearing down
		if (hasChanges && !isSaving && excalidrawInstance) {
			handleSave();
		}
		if (reactRoot) {
			reactRoot.unmount();
			reactRoot = null;
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
			<button
				class="btn btn-square btn-ghost btn-sm"
				onclick={handleBack}
				aria-label="Back to gallery"
			>
				<ArrowLeft size={20} />
			</button>
			<div class="title-block">
				<nav aria-label="Breadcrumb" class="flex flex-wrap items-center gap-0.5">
					<button
						type="button"
						class="rounded-md px-1.5 py-0.5 text-sm font-medium text-base-content/70 transition-colors hover:bg-brand-500/10 hover:text-brand-600"
						onclick={() => goto('/modules/brainstorming')}
					>
						Brainstorming
					</button>
					<ChevronRight size={14} class="flex-shrink-0 text-base-content/30" />
					<span class="rounded-md px-1.5 py-0.5 text-sm font-semibold text-base-content" aria-current="page">
						{$boardQuery.data?.title ?? 'Untitled Board'}
					</span>
				</nav>
				<h1 class="board-title">
					{$boardQuery.data?.title ?? 'Untitled Board'}
				</h1>
			</div>
		</div>

		<div class="header-right">
			{#if saveStatus === 'saving'}
				<span class="status-badge status-saving">
					<Loader2 size={14} class="animate-spin" />
					Saving...
				</span>
			{:else if saveStatus === 'unsaved'}
				<span class="status-badge status-unsaved"> Unsaved changes </span>
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
				class="btn gap-2 btn-sm btn-outline"
				onclick={() => (showShareModal = true)}
			>
				<Share2 size={14} />
				<span>Share</span>
			</button>
			<button
				class="btn gap-2 btn-sm btn-primary"
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
			<div class="relative h-full w-full">
				{#if isLoadingEditor}
					<div class="absolute inset-0 z-10 flex items-center justify-center bg-base-100">
						<div class="loading loading-lg loading-spinner text-brand-500"></div>
					</div>
				{/if}
				<div bind:this={excalidrawContainer} class="excalidraw-wrapper h-full w-full"></div>
			</div>
		{/if}
	</main>

	{#if saveError}
		<div class="toast toast-center toast-bottom z-50">
			<div class="alert alert-error shadow-lg">
				<AlertCircle size={16} />
				<span>{saveError}</span>
			</div>
		</div>
	{/if}
</div>

<!-- Share Modal -->
<ShareModal
	open={showShareModal}
	resourceId={boardId}
	resourceName={$boardQuery.data?.title ?? 'Untitled Board'}
	resourceType="folder"
	onClose={() => (showShareModal = false)}
	onNotification={(payload) => toastStore.show(payload.message, payload.type)}
/>

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

	:global(.excalidraw .layer-ui__wrapper .dropdown-menu .social-links),
	:global(.excalidraw .layer-ui__wrapper .dropdown-menu a[href^="https://github.com"]),
	:global(.excalidraw .layer-ui__wrapper .dropdown-menu a[href^="https://twitter.com"]),
	:global(.excalidraw .layer-ui__wrapper .dropdown-menu a[href^="https://discord.gg"]) {
		display: none !important;
	}
</style>
