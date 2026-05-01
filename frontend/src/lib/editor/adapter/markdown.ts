/**
 * Markdown ↔ Tiptap adapter.
 * Converts between canonical Markdown and Tiptap editor state.
 */

import { Editor } from '@tiptap/core';
import { getEditorExtensions } from './extensions';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface MarkdownParseResult {
	success: boolean;
	html: string;
	error?: string;
}

// ---------------------------------------------------------------------------
// Markdown → HTML (for viewer)
// ---------------------------------------------------------------------------

/**
 * Converts Markdown to sanitized HTML using a headless Tiptap editor.
 * Uses the same extensions as the interactive editor for consistent rendering.
 */
export function markdownToHtml(markdown: string): MarkdownParseResult {
	if (!markdown || !markdown.trim()) {
		return { success: true, html: '' };
	}

	try {
		const editor = new Editor({
			extensions: getEditorExtensions(),
			content: markdown,
			editable: false
		});

		const html = editor.getHTML();
		editor.destroy();

		return { success: true, html };
	} catch (err) {
		return {
			success: false,
			html: '',
			error: err instanceof Error ? err.message : 'Failed to parse Markdown'
		};
	}
}

// ---------------------------------------------------------------------------
// Editor → Markdown (for saving)
// ---------------------------------------------------------------------------

/**
 * Extracts the canonical Markdown content from a Tiptap editor instance.
 */
export function editorToMarkdown(editor: Editor): string {
	if (!editor) return '';

	try {
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		const storage = editor.storage as any;
		const markdownStorage = storage?.markdown as { getMarkdown: () => string } | undefined;

		if (markdownStorage?.getMarkdown) {
			return markdownStorage.getMarkdown();
		}

		// Fallback: return plain text if markdown extension missing
		return editor.getText();
	} catch {
		return editor.getText();
	}
}

// ---------------------------------------------------------------------------
// Create Editor Instance
// ---------------------------------------------------------------------------

export interface CreateEditorOptions {
	element?: HTMLElement;
	content?: string;
	editable?: boolean;
	onUpdate?: (markdown: string) => void;
	onSelectionUpdate?: () => void;
	onCreate?: () => void;
}

/**
 * Creates a Tiptap editor with standard RustShare extensions.
 * The editor accepts Markdown input and can serialize back to Markdown.
 */
export function createRichEditor(options: CreateEditorOptions): Editor {
	const editor = new Editor({
		element: options.element,
		extensions: getEditorExtensions(),
		content: options.content || '',
		editable: options.editable ?? true,
		editorProps: {
			attributes: {
				class: 'rich-editor-content'
			}
		},
		onCreate: () => {
			options.onCreate?.();
		},
		onUpdate: ({ editor: e }) => {
			if (options.onUpdate) {
				const md = editorToMarkdown(e);
				options.onUpdate(md);
			}
		},
		onSelectionUpdate: () => {
			options.onSelectionUpdate?.();
		}
	});

	return editor;
}
