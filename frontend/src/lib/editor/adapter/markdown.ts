/**
 * Markdown ↔ Tiptap adapter.
 * Converts between canonical Markdown and Tiptap editor state.
 */

import { Editor } from '@tiptap/core';
import type { EditorProps } from '@tiptap/pm/view';
import markdownit from 'markdown-it';
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
 * Shared markdown-it instance for rendering inline cell content.
 * Using the same options as tiptap-markdown keeps table cell parsing
 * consistent with the rest of the editor.
 */
const tableCellMarkdown = markdownit({ html: true });

/**
 * Renders a table cell's inline Markdown content to HTML.
 * This consumes Markdown escape sequences (e.g. \~ becomes ~) so that
 * Tiptap stores the literal characters and serialization does not add
 * additional backslashes on every save/load round-trip.
 */
function renderCellContent(content: string): string {
	if (!content) return '';
	return tableCellMarkdown.renderInline(content);
}

/**
 * Restores inline-code placeholders inside a single table cell before
 * the cell is rendered to HTML. This keeps `|` characters inside inline
 * code from breaking the table structure while still letting markdown-it
 * format the code span correctly.
 */
function restoreInlineCodes(text: string, inlineCodes: string[]): string {
	let result = text;
	inlineCodes.forEach((code, idx) => {
		result = result.replace(`\0INLINECODE${idx}\0`, code);
	});
	return result;
}

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
				result.push(convertTableLinesToHtml(tableLines, inlineCodes));
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

function convertTableLinesToHtml(lines: string[], inlineCodes: string[]): string {
	const headerCells = parseTableRow(lines[0]);
	const bodyRows: string[][] = [];
	for (let i = 2; i < lines.length; i++) {
		bodyRows.push(parseTableRow(lines[i]));
	}

	let html = '<table class="editor-table">';

	html += '<thead><tr>';
	headerCells.forEach((cell) => {
		html += `<th>${renderCellContent(restoreInlineCodes(cell.trim(), inlineCodes))}</th>`;
	});
	html += '</tr></thead>';

	if (bodyRows.length > 0) {
		html += '<tbody>';
		bodyRows.forEach((cells) => {
			html += '<tr>';
			cells.forEach((cell) => {
				html += `<td>${renderCellContent(restoreInlineCodes(cell.trim(), inlineCodes))}</td>`;
			});
			html += '</tr>';
		});
		html += '</tbody>';
	}

	html += '</table>';
	return html;
}

// ---------------------------------------------------------------------------
// Wikilink preprocessing
// ---------------------------------------------------------------------------

export interface WikilinkPlaceholder {
	placeholder: string;
	type: 'link' | 'image';
	path: string;
	display: string;
}

function makeWikilinkPlaceholder(index: number): string {
	return `\u00ABWIKILINK-${index}\u00BB`;
}

function escapeHtmlAttribute(text: string): string {
	return text
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;');
}

/**
 * Converts Obsidian-style wikilinks into placeholders so they survive
 * Tiptap rendering. Placeholders are restored to HTML with data attributes
 * after Tiptap produces HTML. This ensures the original Markdown is never
 * modified.
 */
export function preprocessWikilinks(markdown: string): {
	text: string;
	placeholders: WikilinkPlaceholder[];
} {
	const placeholders: WikilinkPlaceholder[] = [];
	let index = 0;

	let text = markdown;

	// Embedded images: ![[path]]
	text = text.replace(/!\[\[([^\]|]+)\]\]/g, (match, path) => {
		const trimmed = path.trim();
		const placeholder = makeWikilinkPlaceholder(index++);
		placeholders.push({ placeholder, type: 'image', path: trimmed, display: trimmed });
		return placeholder;
	});

	// Wikilinks with display text: [[path|display]]
	text = text.replace(/\[\[([^\]|]+)\|([^\]]+)\]\]/g, (match, path, display) => {
		const placeholder = makeWikilinkPlaceholder(index++);
		placeholders.push({
			placeholder,
			type: 'link',
			path: path.trim(),
			display: display.trim()
		});
		return placeholder;
	});

	// Wikilinks without display text: [[path]]
	text = text.replace(/\[\[([^\]|]+)\]\]/g, (match, path) => {
		const trimmed = path.trim();
		const placeholder = makeWikilinkPlaceholder(index++);
		placeholders.push({ placeholder, type: 'link', path: trimmed, display: trimmed });
		return placeholder;
	});

	return { text, placeholders };
}

function restoreWikilinkPlaceholders(html: string, placeholders: WikilinkPlaceholder[]): string {
	let result = html;
	for (const p of placeholders) {
		if (p.type === 'link') {
			result = result.replace(
				p.placeholder,
				`<a data-wikilink="${escapeHtmlAttribute(p.path)}">${escapeHtmlAttribute(p.display)}</a>`
			);
		} else {
			result = result.replace(
				p.placeholder,
				`<img data-wikilink-src="${escapeHtmlAttribute(p.path)}" alt="${escapeHtmlAttribute(p.display)}" />`
			);
		}
	}
	return result;
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
		const wikilinkResult = preprocessWikilinks(markdown);
		const preprocessed = preprocessMarkdownTables(wikilinkResult.text);
		const editor = new Editor({
			extensions: getEditorExtensions(),
			content: preprocessed,
			editable: false
		});

		let html = editor.getHTML();
		editor.destroy();

		html = restoreWikilinkPlaceholders(html, wikilinkResult.placeholders);

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
		interface MarkdownStorage {
			markdown?: { getMarkdown: () => string };
		}
		const storage = editor.storage as MarkdownStorage;
		const markdownStorage = storage?.markdown;

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
	onDocumentUpdate?: (editor: Editor) => void;
	onSelectionUpdate?: () => void;
	onCreate?: () => void;
	/** Additional ProseMirror editor props (e.g. handleDoubleClickOn) */
	editorProps?: EditorProps;
	/** Optional Yjs document for collaborative editing */
	ydoc?: import('yjs').Doc;
}

/**
 * Creates a Tiptap editor with standard RustShare extensions.
 * The editor accepts Markdown input and can serialize back to Markdown.
 */
export function createRichEditor(options: CreateEditorOptions): Editor {
	const preprocessed = preprocessMarkdownTables(options.content || '');

	const editor = new Editor({
		element: options.element,
		extensions: getEditorExtensions({ ydoc: options.ydoc }),
		content: preprocessed,
		editable: options.editable ?? true,
		editorProps: {
			attributes: {
				class: 'rich-editor-content'
			},
			...options.editorProps
		},
		onCreate: () => {
			options.onCreate?.();
		},
		onUpdate: ({ editor: e }) => {
			if (options.onDocumentUpdate) {
				options.onDocumentUpdate(e);
			} else if (options.onUpdate) {
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
