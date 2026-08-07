/**
 * Shared Tiptap extension configuration.
 * All editor instances (viewer and editor) use the same extensions
 * to ensure consistent rendering.
 */

import type { Extensions } from '@tiptap/core';
import { getHTMLFromFragment } from '@tiptap/core';
import { Fragment } from '@tiptap/pm/model';
import StarterKit from '@tiptap/starter-kit';
import Link from '@tiptap/extension-link';
import Underline from '@tiptap/extension-underline';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import { Table as BaseTable } from '@tiptap/extension-table';
import TableRow from '@tiptap/extension-table-row';
import TableCell from '@tiptap/extension-table-cell';
import TableHeader from '@tiptap/extension-table-header';
import Placeholder from '@tiptap/extension-placeholder';
import Image from '@tiptap/extension-image';
import { Markdown } from 'tiptap-markdown';

export interface EditorExtensionsOptions {
	placeholder?: string;
}

function tableChildNodes(node: any): any[] {
	return node?.content?.content ?? [];
}

function tableCellHasSpan(cell: any): boolean {
	return cell.attrs.colspan > 1 || cell.attrs.rowspan > 1;
}

function isMarkdownSerializableTable(node: any): boolean {
	const rows = tableChildNodes(node);
	const firstRow = rows[0];
	const bodyRows = rows.slice(1);

	if (
		tableChildNodes(firstRow).some(
			(cell: any) =>
				cell.type.name !== 'tableHeader' || tableCellHasSpan(cell) || cell.childCount > 1
		)
	) {
		return false;
	}

	if (
		bodyRows.some((row: any) =>
			tableChildNodes(row).some(
				(cell: any) =>
					cell.type.name === 'tableHeader' || tableCellHasSpan(cell) || cell.childCount > 1
			)
		)
	) {
		return false;
	}

	return true;
}

/**
 * Custom Table extension that overrides tiptap-markdown's serializer.
 * Cell content is rendered with `renderInline(..., false)` so characters
 * that look like block markers (e.g. `1.`, `-`, `#`) at the start of a
 * cell are not unnecessarily escaped.
 */
const Table = BaseTable.extend({
	addStorage() {
		return {
			markdown: {
				serialize(state: any, node: any) {
					if (!isMarkdownSerializableTable(node)) {
						const html = getHTMLFromFragment(Fragment.from(node), node.type.schema);
						state.write(html);
						state.closeBlock(node);
						return;
					}

					state.inTable = true;
					node.forEach((row: any, _rowPos: any, i: number) => {
						state.write('| ');
						row.forEach((col: any, _colPos: any, j: number) => {
							if (j) {
								state.write(' | ');
							}
							const cellContent = col.firstChild;
							if (cellContent && cellContent.textContent.trim()) {
								state.renderInline(cellContent, false);
							}
						});
						state.write(' |');
						state.ensureNewLine();
						if (!i) {
							const delimiterRow = Array.from({ length: row.childCount })
								.map(() => '---')
								.join(' | ');
							state.write(`| ${delimiterRow} |`);
							state.ensureNewLine();
						}
					});
					state.closeBlock(node);
					state.inTable = false;
				},
				parse: {
					// handled by markdown-it
				}
			}
		};
	}
});

/**
 * Returns the standard set of Tiptap extensions for the RustShare editor.
 * Both the viewer and editor must use identical extensions.
 */
export function getEditorExtensions(options?: EditorExtensionsOptions): Extensions {
	const extensions: Extensions = [
		StarterKit.configure({
			heading: { levels: [1, 2, 3] },
			codeBlock: { HTMLAttributes: { class: 'editor-code-block' } },
			code: { HTMLAttributes: { class: 'editor-inline-code' } },
			blockquote: { HTMLAttributes: { class: 'editor-blockquote' } },
			bulletList: { HTMLAttributes: { class: 'editor-bullet-list' } },
			orderedList: { HTMLAttributes: { class: 'editor-ordered-list' } },
			horizontalRule: { HTMLAttributes: { class: 'editor-hr' } },
			link: false,
			underline: false
		}),
		Link.configure({
			openOnClick: false,
			HTMLAttributes: { class: 'editor-link', rel: 'noopener noreferrer' }
		}),
		Underline,
		TaskList.configure({
			HTMLAttributes: { class: 'editor-task-list' }
		}),
		TaskItem.configure({
			nested: true,
			HTMLAttributes: { class: 'editor-task-item' }
		}),
		Table.configure({
			resizable: false,
			HTMLAttributes: { class: 'editor-table' }
		}),
		TableRow,
		TableCell,
		TableHeader,
		Image.configure({
			allowBase64: true,
			HTMLAttributes: { class: 'editor-image max-w-full rounded-lg my-2' }
		}),
		Placeholder.configure({
			placeholder: options?.placeholder || "Type '/' for commands…"
		}),
		Markdown.configure({
			html: true, // Needed for underline (<u>) serialization
			tightLists: true,
			bulletListMarker: '-',
			breaks: false,
			transformPastedText: true,
			transformCopiedText: true
		})
	];

	return extensions;
}
