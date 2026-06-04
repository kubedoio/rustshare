<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { File } from '$lib/api/types';
	import { editFile, getFileContent } from '$lib/api/files';
	import { getMonacoLanguage } from '$lib/utils/editor';
	import BaseEditor from './BaseEditor.svelte';
	import { createEventDispatcher } from 'svelte';
	import { Redo2, Undo2 } from 'lucide-svelte';

	let {
		open = false,
		file = null,
		onClose,
		onSaved
	}: {
		open?: boolean;
		file?: File | null;
		onClose?: () => void;
		onSaved?: (event: { file: File }) => void;
	} = $props();

	const dispatch = createEventDispatcher<{
		close: void;
		saved: { file: File };
	}>();

	let content = $state('');
	let originalContent = $state('');
	let isLoading = $state(false);
	let isSaving = $state(false);
	let error = $state<string | null>(null);
	let saveMode: 'overwrite' | 'new_version' = $state('new_version');
	let editorContainer: HTMLDivElement;
	let monaco: typeof import('monaco-editor') | null = $state(null);
	let editor: import('monaco-editor').editor.IStandaloneCodeEditor | null = $state(null);
	let hasChanges = $state(false);
	let canUndo = $state(false);
	let canRedo = $state(false);

	// Monaco editor options
	const editorOptions: import('monaco-editor').editor.IStandaloneEditorConstructionOptions = {
		automaticLayout: true,
		minimap: { enabled: true, scale: 1 },
		fontSize: 14,
		lineNumbers: 'on',
		roundedSelection: false,
		scrollBeyondLastLine: false,
		readOnly: false,
		theme: 'vs-dark',
		wordWrap: 'on',
		formatOnPaste: true,
		formatOnType: true,
		tabSize: 2,
		insertSpaces: true
	};

	async function loadMonaco() {
		if (typeof window === 'undefined') return null;

		// Dynamic import for Monaco
		const monacoModule = await import('monaco-editor');
		return monacoModule;
	}

	async function initEditor() {
		if (!editorContainer || !file || editor) return;
		const targetFileId = file.id;

		monaco = await loadMonaco();
		if (!monaco || !editorContainer || file?.id !== targetFileId) return;

		const language = getMonacoLanguage(file.name);

		editor = monaco.editor.create(editorContainer, {
			...editorOptions,
			value: content,
			language
		});

		// Listen for content changes
		editor.onDidChangeModelContent(() => {
			if (editor) {
				content = editor.getValue();
				hasChanges = content !== originalContent;
				updateHistoryState();
			}
		});

		editor.onDidChangeCursorSelection(updateHistoryState);
		updateHistoryState();

		// Force layout after a small delay to ensure it fits the container
		setTimeout(() => {
			if (editor) editor.layout();
		}, 100);
	}

	async function loadContent() {
		if (!file) return;
		const targetFileId = file.id;

		isLoading = true;
		error = null;

		try {
			const loadedContent = await getFileContent(targetFileId);
			if (file?.id !== targetFileId) return;
			content = loadedContent;
			originalContent = loadedContent;
			hasChanges = false;

			// Update editor if it exists
			if (editor) {
				editor.setValue(content);
				updateHistoryState();
			}
		} catch (err) {
			if (file?.id !== targetFileId) return;
			error = err instanceof Error ? err.message : 'Failed to load file content';
		} finally {
			isLoading = false;
		}
	}

	async function handleSave(
		event: CustomEvent<{ saveMode: 'overwrite' | 'new_version'; changeDescription?: string }>
	) {
		if (!file || !hasChanges) return;

		isSaving = true;
		error = null;

		try {
			const result = await editFile(
				file.id,
				content,
				event.detail.saveMode,
				event.detail.changeDescription
			);

			// Update file with new data
			file = {
				...file,
				size: result.size,
				current_version: result.current_version,
				modified_at: result.modified_at
			};

			originalContent = content;
			hasChanges = false;
			updateHistoryState();

			dispatch('saved', { file });
			onSaved?.({ file });
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to save file';
		} finally {
			isSaving = false;
		}
	}

	function handleClose() {
		dispatch('close');
		onClose?.();
	}

	function getUndoRedoModel() {
		return editor?.getModel() as
			| (import('monaco-editor').editor.ITextModel & {
					canUndo?: () => boolean;
					canRedo?: () => boolean;
			  })
			| null;
	}

	function updateHistoryState() {
		const model = getUndoRedoModel();
		canUndo = model?.canUndo?.() ?? false;
		canRedo = model?.canRedo?.() ?? false;
	}

	function undo() {
		if (!editor || !canUndo) return;
		editor.trigger('toolbar', 'undo', null);
		updateHistoryState();
	}

	function redo() {
		if (!editor || !canRedo) return;
		editor.trigger('toolbar', 'redo', null);
		updateHistoryState();
	}

	// Initialize editor when modal opens
	$effect(() => {
		if (open && file) {
			const targetFileId = file.id;
			// Await content loading before initializing the editor to prevent blank display
			loadContent().then(() => {
				if (file?.id !== targetFileId || editor) return;
				// Small delay to ensure container is fully bound and visible
				setTimeout(() => {
					if (file?.id === targetFileId) initEditor();
				}, 50);
			});
		}
	});

	// Cleanup when modal closes
	$effect(() => {
		if (!open && editor) {
			editor.dispose();
			editor = null;
			monaco = null;
		}
	});

	onDestroy(() => {
		if (editor) {
			editor.dispose();
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
	title="Edit Text File"
	on:close={handleClose}
	on:save={handleSave}
>
	<div class="flex h-full flex-col overflow-hidden">
		<div
			class="flex flex-wrap items-center gap-1 border-b border-base-300 bg-base-200 px-3 py-2"
			role="toolbar"
			aria-label="Text editor toolbar"
		>
			<button
				type="button"
				class="inline-flex items-center justify-center rounded-md p-1.5 text-base-content transition-colors hover:bg-base-300 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent"
				onclick={undo}
				disabled={!canUndo}
				title="Undo"
				aria-label="Undo"
			>
				<Undo2 size={16} />
			</button>
			<button
				type="button"
				class="inline-flex items-center justify-center rounded-md p-1.5 text-base-content transition-colors hover:bg-base-300 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent"
				onclick={redo}
				disabled={!canRedo}
				title="Redo"
				aria-label="Redo"
			>
				<Redo2 size={16} />
			</button>
		</div>
		<div bind:this={editorContainer} class="min-h-0 flex-1"></div>
	</div>
</BaseEditor>

<style>
	:global(.monaco-editor) {
		padding-top: 8px;
	}
</style>
