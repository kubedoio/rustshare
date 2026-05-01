/**
 * Slash Command definitions for the Rich Markdown Editor.
 * Each command has a label, description, icon key, search keywords,
 * and an action that operates on a Tiptap editor.
 */

import type { Editor } from '@tiptap/core';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface SlashCommand {
	id: string;
	label: string;
	description: string;
	icon: string;
	keywords: string[];
	group: 'basic' | 'list' | 'block' | 'media';
	/** If true, only shown when the host provides an attachment handler */
	requiresAttachmentHandler?: boolean;
	/** Execute the command on the editor, returns true if successful */
	action: (editor: Editor) => boolean;
}

// ---------------------------------------------------------------------------
// Command Definitions
// ---------------------------------------------------------------------------

export const SLASH_COMMANDS: SlashCommand[] = [
	{
		id: 'text',
		label: 'Text',
		description: 'Plain text paragraph',
		icon: 'type',
		keywords: ['text', 'paragraph', 'plain'],
		group: 'basic',
		action: (editor) => editor.chain().focus().setParagraph().run()
	},
	{
		id: 'heading1',
		label: 'Heading 1',
		description: 'Large section heading',
		icon: 'heading-1',
		keywords: ['heading', 'h1', 'title', 'large'],
		group: 'basic',
		action: (editor) => editor.chain().focus().toggleHeading({ level: 1 }).run()
	},
	{
		id: 'heading2',
		label: 'Heading 2',
		description: 'Medium section heading',
		icon: 'heading-2',
		keywords: ['heading', 'h2', 'subtitle', 'medium'],
		group: 'basic',
		action: (editor) => editor.chain().focus().toggleHeading({ level: 2 }).run()
	},
	{
		id: 'heading3',
		label: 'Heading 3',
		description: 'Small section heading',
		icon: 'heading-3',
		keywords: ['heading', 'h3', 'small'],
		group: 'basic',
		action: (editor) => editor.chain().focus().toggleHeading({ level: 3 }).run()
	},
	{
		id: 'bullet-list',
		label: 'Bullet List',
		description: 'Unordered bullet list',
		icon: 'list',
		keywords: ['bullet', 'list', 'unordered', 'ul'],
		group: 'list',
		action: (editor) => editor.chain().focus().toggleBulletList().run()
	},
	{
		id: 'numbered-list',
		label: 'Numbered List',
		description: 'Ordered numbered list',
		icon: 'list-ordered',
		keywords: ['numbered', 'list', 'ordered', 'ol'],
		group: 'list',
		action: (editor) => editor.chain().focus().toggleOrderedList().run()
	},
	{
		id: 'task-list',
		label: 'Task List',
		description: 'Checklist with checkboxes',
		icon: 'list-checks',
		keywords: ['task', 'todo', 'checklist', 'checkbox'],
		group: 'list',
		action: (editor) => editor.chain().focus().toggleTaskList().run()
	},
	{
		id: 'blockquote',
		label: 'Quote',
		description: 'Block quotation',
		icon: 'quote',
		keywords: ['quote', 'blockquote', 'citation'],
		group: 'block',
		action: (editor) => editor.chain().focus().toggleBlockquote().run()
	},
	{
		id: 'code-block',
		label: 'Code Block',
		description: 'Fenced code block',
		icon: 'braces',
		keywords: ['code', 'block', 'fenced', 'snippet', 'pre'],
		group: 'block',
		action: (editor) => editor.chain().focus().toggleCodeBlock().run()
	},
	{
		id: 'table',
		label: 'Table',
		description: 'Insert a 3×3 table',
		icon: 'table',
		keywords: ['table', 'grid', 'columns', 'rows'],
		group: 'block',
		action: (editor) =>
			editor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()
	},
	{
		id: 'divider',
		label: 'Divider',
		description: 'Horizontal rule separator',
		icon: 'minus',
		keywords: ['divider', 'horizontal', 'rule', 'separator', 'hr', 'line'],
		group: 'block',
		action: (editor) => editor.chain().focus().setHorizontalRule().run()
	},
	{
		id: 'image',
		label: 'Image',
		description: 'Upload and embed an image',
		icon: 'image',
		keywords: ['image', 'picture', 'photo', 'upload'],
		group: 'media',
		requiresAttachmentHandler: true,
		action: () => false // Handled by event dispatch
	},
	{
		id: 'file-attachment',
		label: 'File Attachment',
		description: 'Attach a file to this document',
		icon: 'paperclip',
		keywords: ['file', 'attachment', 'upload', 'attach'],
		group: 'media',
		requiresAttachmentHandler: true,
		action: () => false // Handled by event dispatch
	}
];

// ---------------------------------------------------------------------------
// Filtering
// ---------------------------------------------------------------------------

/**
 * Filters slash commands by query string. Matches against label and keywords.
 * Optionally filters out commands that require an attachment handler.
 */
export function filterSlashCommands(
	query: string,
	options?: { hasAttachmentHandler?: boolean }
): SlashCommand[] {
	const q = query.toLowerCase().trim();
	const hasHandler = options?.hasAttachmentHandler ?? false;

	return SLASH_COMMANDS.filter((cmd) => {
		// Hide commands requiring attachment handler if not available
		if (cmd.requiresAttachmentHandler && !hasHandler) return false;

		// Empty query matches all
		if (!q) return true;

		// Match against label and keywords
		if (cmd.label.toLowerCase().includes(q)) return true;
		return cmd.keywords.some((kw) => kw.includes(q));
	});
}

/**
 * Gets a slash command by its ID.
 */
export function getSlashCommandById(id: string): SlashCommand | undefined {
	return SLASH_COMMANDS.find((cmd) => cmd.id === id);
}
