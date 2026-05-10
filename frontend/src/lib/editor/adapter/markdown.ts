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
// Table preprocessing
// ---------------------------------------------------------------------------

/**
 * Converts GFM-style Markdown tables into HTML tables.
 * tiptap-markdown does not natively parse GFM tables, but Tiptap's Table
 * extension can parse HTML tables when `html: true` is enabled on the
 * Markdown extension. This ensures tables survive the round-trip:
 *   markdown → HTML tables → Tiptap nodes → markdown (serialized back as GFM)
 */
export function preprocessMarkdownTables(markdown: string): string {
	if (!markdown.includes('|')) return markdown;

	// Extract fenced code blocks so we don't mangle | inside them
	const codeBlocks: string[] = [];
	let processed = markdown.replace(/```[\s\S]*?```/g, (match) => {
		const placeholder = `\0CODEBLOCK${codeBlocks.length}\0`;
		codeBlocks.push(match);
		return placeholder;
	});

	// Also protect inline code segments that contain |
	const inlineCodes: string[] = [];
	processed = processed.replace(/`[^`\n]*\|[^`\n]*`/g, (match) => {
		const placeholder = `\0INLINECODE${inlineCodes.length}\0`;
		inlineCodes.push(match);
		return placeholder;
	});

	// Process tables line by line
	const lines = processed.split('\n');
	const result: string[] = [];
	let i = 0;

	while (i < lines.length) {
		const line = lines[i];
		if (line.includes('|')) {
			const tableLines: string[] = [];
			let j = i;
			while (j < lines.length && lines[j].includes('|')) {
				tableLines.push(lines[j]);
				j++;
			}

			if (tableLines.length >= 2 && isTableSeparator(tableLines[1])) {
				result.push(convertTableLinesToHtml(tableLines));
				i = j;
				continue;
			}
		}
		result.push(lines[i]);
		i++;
	}

	processed = result.join('\n');

	// Restore inline codes
	inlineCodes.forEach((code, idx) => {
		processed = processed.replace(`\0INLINECODE${idx}\0`, code);
	});

	// Restore code blocks
	codeBlocks.forEach((block, idx) => {
		processed = processed.replace(`\0CODEBLOCK${idx}\0`, block);
	});

	return processed;
}

function isTableSeparator(line: string): boolean {
	const trimmed = line.trim();
	return /^[\s|:-]+$/.test(trimmed) && trimmed.length > 0 && /-/.test(trimmed);
}

function parseTableRow(line: string): string[] {
	const trimmed = line.trim();
	let content = trimmed.startsWith('|') ? trimmed.slice(1) : trimmed;
	if (content.endsWith('|')) {
		content = content.slice(0, -1);
	}
	return content.split('|');
}

function convertTableLinesToHtml(lines: string[]): string {
	const headerCells = parseTableRow(lines[0]);
	const bodyRows: string[][] = [];
	for (let i = 2; i < lines.length; i++) {
		bodyRows.push(parseTableRow(lines[i]));
	}

	let html = '<table class="editor-table">';

	html += '<thead><tr>';
	headerCells.forEach((cell) => {
		html += `<th>${cell.trim()}</th>`;
	});
	html += '</tr></thead>';

	if (bodyRows.length > 0) {
		html += '<tbody>';
		bodyRows.forEach((cells) => {
			html += '<tr>';
			cells.forEach((cell) => {
				html += `<td>${cell.trim()}</td>`;
			});
			html += '</tr>';
		});
		html += '</tbody>';
	}

	html += '</table>';
	return html;
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
		const preprocessed = preprocessMarkdownTables(markdown);
		const editor = new Editor({
			extensions: getEditorExtensions(),
			content: preprocessed,
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
	const preprocessed = preprocessMarkdownTables(options.content || '');

	const editor = new Editor({
		element: options.element,
		extensions: getEditorExtensions(),
		content: preprocessed,
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
