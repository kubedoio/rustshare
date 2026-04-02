<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { File } from '$lib/api/types';
	import { editFile, getFileContent } from '$lib/api/files';
	import BaseEditor from './BaseEditor.svelte';
	import { createEventDispatcher } from 'svelte';

	export let open = false;
	export let file: File | null = null;

	const dispatch = createEventDispatcher<{
		close: void;
		saved: { file: File };
	});

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

	// Simple markdown to HTML conversion (basic)
	function renderMarkdown(markdown: string): string {
		if (!markdown) return '';

		return (
			markdown
				// Escape HTML
				.replace(/&/g, '&amp;')
				.replace(/</g, '&lt;')
				.replace(/>/g, '&gt;')
				// Headers
				.replace(/^### (.*$)/gim, '<h3>$1</h3>')
				.replace(/^## (.*$)/gim, '<h2>$1</h2>')
				.replace(/^# (.*$)/gim, '<h1>$1</h1>')
				// Bold
				.replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
				.replace(/__(.*?)__/g, '<strong>$1</strong>')
				// Italic
				.replace(/\*(.*?)\*/g, '<em>$1</em>')
				.replace(/_(.*?)_/g, '<em>$1</em>')
				// Code inline
				.replace(/`([^`]+)`/g, '<code class="bg-base-300 px-1 rounded text-sm">$1</code>')
				// Code blocks
				.replace(/```(\w+)?\n([\s\S]*?)```/g, '<pre class="bg-base-300 p-3 rounded-lg overflow-x-auto my-2"><code>$2</code></pre>')
				// Blockquotes
				.replace(/^&gt; (.*$)/gim, '<blockquote class="border-l-4 border-primary pl-4 my-2 italic">$1</blockquote>')
				// Links
				.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="text-primary hover:underline" target="_blank">$1</a>')
				// Images
				.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '<img src="$2" alt="$1" class="max-w-full rounded-lg my-2" />')
				// Unordered lists
				.replace(/^\s*- (.*$)/gim, '<li class="ml-4">$1</li>')
				.replace(/(<li.*<\/li>\n)+/g, '<ul class="list-disc my-2">$&</ul>')
				// Ordered lists
				.replace(/^\s*\d+\. (.*$)/gim, '<li class="ml-4">$1</li>')
				.replace(/(<li.*<\/li>\n)+/g, '<ol class="list-decimal my-2">$&</ol>')
				// Horizontal rule
				.replace(/^---$/gim, '<hr class="my-4 border-base-300" />')
				// Line breaks
				.replace(/\n/g, '<br />')
		);
	}

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
		if (!editorContainer || !file) return;

		monaco = await loadMonaco();
		if (!monaco) return;

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
		loadContent();
		setTimeout(() => initEditor(), 0);
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
			<div bind:this={editorContainer} class="flex-1" class:w-full={!showPreview} class:w-1/2={showPreview} />
			
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
