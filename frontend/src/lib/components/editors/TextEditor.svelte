<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { File } from '$lib/api/types';
	import { editFile, getFileContent } from '$lib/api/files';
	import { getMonacoLanguage } from '$lib/utils/editor';
	import BaseEditor from './BaseEditor.svelte';
	import { createEventDispatcher } from 'svelte';

	export let open = false;
	export let file: File | null = null;

	const dispatch = createEventDispatcher<{
		close: void;
		saved: { file: File };
	}>();

	let content = '';
	let originalContent = '';
	let isLoading = false;
	let isSaving = false;
	let error: string | null = null;
	let saveMode: 'overwrite' | 'new_version' = 'overwrite';
	let editorContainer: HTMLDivElement;
	let monaco: typeof import('monaco-editor') | null = null;
	let editor: import('monaco-editor').editor.IStandaloneCodeEditor | null = null;
	let hasChanges = false;

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
		insertSpaces: true,
	};

	async function loadMonaco() {
		if (typeof window === 'undefined') return null;
		
		// Dynamic import for Monaco
		const monacoModule = await import('monaco-editor');
		return monacoModule;
	}

	async function initEditor() {
		if (!editorContainer || !file) return;

		monaco = await loadMonaco();
		if (!monaco) return;

		const language = getMonacoLanguage(file.name);

		editor = monaco.editor.create(editorContainer, {
			...editorOptions,
			value: content,
			language,
		});

		// Listen for content changes
		editor.onDidChangeModelContent(() => {
			if (editor) {
				content = editor.getValue();
				hasChanges = content !== originalContent;
			}
		});
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

			// Update editor if it exists
			if (editor) {
				editor.setValue(content);
			}
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to load file content';
		} finally {
			isLoading = false;
		}
	}

	async function handleSave(event: CustomEvent<{ saveMode: 'overwrite' | 'new_version'; changeDescription?: string }>) {
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

	// Initialize editor when modal opens
	$: if (open && file) {
		loadContent();
		// Small delay to ensure container is rendered
		setTimeout(() => initEditor(), 0);
	}

	// Cleanup when modal closes
	$: if (!open && editor) {
		editor.dispose();
		editor = null;
		monaco = null;
	}

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
	<div bind:this={editorContainer} class="w-full h-full" />
</BaseEditor>

<style>
	:global(.monaco-editor) {
		padding-top: 8px;
	}
</style>
