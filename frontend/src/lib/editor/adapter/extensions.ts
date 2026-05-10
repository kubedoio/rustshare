/**
 * Shared Tiptap extension configuration.
 * All editor instances (viewer and editor) use the same extensions
 * to ensure consistent rendering.
 */

import StarterKit from '@tiptap/starter-kit';
import Link from '@tiptap/extension-link';
import Underline from '@tiptap/extension-underline';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import { Table } from '@tiptap/extension-table';
import TableRow from '@tiptap/extension-table-row';
import TableCell from '@tiptap/extension-table-cell';
import TableHeader from '@tiptap/extension-table-header';
import Placeholder from '@tiptap/extension-placeholder';
import { Markdown } from 'tiptap-markdown';

/**
 * Returns the standard set of Tiptap extensions for the RustShare editor.
 * Both the viewer and editor must use identical extensions.
 */
export function getEditorExtensions(options?: { placeholder?: string }) {
	return [
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
}
