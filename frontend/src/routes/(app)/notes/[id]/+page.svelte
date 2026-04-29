<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onDestroy, onMount } from 'svelte';
	import { renderMarkdown } from '$lib/utils/markdown';
	import { formatDate } from '$lib/utils/format';
	import {
		createNote,
		getNote,
		saveNote,
		renameNote,
		deleteNote,
		toggleVisibility
	} from '$lib/api/notes';
	import type { Note } from '$lib/api/types';
	import {
		ArrowLeft,
		Eye,
		EyeOff,
		Trash2,
		Save,
		Check,
		AlertCircle,
		Globe,
		Lock,
		Bold,
		Italic,
		Heading1,
		Heading2,
		List,
		ListOrdered,
		Quote,
		Code,
		Link
	} from 'lucide-svelte';

	const noteId = $page.params.id as string;

	let note: Note | null = null;
	let title = '';
	let content = '';
	let originalContent = '';
	let isLoading = true;
	let isSaving = false;
	let saveError: string | null = null;
	let showPreview = true;
	let saveState: 'idle' | 'saving' | 'saved' | 'error' = 'idle';
	let autoSaveTimer: ReturnType<typeof setTimeout> | null = null;
	let renderedPreview = '';
	let showDeleteConfirm = false;
	let copiedPublicUrl = false;

	$: if (content !== undefined) {
		renderedPreview = renderMarkdown(content);
	}

	onMount(() => {
		void (async () => {
			if (!noteId) {
				saveError = 'Invalid note ID';
				isLoading = false;
				return;
			}
			await loadNote();
		})();
	});

	onDestroy(() => {
		if (autoSaveTimer) clearTimeout(autoSaveTimer);
	});

	async function loadNote() {
		isLoading = true;
		try {
			note = await getNote(noteId);
			title = note.metadata.title;
			content = note.content;
			originalContent = note.content;
		} catch (err) {
			saveError = err instanceof Error ? err.message : 'Failed to load note';
		} finally {
			isLoading = false;
		}
	}

	function scheduleAutoSave() {
		if (autoSaveTimer) clearTimeout(autoSaveTimer);
		saveState = 'idle';
		autoSaveTimer = setTimeout(() => {
			if (content !== originalContent && !isSaving) {
				handleSave();
			}
		}, 1500);
	}

	async function handleSave() {
		if (!note || content === originalContent) return;
		isSaving = true;
		saveState = 'saving';
		saveError = null;
		try {
			await saveNote(note.id, { content });
			originalContent = content;
			saveState = 'saved';
			// Update note metadata timestamps locally
			note = { ...note, modified_at: new Date().toISOString() };
		} catch (err) {
			saveState = 'error';
			saveError = err instanceof Error ? err.message : 'Failed to save';
		} finally {
			isSaving = false;
		}
	}

	async function handleTitleBlur() {
		if (!note || title === note.metadata.title) return;
		try {
			note = await renameNote(note.id, { title });
		} catch (err) {
			saveError = err instanceof Error ? err.message : 'Failed to rename';
		}
	}

	async function handleToggleVisibility() {
		if (!note) return;
		try {
			const result = await toggleVisibility(note.id);
			note = {
				...note,
				metadata: {
					...note.metadata,
					visibility: result.visibility,
					public_share_id: result.public_share_id
				}
			};
		} catch (err) {
			saveError = err instanceof Error ? err.message : 'Failed to toggle visibility';
		}
	}

	async function handleDelete() {
		if (!note) return;
		try {
			await deleteNote(note.id);
			goto('/dashboard');
		} catch (err) {
			saveError = err instanceof Error ? err.message : 'Failed to delete';
			showDeleteConfirm = false;
		}
	}

	function copyPublicUrl() {
		if (!note?.metadata.public_share_id) return;
		const url = `${window.location.origin}/p/note/${note.metadata.public_share_id}`;
		navigator.clipboard.writeText(url);
		copiedPublicUrl = true;
		setTimeout(() => (copiedPublicUrl = false), 2000);
	}

	function insertText(before: string, after: string = '') {
		const textarea = document.querySelector('textarea') as HTMLTextAreaElement;
		if (!textarea) return;
		const start = textarea.selectionStart;
		const end = textarea.selectionEnd;
		const selected = content.slice(start, end);
		const replacement = before + selected + after;
		content = content.slice(0, start) + replacement + content.slice(end);
		setTimeout(() => {
			textarea.focus();
			const newCursor = start + before.length + selected.length;
			textarea.setSelectionRange(newCursor, newCursor);
		}, 0);
	}

	function goBack() {
		if (window.history.length > 1) {
			window.history.back();
		} else {
			goto('/dashboard');
		}
	}
</script>

<svelte:head>
	<title>{title || 'Note'} - RustShare</title>
</svelte:head>

