import { describe, it, expect, afterEach } from 'vitest';
import { Editor } from '@tiptap/core';
import { markdownToHtml, editorToMarkdown, createRichEditor, preprocessMarkdownTables } from './markdown';
import { getEditorExtensions } from './extensions';

// ---------------------------------------------------------------------------
// Helper: round-trip Markdown through Tiptap
// ---------------------------------------------------------------------------

function roundTrip(markdown: string): string {
	const editor = new Editor({
		extensions: getEditorExtensions(),
		content: markdown
	});
	const result = editorToMarkdown(editor);
	editor.destroy();
	return result;
}

function normalize(md: string): string {
	return md.replace(/\r\n/g, '\n').trim();
}

// ---------------------------------------------------------------------------
// markdownToHtml
// ---------------------------------------------------------------------------

describe('markdownToHtml', () => {
	it('converts simple paragraph', () => {
		const result = markdownToHtml('Hello world');
		expect(result.success).toBe(true);
		expect(result.html).toContain('Hello world');
		expect(result.html).toContain('<p>');
	});

	it('converts headings', () => {
		const result = markdownToHtml('# Title\n\n## Subtitle');
		expect(result.success).toBe(true);
		expect(result.html).toContain('<h1>Title</h1>');
		expect(result.html).toContain('<h2>Subtitle</h2>');
	});

	it('converts bold and italic', () => {
		const result = markdownToHtml('**bold** and *italic*');
		expect(result.success).toBe(true);
		expect(result.html).toContain('<strong>bold</strong>');
		expect(result.html).toContain('<em>italic</em>');
	});

	it('returns empty for empty input', () => {
		expect(markdownToHtml('').success).toBe(true);
		expect(markdownToHtml('').html).toBe('');
	});

	it('returns empty for whitespace-only input', () => {
		expect(markdownToHtml('   ').success).toBe(true);
		expect(markdownToHtml('   ').html).toBe('');
	});
});

// ---------------------------------------------------------------------------
// Markdown round-trip — heading
// ---------------------------------------------------------------------------

describe('Markdown round-trip: headings', () => {
	it('H1 survives round-trip', () => {
		const result = normalize(roundTrip('# Heading 1'));
		expect(result).toBe('# Heading 1');
	});

	it('H2 survives round-trip', () => {
		const result = normalize(roundTrip('## Heading 2'));
		expect(result).toBe('## Heading 2');
	});

	it('H3 survives round-trip', () => {
		const result = normalize(roundTrip('### Heading 3'));
		expect(result).toBe('### Heading 3');
	});
});

// ---------------------------------------------------------------------------
// Markdown round-trip — inline formatting
// ---------------------------------------------------------------------------

describe('Markdown round-trip: inline formatting', () => {
	it('bold survives round-trip', () => {
		const result = normalize(roundTrip('**bold text**'));
		expect(result).toContain('**bold text**');
	});

	it('italic survives round-trip', () => {
		const result = normalize(roundTrip('*italic text*'));
		expect(result).toContain('*italic text*');
	});

	it('inline code survives round-trip', () => {
		const result = normalize(roundTrip('use `console.log()`'));
		expect(result).toContain('`console.log()`');
	});
});

// ---------------------------------------------------------------------------
// Markdown round-trip — lists
// ---------------------------------------------------------------------------

describe('Markdown round-trip: lists', () => {
	it('bullet list survives round-trip', () => {
		const input = '- Item 1\n- Item 2\n- Item 3';
		const result = normalize(roundTrip(input));
		expect(result).toContain('Item 1');
		expect(result).toContain('Item 2');
		expect(result).toContain('Item 3');
		// Should use list markers (- or *)
		expect(result).toMatch(/^[-*]\s/m);
	});

	it('numbered list survives round-trip', () => {
		const input = '1. First\n2. Second\n3. Third';
		const result = normalize(roundTrip(input));
		expect(result).toContain('First');
		expect(result).toContain('Second');
		expect(result).toContain('Third');
		expect(result).toMatch(/^\d+\.\s/m);
	});
});

// ---------------------------------------------------------------------------
// Markdown round-trip — block elements
// ---------------------------------------------------------------------------

describe('Markdown round-trip: block elements', () => {
	it('blockquote survives round-trip', () => {
		const result = normalize(roundTrip('> Quoted text'));
		expect(result).toContain('> Quoted text');
	});

	it('code block survives round-trip', () => {
		const input = '```\nconst x = 1;\n```';
		const result = normalize(roundTrip(input));
		expect(result).toContain('const x = 1;');
		expect(result).toContain('```');
	});

	it('horizontal rule survives round-trip', () => {
		const result = normalize(roundTrip('---'));
		expect(result).toMatch(/---/);
	});
});

// ---------------------------------------------------------------------------
// Markdown round-trip — links
// ---------------------------------------------------------------------------

describe('Markdown round-trip: links', () => {
	it('link survives round-trip', () => {
		const result = normalize(roundTrip('[Example](https://example.com)'));
		expect(result).toContain('[Example](https://example.com)');
	});
});

// ---------------------------------------------------------------------------
// createRichEditor
// ---------------------------------------------------------------------------

