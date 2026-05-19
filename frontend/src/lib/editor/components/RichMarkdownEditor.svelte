<!--
  RichMarkdownEditor — editable Tiptap editor with toolbar and slash commands.
  Accepts Markdown, provides Markdown output. The Tiptap instance lives
  inside this component and is created/destroyed via Svelte lifecycle.
-->
<script lang="ts">
	import { onMount, onDestroy, createEventDispatcher } from 'svelte';
	import type { Editor } from '@tiptap/core';
	import { NodeSelection } from '@tiptap/pm/state';
	import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
	import type { EditorView } from '@tiptap/pm/view';
	import { createRichEditor, editorToMarkdown } from '../adapter/markdown';
	import type { SlashCommand } from '../adapter/slash-commands';
	import { validateAttachmentUpload } from '../adapter/attachments';
	import type { EditorPermissions } from '../types';
	import { WRITE_PERMISSIONS } from '../types';
	import EditorToolbar from './EditorToolbar.svelte';
	import SlashCommandMenu from './SlashCommandMenu.svelte';
	import ExcalidrawEditor from './ExcalidrawEditor.svelte';

	let {
		content = '',
		editable = true,
		hasAttachmentHandler = false,
		currentMarkdown = content,
		syncExternalContent = true,
		ydoc = undefined
	}: {
		content?: string;
		editable?: boolean;
		hasAttachmentHandler?: boolean;
		currentMarkdown?: string;
		syncExternalContent?: boolean;
		ydoc?: import('yjs').Doc | undefined;
	} = $props();

	const dispatch = createEventDispatcher<{
		change: { markdown: string };
		ready: { editor: Editor };
		attachment: { type: 'image' | 'file' };
		sketch: { blob: Blob; filename: string };
		filedrop: { files: File[] };
		paste: { files: File[] };
	}>();

	let editorElement: HTMLDivElement;
	let editorWrapperElement: HTMLDivElement;
	let editor: Editor | null = $state.raw(null);
	let editorTick = $state(0);
	let initialized = $state(false);
	let isDragOver = $state(false);
	let markdownUpdateTimer: ReturnType<typeof setTimeout> | null = $state(null);
	let localMarkdown = $state(currentMarkdown);

	// Slash menu state
	let showSlashMenu = $state(false);
	let slashQuery = $state('');
	let slashMenuTop = $state(0);
	let slashMenuLeft = $state(0);
	let slashRangeFrom = $state(0);

	// Excalidraw state
	let showExcalidraw = $state(false);
	let excalidrawInitialData: { elements?: any[]; appState?: any; files?: any } | null = $state.raw(null);
	let sketchEditPos: number | null = $state(null);

	onMount(() => {
		if (!editorElement) return;
		editorElement.replaceChildren();

		editor = createRichEditor({
			element: editorElement,
			content: ydoc ? '' : content,
			editable,
			ydoc,
			editorProps: {
				handleClickOn(view: EditorView, pos: number, node: ProseMirrorNode) {
					if (node.type.name === 'image') {
						view.dispatch(view.state.tr.setSelection(NodeSelection.create(view.state.doc, pos)));
						return true;
					}
					return false;
				},
				handleDoubleClickOn(
					view: EditorView,
					pos: number,
					node: ProseMirrorNode,
					nodePos: number,
					event: MouseEvent
				) {
					if (node.type.name !== 'image') return false;

					const src = node.attrs.src as string;
					const alt = (node.attrs.alt as string) || '';
					if (!src || !src.startsWith('data:')) return false;

					const isSketch = alt === 'Sketch' || alt === 'Excalidraw Sketch';
					if (!isSketch) return false;

					event.preventDefault();
					event.stopPropagation();

					// Launch async without awaiting in the sync handler
					launchSketchEdit(src, pos);
					return true;
				}
			},
			onDocumentUpdate: () => {
				scheduleMarkdownUpdate();
				checkSlashTrigger();
			},
			onSelectionUpdate: () => {
				editorTick++; // Trigger Svelte reactivity
				if (showSlashMenu) {
					checkSlashTrigger();
				}
			},
			onCreate: () => {
				initialized = true;
				if (editor) {
					dispatch('ready', { editor });
				}
			}
		});

		// Intercept keydown for slash menu navigation
		editor.view.dom.addEventListener('keydown', handleEditorKeydown);
		// Intercept paste for image/file paste
		editor.view.dom.addEventListener('paste', handlePaste);
	});

	onDestroy(() => {
		if (markdownUpdateTimer) {
			clearTimeout(markdownUpdateTimer);
			markdownUpdateTimer = null;
		}
		if (editor) {
			editor.view.dom.removeEventListener('keydown', handleEditorKeydown);
			editor.view.dom.removeEventListener('paste', handlePaste);
			editor.destroy();
			editor = null;
		}
		editorElement?.replaceChildren();
	});

	// React to editable prop changes
	$effect(() => {
		if (editor && initialized) {
			editor.setEditable(editable);
		}
	});

	import { untrack } from 'svelte';

	// React to external content changes (e.g. after save + refetch)
	let lastExternalContent = $state(content);
	$effect(() => {
		// Only sync if content actually changed from what we last saw from outside
		if (editor && initialized && syncExternalContent && content !== lastExternalContent) {
			const newContent = content;
			untrack(() => {
				if (newContent !== editorToMarkdown(editor!)) {
					editor!.commands.setContent(newContent, { emitUpdate: false });
				}
				lastExternalContent = newContent;
				localMarkdown = newContent;
			});
		}
	});

	$effect(() => {
		// Keep lastExternalContent in sync even if syncExternalContent is false
		if (!syncExternalContent && content !== lastExternalContent) {
			const newContent = content;
			untrack(() => {
				lastExternalContent = newContent;
			});
		}
	});

	function scheduleMarkdownUpdate() {
		if (markdownUpdateTimer) clearTimeout(markdownUpdateTimer);
		markdownUpdateTimer = setTimeout(() => {
			markdownUpdateTimer = null;
			if (!editor) return;
			const md = editorToMarkdown(editor);
			localMarkdown = md;
			dispatch('change', { markdown: md });
		}, 250);
	}

	// --- Slash Menu Logic ---

	function checkSlashTrigger() {
		if (!editor) return;

		const { state } = editor;
		const { from, empty } = state.selection;
		if (!empty) {
			closeSlashMenu();
			return;
		}

		// Get text from start of current block to cursor
		const resolvedPos = state.doc.resolve(from);
		const blockStart = resolvedPos.start();
		const textBefore = state.doc.textBetween(blockStart, from, '\0');

		// Match /query pattern (slash at start of block)
		const match = textBefore.match(/^\/(\S*)$/);
		if (match) {
			slashQuery = match[1];
			slashRangeFrom = blockStart;

			// Position menu below cursor
			try {
				const coords = editor.view.coordsAtPos(from);
				slashMenuTop = coords.bottom + 4;
				slashMenuLeft = coords.left;
			} catch {
				// Fallback position
				const rect = editorWrapperElement?.getBoundingClientRect();
				if (rect) {
					slashMenuTop = rect.top + 40;
					slashMenuLeft = rect.left + 24;
				}
			}

			showSlashMenu = true;
		} else {
			closeSlashMenu();
		}
	}

	function closeSlashMenu() {
		showSlashMenu = false;
		slashQuery = '';
	}

	function handleSlashSelect(event: CustomEvent<{ command: SlashCommand }>) {
		const { command } = event.detail;
		if (!editor) return;

		// Delete the /query text
		const { state } = editor;
		const { from } = state.selection;
		const resolvedPos = state.doc.resolve(from);
		const blockStart = resolvedPos.start();

		editor.chain().focus().deleteRange({ from: blockStart, to: from }).run();

		// Handle media commands via events
		if (command.id === 'sketch') {
			showExcalidraw = true;
		} else if (command.requiresAttachmentHandler) {
			dispatch('attachment', { type: command.id === 'image' ? 'image' : 'file' });
		} else {
			command.action(editor);
		}

		closeSlashMenu();
	}

	function handleEditorKeydown(event: KeyboardEvent) {
		if (!showSlashMenu) return;

		// Let the slash menu handle navigation keys
		if (['ArrowDown', 'ArrowUp', 'Enter', 'Escape'].includes(event.key)) {
			// The SlashCommandMenu component handles these via its own keydown
			// We need to prevent Tiptap from handling them
			const menuEl = document.querySelector('.slash-menu') as HTMLElement;
			if (menuEl) {
				event.preventDefault();
				event.stopPropagation();
				menuEl.dispatchEvent(
					new KeyboardEvent('keydown', {
						key: event.key,
						code: event.code,
						bubbles: true
					})
				);
			}
		}
	}

	/**
	 * Handles paste events to intercept file pastes (e.g. images from clipboard).
	 */
	function handlePaste(event: ClipboardEvent) {
		if (!editable || !hasAttachmentHandler) return;
		const items = event.clipboardData?.items;
		if (!items) return;

		const files: File[] = [];
		for (const item of items) {
			if (item.kind === 'file') {
				const file = item.getAsFile();
				if (file) files.push(file);
			}
		}

		if (files.length > 0) {
			event.preventDefault();
			event.stopPropagation();
			dispatch('paste', { files });
		}
	}

	/**
	 * Converts a base64 data URL to a Blob.
	 */
	function dataURLToBlob(dataURL: string): Blob {
		const arr = dataURL.split(',');
		const mime = arr[0].match(/:(.*?);/)?.[1] || 'image/png';
		const bstr = atob(arr[1]);
		let n = bstr.length;
		const u8arr = new Uint8Array(n);
		while (n--) {
			u8arr[n] = bstr.charCodeAt(n);
		}
		return new Blob([u8arr], { type: mime });
	}

	/**
	 * Loads an embedded Excalidraw scene from a base64 data URL and opens the editor.
	 */
	async function launchSketchEdit(src: string, pos: number) {
		if (!editor?.isEditable) return;
		try {
			let blob: Blob;
			if (src.startsWith('data:')) {
				blob = dataURLToBlob(src);
			} else {
				const response = await fetch(src);
				blob = await response.blob();
			}
			const { loadFromBlob } = await import('@excalidraw/excalidraw');
			const scene = await loadFromBlob(blob, null, null);

			excalidrawInitialData = {
				elements: scene.elements,
				appState: scene.appState,
				files: scene.files
			};
			sketchEditPos = pos;
			showExcalidraw = true;
		} catch (err) {
			console.error('Failed to load sketch for editing:', err);
			// If loadFromBlob fails (e.g. no embedded scene), just open empty
			excalidrawInitialData = null;
			sketchEditPos = pos;
			showExcalidraw = true;
		}
	}

	/**
	 * Handles sketch save from Excalidraw — inserts new or replaces existing.
	 */
	function handleSketchSave(event: CustomEvent<{ blob: Blob; filename: string }>) {
		const reader = new FileReader();
		reader.onload = () => {
			const dataUrl = reader.result as string;
			if (!editor) return;

			const pos = sketchEditPos;
			if (pos !== null) {
				// Replace existing sketch image
				editor
					.chain()
					.focus()
					.command(({ tr }) => {
						const node = editor!.schema.nodes.image.create({
							src: dataUrl,
							alt: 'Sketch',
							title: null
						});
						tr.replaceWith(pos, pos + 1, node);
						return true;
					})
					.run();
			} else {
				// New sketch — dispatch to parent for insertion
				dispatch('sketch', event.detail);
			}

			// Reset state
			showExcalidraw = false;
			excalidrawInitialData = null;
			sketchEditPos = null;
		};
		reader.readAsDataURL(event.detail.blob);
	}

	// --- Drag & Drop ---

	function handleDragEnter(event: DragEvent) {
		if (!editable || !hasAttachmentHandler) return;
		if (event.dataTransfer?.types.includes('Files')) {
			event.preventDefault();
			isDragOver = true;
		}
	}

	function handleDragOver(event: DragEvent) {
		if (!editable || !hasAttachmentHandler) return;
		if (event.dataTransfer?.types.includes('Files')) {
			event.preventDefault();
		}
	}

	function handleDragLeave(event: DragEvent) {
		const related = event.relatedTarget as Node | null;
		if (!editorWrapperElement?.contains(related)) {
			isDragOver = false;
		}
	}

	function handleDrop(event: DragEvent) {
		isDragOver = false;
		if (!editable || !hasAttachmentHandler) return;

		const droppedFiles = event.dataTransfer?.files;
		if (!droppedFiles?.length) return;

		event.preventDefault();
		const files = Array.from(droppedFiles);
		dispatch('filedrop', { files });
	}

	// --- Public API ---

	/**
	 * Returns the current Markdown content.
	 */
	export function getMarkdown(): string {
		if (!editor) return content;
		if (markdownUpdateTimer) {
			clearTimeout(markdownUpdateTimer);
			markdownUpdateTimer = null;
		}
		const md = editorToMarkdown(editor);
		localMarkdown = md;
		return md;
	}

	/**
	 * Replaces the editor content with new Markdown.
	 */
	export function setContent(markdown: string): void {
		if (editor) {
			editor.commands.setContent(markdown, { emitUpdate: false });
			localMarkdown = markdown;
			lastExternalContent = markdown;
		}
	}

	/**
	 * Returns the Tiptap editor instance for external operations
	 * (e.g. inserting attachment Markdown after upload).
	 */
	export function getEditor(): Editor | null {
		return editor;
	}