<div class="note-editor-page">
	<!-- Header -->
	<header class="note-header">
		<div class="note-header-left">
			<button class="btn btn-ghost btn-sm" on:click={goBack}>
				<ArrowLeft size={16} />
				<span>Back</span>
			</button>
		</div>

		<div class="note-header-center">
			{#if isLoading}
				<div class="h-8 w-48 skeleton rounded"></div>
			{:else}
				<input
					type="text"
					class="note-title-input"
					bind:value={title}
					on:blur={handleTitleBlur}
					placeholder="Untitled Note"
				/>
			{/if}
		</div>

		<div class="note-header-right">
			<div class="save-status">
				{#if saveState === 'saving'}
					<Save size={14} class="animate-pulse" />
					<span>Saving...</span>
				{:else if saveState === 'error'}
					<AlertCircle size={14} class="text-error" />
					<span class="text-error">Error</span>
				{/if}
			</div>

			{#if !isLoading && note}
				<button
					class="btn btn-sm {note.metadata.visibility === 'public' ? 'btn-primary' : 'btn-ghost'}"
					on:click={handleToggleVisibility}
					title={note.metadata.visibility === 'public' ? 'Public note' : 'Private note'}
				>
					{#if note.metadata.visibility === 'public'}
						<Globe size={14} />
						<span>Public</span>
					{:else}
						<Lock size={14} />
						<span>Private</span>
					{/if}
				</button>

				<button class="btn text-error btn-ghost btn-sm" on:click={() => (showDeleteConfirm = true)}>
					<Trash2 size={14} />
				</button>
			{/if}
		</div>
	</header>

	<!-- Public URL banner -->
	{#if note?.metadata.visibility === 'public' && note.metadata.public_share_id}
		<div class="public-url-banner">
			<div class="public-url-content">
				<Globe size={14} class="text-brand-500" />
				<span>Anyone with the link can view this note</span>
				<button class="btn btn-ghost btn-xs" on:click={copyPublicUrl}>
					{copiedPublicUrl ? 'Copied!' : 'Copy link'}
				</button>
			</div>
		</div>
	{/if}

	<!-- Error banner -->
	{#if saveError}
		<div class="error-banner">
			<AlertCircle size={16} />
			<span>{saveError}</span>
			<button class="btn btn-ghost btn-xs" on:click={() => (saveError = null)}>Dismiss</button>
		</div>
	{/if}

	<!-- Toolbar -->
	<div class="note-toolbar">
		<div class="toolbar-group">
			<button class="toolbar-btn" on:click={() => insertText('**', '**')} title="Bold">
				<Bold size={14} />
			</button>
			<button class="toolbar-btn" on:click={() => insertText('*', '*')} title="Italic">
				<Italic size={14} />
			</button>
		</div>
		<div class="toolbar-divider"></div>
		<div class="toolbar-group">
			<button class="toolbar-btn" on:click={() => insertText('# ')} title="Heading 1">
				<Heading1 size={14} />
			</button>
			<button class="toolbar-btn" on:click={() => insertText('## ')} title="Heading 2">
				<Heading2 size={14} />
			</button>
		</div>
		<div class="toolbar-divider"></div>
		<div class="toolbar-group">
			<button class="toolbar-btn" on:click={() => insertText('- ')} title="Bullet list">
				<List size={14} />
			</button>
			<button class="toolbar-btn" on:click={() => insertText('1. ')} title="Numbered list">
				<ListOrdered size={14} />
			</button>
			<button class="toolbar-btn" on:click={() => insertText('> ')} title="Quote">
				<Quote size={14} />
			</button>
			<button class="toolbar-btn" on:click={() => insertText('`', '`')} title="Code">
				<Code size={14} />
			</button>
			<button class="toolbar-btn" on:click={() => insertText('[', '](url)')} title="Link">
				<Link size={14} />
			</button>
		</div>
		<div class="flex-1"></div>
		<button class="toolbar-btn preview-toggle" on:click={() => (showPreview = !showPreview)}>
			{#if showPreview}
				<EyeOff size={14} />
				<span>Hide preview</span>
			{:else}
				<Eye size={14} />
				<span>Show preview</span>
			{/if}
		</button>
	</div>

	<!-- Editor + Preview -->
	<div class="note-editor-body">
		{#if isLoading}
			<div class="loading-state">
				<span class="loading loading-lg loading-spinner"></span>
				<p>Loading note...</p>
			</div>
		{:else}
			<div class="editor-pane" class:fullscreen={!showPreview}>
				<textarea
					class="note-textarea"
					bind:value={content}
					on:input={scheduleAutoSave}
					placeholder="Start typing..."
					spellcheck="false"
				></textarea>
			</div>

			{#if showPreview}
				<div class="preview-pane">
					<div class="preview-content prose-sm prose max-w-none">
						{#if renderedPreview}
							{@html renderedPreview}
						{:else}
							<p class="text-base-content/40 italic">Nothing to preview</p>
						{/if}
					</div>
				</div>
			{/if}
		{/if}
	</div>

	<!-- Footer meta -->
	{#if note}
		<footer class="note-footer">
			<span>Created {formatDate(note.created_at)}</span>
			<span class="dot">•</span>
			<span>Updated {formatDate(note.modified_at)}</span>
		</footer>
	{/if}
</div>

<!-- Delete confirmation modal -->
{#if showDeleteConfirm}
	<div class="modal-open modal">
		<div class="modal-box">
			<h3 class="text-lg font-bold">Delete note?</h3>
			<p class="py-4">
				This will move "{title || 'Untitled Note'}" to trash. You can restore it later.
			</p>
			<div class="modal-action">
				<button class="btn btn-ghost" on:click={() => (showDeleteConfirm = false)}>Cancel</button>
				<button class="btn btn-error" on:click={handleDelete}>Delete</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.note-editor-page {
		display: flex;
		flex-direction: column;
		height: 100vh;
		background: var(--base-100);
	}

	.note-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--base-300);
		min-height: 3.5rem;
	}

	.note-header-left,
	.note-header-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
	}

	.note-header-center {
		flex: 1;
		display: flex;
		justify-content: center;
		min-width: 0;
	}

	.note-title-input {
		font-size: 1.125rem;
		font-weight: 600;
		text-align: center;
		background: transparent;
		border: 1px solid transparent;
		border-radius: 0.375rem;
		padding: 0.25rem 0.75rem;
		max-width: 32rem;
		width: 100%;
		color: var(--base-content);
	}

	.note-title-input:hover,
	.note-title-input:focus {
		border-color: var(--base-300);
		background: var(--base-200);
		outline: none;
	}

	.save-status {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.75rem;
		color: var(--base-content-muted);
		padding: 0 0.5rem;
	}

	.public-url-banner {
		padding: 0.5rem 1rem;
		background: color-mix(in oklab, var(--brand-500) 8%, transparent);
		border-bottom: 1px solid color-mix(in oklab, var(--brand-500) 15%, transparent);
	}

	.public-url-content {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		font-size: 0.8125rem;
		color: var(--base-content);
	}

	.error-banner {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		padding: 0.5rem 1rem;
		background: color-mix(in oklab, var(--error) 10%, transparent);
		border-bottom: 1px solid color-mix(in oklab, var(--error) 20%, transparent);
		font-size: 0.8125rem;
		color: var(--error);
	}

	.note-toolbar {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.5rem 1rem;
		border-bottom: 1px solid var(--base-300);
		background: var(--base-200);
	}

	.toolbar-group {
		display: flex;
		align-items: center;
		gap: 0.125rem;
	}

	.toolbar-divider {
		width: 1px;
		height: 1.25rem;
		background: var(--base-300);
		margin: 0 0.25rem;
	}

	.toolbar-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.5rem;
		border-radius: 0.375rem;
		font-size: 0.75rem;
		color: var(--base-content);
		background: transparent;
		border: none;
		cursor: pointer;
		transition: background 0.15s;
	}

	.toolbar-btn:hover {
		background: var(--base-300);
	}

	.note-editor-body {
		flex: 1;
		display: flex;
		overflow: hidden;
		min-height: 0;
	}

	.editor-pane {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.editor-pane.fullscreen {
		flex: 1;
	}

	.note-textarea {
		flex: 1;
		width: 100%;
		resize: none;
		border: none;
		padding: 1.5rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
		font-size: 0.9375rem;
		line-height: 1.7;
		background: var(--base-100);
		color: var(--base-content);
		outline: none;
	}

	.preview-pane {
		flex: 1;
		min-width: 0;
		border-left: 1px solid var(--base-300);
		background: var(--base-200);
		overflow-y: auto;
	}

	.preview-content {
		padding: 1.5rem;
	}

	.loading-state {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 1rem;
		color: var(--base-content-muted);
	}

	.note-footer {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		padding: 0.5rem 1rem;
		border-top: 1px solid var(--base-300);
		font-size: 0.75rem;
		color: var(--base-content-muted);
	}

	.dot {
		opacity: 0.5;
	}

	@media (max-width: 768px) {
		.note-header {
			flex-wrap: wrap;
			gap: 0.5rem;
			padding: 0.5rem;
		}

		.note-header-center {
			order: 3;
			width: 100%;
			justify-content: flex-start;
		}

		.note-title-input {
			text-align: left;
			max-width: none;
		}

		.note-editor-body {
			flex-direction: column;
		}

		.preview-pane {
			border-left: none;
			border-top: 1px solid var(--base-300);
		}
	}
</style>