describe('createRichEditor', () => {
	let editor: Editor | null = null;

	afterEach(() => {
		editor?.destroy();
		editor = null;
	});

	it('creates an editor instance', () => {
		editor = createRichEditor({ content: '# Test' });
		expect(editor).toBeTruthy();
		expect(editor.isEditable).toBe(true);
	});

	it('creates a read-only editor', () => {
		editor = createRichEditor({ content: 'Read only', editable: false });
		expect(editor.isEditable).toBe(false);
	});

	it('loads Markdown content', () => {
		editor = createRichEditor({ content: '**Bold** and *italic*' });
		const html = editor.getHTML();
		expect(html).toContain('<strong>Bold</strong>');
		expect(html).toContain('<em>italic</em>');
	});

	it('fires onUpdate callback', () => {
		let lastMarkdown = '';
		editor = createRichEditor({
			content: 'Initial',
			onUpdate: (md) => {
				lastMarkdown = md;
			}
		});

		// Simulate an edit
		editor.commands.setContent('Updated content');
		expect(lastMarkdown).toContain('Updated content');
	});
});

// ---------------------------------------------------------------------------
// editorToMarkdown
// ---------------------------------------------------------------------------

describe('editorToMarkdown', () => {
	it('extracts Markdown from editor', () => {
		const editor = createRichEditor({ content: '# Hello\n\nWorld' });
		const md = normalize(editorToMarkdown(editor));
		expect(md).toContain('# Hello');
		expect(md).toContain('World');
		editor.destroy();
	});

	it('returns empty for empty editor', () => {
		const editor = createRichEditor({ content: '' });
		const md = editorToMarkdown(editor);
		// Empty editor may return empty or a single newline
		expect(md.trim()).toBe('');
		editor.destroy();
	});
});

// ---------------------------------------------------------------------------
// Markdown round-trip — new formatting (Prompt 03)
// ---------------------------------------------------------------------------

describe('Markdown round-trip: underline', () => {
	it('underline renders and serializes as HTML', () => {
		// Underline has no standard Markdown syntax; uses <u> tag
		const input = '<u>underlined</u>';
		const result = markdownToHtml(input);
		expect(result.success).toBe(true);
		expect(result.html).toContain('underlined');
	});
});

describe('Markdown round-trip: task lists', () => {
	it('unchecked task list survives round-trip', () => {
		const input = '- [ ] Buy groceries\n- [ ] Walk the dog';
		const result = normalize(roundTrip(input));
		expect(result).toContain('Buy groceries');
		expect(result).toContain('Walk the dog');
	});

	it('checked task list survives round-trip', () => {
		const input = '- [x] Done task\n- [ ] Pending task';
		const result = normalize(roundTrip(input));
		expect(result).toContain('Done task');
		expect(result).toContain('Pending task');
	});
});

describe('Markdown round-trip: tables', () => {
	it('table created via editor command produces Markdown', () => {
		const editor = createRichEditor({ content: '' });
		editor.chain().focus().insertTable({ rows: 2, cols: 2, withHeaderRow: true }).run();
		const md = editorToMarkdown(editor);
		// Should contain table separators
		expect(md).toContain('|');
		editor.destroy();
	});
});

describe('Markdown round-trip: mixed formatting', () => {
	it('bold + italic combo survives', () => {
		const result = normalize(roundTrip('***bold and italic***'));
		expect(result).toMatch(/\*{1,3}bold and italic\*{1,3}/);
	});

	it('heading with inline code survives', () => {
		const result = normalize(roundTrip('## Using `fetch()` API'));
		expect(result).toContain('## ');
		expect(result).toContain('`fetch()`');
	});

	it('blockquote with bold survives', () => {
		const result = normalize(roundTrip('> **Important** note'));
		expect(result).toContain('>');
		expect(result).toContain('**Important**');
	});
});


describe('preprocessMarkdownTables', () => {
	it('converts a simple GFM table to HTML', () => {
		const input = '| A | B |\n|---|---|\n| 1 | 2 |';
		const result = preprocessMarkdownTables(input);
		expect(result).toContain('<table');
		expect(result).toContain('<th>A</th>');
		expect(result).toContain('<td>1</td>');
		expect(result).toContain('</table>');
	});

	it('ignores non-table | patterns', () => {
		const input = 'This | that | other';
		const result = preprocessMarkdownTables(input);
		expect(result).toBe(input);
	});

	it('does not convert tables inside fenced code blocks', () => {
		const input = '```\n| A | B |\n|---|---|\n| 1 | 2 |\n```';
		const result = preprocessMarkdownTables(input);
		expect(result).not.toContain('<table');
		expect(result).toBe(input);
	});

	it('does not convert tables inside inline code', () => {
		const input = 'Use `| pipe |` for tables';
		const result = preprocessMarkdownTables(input);
		expect(result).not.toContain('<table');
		expect(result).toBe(input);
	});

	it('converts a table without outer pipes', () => {
		const input = 'A | B\n---|---\n1 | 2';
		const result = preprocessMarkdownTables(input);
		expect(result).toContain('<table');
		expect(result).toContain('<th>A</th>');
	});

	it('passes through markdown with no tables unchanged', () => {
		const input = '# Hello\n\nSome **bold** text.';
		expect(preprocessMarkdownTables(input)).toBe(input);
	});
});


describe('base64 image round-trip', () => {
	it('preserves data URL in markdown', () => {
		const input = '![Sketch](data:image/png;base64,iVBORw0KGgo=)';
		const editor = createRichEditor({ content: input });
		const md = editorToMarkdown(editor);

		expect(md).toContain('data:image/png;base64');
		editor.destroy();
	});

	it('renders data URL image in HTML', () => {
		const input = '![Sketch](data:image/png;base64,iVBORw0KGgo=)';
		const result = markdownToHtml(input);

		expect(result.success).toBe(true);
		expect(result.html).toContain('data:image/png;base64');
	});
});