</script>

<div
	class="rich-markdown-editor"
	class:readonly={!editable}
	class:drag-over={isDragOver}
	bind:this={editorWrapperElement}
	on:dragenter={handleDragEnter}
	on:dragover={handleDragOver}
	on:dragleave={handleDragLeave}
	on:drop={handleDrop}
	role="region"
	aria-label="Markdown Editor"
>
	{#if editable}
		<EditorToolbar
			{editor}
			{hasAttachmentHandler}
			on:attachment={() => dispatch('attachment', { type: 'file' })}
		/>
	{/if}

	<div class="editor-wrapper">
		<div bind:this={editorElement} class="editor-container"></div>
	</div>

	{#if isDragOver}
		<div class="drop-overlay">
			<span class="drop-label">Drop files to attach</span>
		</div>
	{/if}

	{#if showSlashMenu}
		<SlashCommandMenu
			query={slashQuery}
			top={slashMenuTop}
			left={slashMenuLeft}
			{hasAttachmentHandler}
			on:select={handleSlashSelect}
			on:close={closeSlashMenu}
		/>
	{/if}

	<ExcalidrawEditor
		open={showExcalidraw}
		initialData={excalidrawInitialData}
		on:close={() => {
			showExcalidraw = false;
			excalidrawInitialData = null;
			sketchEditPos = null;
		}}
		on:save={handleSketchSave}
	/>
</div>

<style>
	.rich-markdown-editor {
		display: flex;
		flex-direction: column;
		height: 100%;
		border: 1px solid var(--color-base-300, #e5e7eb);
		border-radius: 0.5rem;
		overflow: hidden;
		background: var(--color-base-100, #fff);
		position: relative;
	}

	.editor-wrapper {
		flex: 1;
		overflow-y: auto;
	}

	.editor-container {
		min-height: 100%;
	}

	/* ProseMirror content styling */
	.editor-container :global(.ProseMirror) {
		padding: 1.5rem;
		outline: none;
		min-height: 200px;
		line-height: 1.7;
		color: var(--color-base-content, #374151);
	}

	/* Placeholder */
	.editor-container :global(.ProseMirror p.is-editor-empty:first-child::before) {
		content: attr(data-placeholder);
		float: left;
		color: var(--color-base-content, #9ca3af);
		opacity: 0.4;
		pointer-events: none;
		height: 0;
		font-style: italic;
	}

	/* Headings */
	.editor-container :global(.ProseMirror h1) {
		font-size: 1.875rem;
		font-weight: 700;
		margin: 1.5rem 0 0.75rem;
		line-height: 1.2;
	}

	.editor-container :global(.ProseMirror h2) {
		font-size: 1.5rem;
		font-weight: 600;
		margin: 1.25rem 0 0.625rem;
		line-height: 1.3;
	}

	.editor-container :global(.ProseMirror h3) {
		font-size: 1.25rem;
		font-weight: 600;
		margin: 1rem 0 0.5rem;
		line-height: 1.4;
	}

	/* Paragraphs */
	.editor-container :global(.ProseMirror p) {
		margin: 0 0 0.75rem;
	}

	/* Lists */
	.editor-container :global(.ProseMirror ul:not(.editor-task-list)) {
		list-style-type: disc;
		padding-left: 1.5rem;
		margin: 0 0 0.75rem;
	}

	.editor-container :global(.ProseMirror ol) {
		list-style-type: decimal;
		padding-left: 1.5rem;
		margin: 0 0 0.75rem;
	}

	.editor-container :global(.ProseMirror li) {
		margin-bottom: 0.25rem;
	}

	/* Task lists */
	.editor-container :global(.ProseMirror .editor-task-list) {
		list-style: none;
		padding-left: 0;
		margin: 0 0 0.75rem;
	}

	.editor-container :global(.ProseMirror .editor-task-item) {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		margin-bottom: 0.25rem;
	}

	.editor-container :global(.ProseMirror .editor-task-item label) {
		display: flex;
		align-items: center;
		margin-top: 0.15rem;
	}

	.editor-container :global(.ProseMirror .editor-task-item input[type='checkbox']) {
		width: 1rem;
		height: 1rem;
		cursor: pointer;
		accent-color: var(--color-primary, #3b82f6);
	}

	.editor-container :global(.ProseMirror .editor-task-item[data-checked='true'] > div > p) {
		text-decoration: line-through;
		opacity: 0.6;
	}

	/* Blockquote */
	.editor-container :global(.ProseMirror blockquote) {
		border-left: 3px solid var(--color-primary, #3b82f6);
		padding-left: 1rem;
		margin: 0.75rem 0;
		color: var(--color-base-content, #6b7280);
		font-style: italic;
	}

	/* Code */
	.editor-container :global(.ProseMirror code) {
		font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
		font-size: 0.875em;
		background: var(--color-base-300, #e5e7eb);
		padding: 0.125rem 0.375rem;
		border-radius: 0.25rem;
	}

	.editor-container :global(.ProseMirror pre) {
		background: var(--color-base-200, #1e1e1e);
		padding: 1rem;
		border-radius: 0.5rem;
		overflow-x: auto;
		margin: 0.75rem 0;
	}

	.editor-container :global(.ProseMirror pre code) {
		background: none;
		padding: 0;
		font-size: 0.875rem;
	}

	/* Links */
	.editor-container :global(.ProseMirror a) {
		color: var(--color-primary, #3b82f6);
		text-decoration: underline;
		cursor: pointer;
	}

	/* Horizontal rule */
	.editor-container :global(.ProseMirror hr) {
		border: none;
		border-top: 1px solid var(--color-base-300, #e5e7eb);
		margin: 1.5rem 0;
	}

	/* Images */
	.editor-container :global(.ProseMirror img) {
		cursor: pointer;
		transition: box-shadow 0.15s ease;
		max-width: 100%;
		height: auto;
	}

	.editor-container :global(.ProseMirror img:hover) {
		box-shadow: 0 0 0 3px color-mix(in oklab, var(--color-primary, #3b82f6) 25%, transparent);
	}

	.editor-container :global(.ProseMirror .ProseMirror-selectednode) {
		outline: 3px solid var(--color-primary, #3b82f6);
		outline-offset: 2px;
	}

	/* Table */
	.editor-container :global(.ProseMirror .editor-table) {
		border-collapse: collapse;
		width: 100%;
		margin: 0.75rem 0;
		table-layout: fixed;
	}

	.editor-container :global(.ProseMirror .editor-table td),
	.editor-container :global(.ProseMirror .editor-table th) {
		border: 1px solid var(--color-base-300, #d1d5db);
		padding: 0.5rem 0.75rem;
		vertical-align: top;
		min-width: 80px;
	}

	.editor-container :global(.ProseMirror .editor-table th) {
		background: var(--color-base-200, #f3f4f6);
		font-weight: 600;
	}

	.editor-container :global(.ProseMirror .editor-table .selectedCell) {
		background: color-mix(in oklab, var(--color-primary, #3b82f6) 10%, transparent);
	}

	/* Underline */
	.editor-container :global(.ProseMirror u) {
		text-decoration: underline;
	}

	/* Read-only state */
	.readonly {
		border-color: transparent;
	}

	.readonly .editor-container :global(.ProseMirror) {
		cursor: default;
	}

	/* Drag & Drop overlay */
	.drag-over {
		border-color: var(--color-primary, #3b82f6);
	}

	.drop-overlay {
		position: absolute;
		inset: 0;
		z-index: 30;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(
			in oklab,
			var(--color-primary, #3b82f6) 8%,
			var(--color-base-100, #fff) 92%
		);
		border: 2px dashed var(--color-primary, #3b82f6);
		border-radius: 0.5rem;
		pointer-events: none;
	}

	.drop-label {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-primary, #3b82f6);
		padding: 0.5rem 1rem;
		border-radius: 0.375rem;
		background: var(--color-base-100, #fff);
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
	}
</style>
