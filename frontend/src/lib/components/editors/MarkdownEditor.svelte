<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { File } from '$lib/api/types';
	import { editFile, getFileContent } from '$lib/api/files';
	import { renderMarkdown } from '$lib/utils/markdown';
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
	let showPreview = true;



	$: renderedPreview = renderMarkdown(content);

	// Monaco editor options
	const editorOptions: import('monaco-editor').editor.IStandaloneEditorConstructionOptions = {
		automaticLayout: true,
		minimap: { enabled: false },
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
		
		const monacoModule = await import('monaco-editor');
		return monacoModule;
	}

	async function initEditor() {
		if (!editorContainer || !file || editor) return;

		monaco = await loadMonaco();
		if (!monaco || !editorContainer) return;

		editor = monaco.editor.create(editorContainer, {
			...editorOptions,
			value: content,
			language: 'markdown',
		});

		// Listen for content changes
		editor.onDidChangeModelContent(() => {
			if (editor) {
				content = editor.getValue();
				hasChanges = content !== originalContent;
			}
		});

		// Force layout after a small delay to ensure it fits the container
		setTimeout(() => {
			if (editor) editor.layout();
		}, 100);
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

	// Toolbar actions
	function insertText(before: string, after: string = '') {
		if (!editor) return;
		const selection = editor.getSelection();
		if (!selection) return;
		
		const model = editor.getModel();
		if (!model) return;

		const selectedText = model.getValueInRange(selection);
		const newText = before + selectedText + after;
		
		editor.executeEdits('toolbar', [{
			range: selection,
			text: newText,
			forceMoveMarkers: true
		}]);
		
		editor.focus();
	}

	$: if (open && file) {
		// Await content loading before initializing the editor to prevent blank display
		loadContent().then(() => {
			if (!editor) {
				// Small delay to ensure container is fully bound and visible
				setTimeout(() => initEditor(), 50);
			}
		});
	}

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
	title="Edit Markdown"
	on:close={handleClose}
	on:save={handleSave}
>
	<div class="flex flex-col h-full">
		<!-- Toolbar -->
		<div class="border-b border-base-300 px-4 py-2 flex items-center gap-2">
			<div class="join">
				<button class="btn btn-xs join-item" on:click={() => insertText('**', '**')} title="Bold">
					<strong>B</strong>
				</button>
				<button class="btn btn-xs join-item" on:click={() => insertText('*', '*')} title="Italic">
					<em>I</em>
				</button>
				<button class="btn btn-xs join-item" on:click={() => insertText('`', '`')} title="Code">
					&lt;/&gt;
				</button>
			</div>
			<div class="divider divider-horizontal mx-1"></div>
			<div class="join">
				<button class="btn btn-xs join-item" on:click={() => insertText('# ')} title="Heading 1">
					H1
				</button>
				<button class="btn btn-xs join-item" on:click={() => insertText('## ')} title="Heading 2">
					H2
				</button>
				<button class="btn btn-xs join-item" on:click={() => insertText('### ')} title="Heading 3">
					H3
				</button>
			</div>
			<div class="divider divider-horizontal mx-1"></div>
			<div class="join">
				<button class="btn btn-xs join-item" on:click={() => insertText('- ')} title="Bullet List">
					• List
				</button>
				<button class="btn btn-xs join-item" on:click={() => insertText('1. ')} title="Numbered List">
					1. List
				</button>
				<button class="btn btn-xs join-item" on:click={() => insertText('> ')} title="Quote">
					" Quote
				</button>
			</div>
			<div class="divider divider-horizontal mx-1"></div>
			<div class="join">
				<button class="btn btn-xs join-item" on:click={() => insertText('[', '](url)')} title="Link">
					Link
				</button>
				<button class="btn btn-xs join-item" on:click={() => insertText('```\n', '\n```')} title="Code Block">
					Code Block
				</button>
			</div>
			<div class="flex-1"></div>
			<button
				class="btn btn-xs btn-ghost"
				on:click={() => showPreview = !showPreview}
			>
				{showPreview ? 'Hide Preview' : 'Show Preview'}
			</button>
		</div>

		<!-- Editor and Preview -->
		<div class="flex-1 flex overflow-hidden">
			<div bind:this={editorContainer} class="flex-1 {showPreview ? 'w-1/2' : 'w-full'}" />
			
			{#if showPreview}
				<div class="w-1/2 border-l border-base-300 overflow-auto p-4 bg-base-100">
					<div class="prose prose-sm max-w-none">
						{@html renderedPreview}
					</div>
				</div>
			{/if}
		</div>
	</div>
</BaseEditor>
