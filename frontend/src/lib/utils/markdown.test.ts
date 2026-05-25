import { describe, it, expect } from 'vitest';
import { renderMarkdown } from './markdown';

describe('renderMarkdown', () => {
	it('renders basic markdown successfully', () => {
		const input = '# Heading 1\n## Heading 2\n[link](https://example.com)\n**bold**';
		const html = renderMarkdown(input);
		expect(html).toContain('<h1>Heading 1</h1>');
		expect(html).toContain('<h2>Heading 2</h2>');
		expect(html).toContain('href="https://example.com"');
		expect(html).toContain('<strong>bold</strong>');
	});

	it('preserves target="_blank" and rel on links after sanitization', () => {
		const input = '[link](https://example.com)';
		const html = renderMarkdown(input);
		expect(html).toContain('target="_blank"');
		expect(html).toContain('rel="noopener noreferrer"');
	});

	it('prevents javascript: URIs in links', () => {
		const input = '[click](javascript:alert(1))';
		const html = renderMarkdown(input);
		expect(html).not.toContain('javascript:');
	});

	it('prevents javascript: URIs in links with mixed case', () => {
		const input = '[click](JAVAScript:alert(1))';
		const html = renderMarkdown(input);
		expect(html).not.toContain('JAVAScript:');
	});

	it('prevents onclick and other event handlers on elements', () => {
		// Since we are simply rendering markdown, testing standard link creation
		// However, if the user could somehow inject an onclick (which our regexes try to limit, but DOMPurify cleans up)
		const input = '[click](" onclick="alert(1)"))';
		const html = renderMarkdown(input);
		expect(html).not.toContain('onclick="alert(1)"');
	});

	it('strips <script> tags entirely', () => {
		const input = '<script>alert(1)</script>';
		const html = renderMarkdown(input);
		expect(html).not.toContain('<script>');
	});

	it('sanitizes data URI links containing malicious HTML', () => {
		const input = '[click](data:text/html,<script>alert(1)</script>)';
		const html = renderMarkdown(input);
		// Should not contain an executable script payload
		expect(html).not.toContain('<script>');
	});
});

describe('table rendering', () => {
	it('renders a GFM table as HTML', () => {
		const input = '| Name | Value |\n|------|-------|\n| Foo  | 42    |';
		const html = renderMarkdown(input);
		expect(html).toContain('<table');
		expect(html).toContain('Name</th>');
		expect(html).toContain('Foo</td>');
		expect(html).toContain('42</td>');
	});

	it('does not render fake tables (missing separator)', () => {
		const input = '| A | B |\n| C | D |';
		const html = renderMarkdown(input);
		expect(html).not.toContain('<table');
	});

	it('renders a table without outer pipes', () => {
		const input = 'Name | Value\n-----|------\nFoo  | 42';
		const html = renderMarkdown(input);
		expect(html).toContain('<table');
		expect(html).toContain('Name</th>');
	});
});

describe('task list rendering', () => {
	it('renders unchecked task items', () => {
		const input = '- [ ] Buy groceries\n- [ ] Walk the dog';
		const html = renderMarkdown(input);
		expect(html).toContain('<ul class="list-none my-2 pl-0">');
		expect(html).toContain('<input type="checkbox" disabled="" class="');
		expect(html).toContain('Buy groceries');
		expect(html).toContain('Walk the dog');
		expect(html).not.toContain('checked');
	});

	it('renders checked task items', () => {
		const input = '- [x] Buy groceries\n- [X] Walk the dog';
		const html = renderMarkdown(input);
		expect(html).toContain('<input type="checkbox" disabled="" checked="" class="');
		expect(html).toContain('Buy groceries');
		expect(html).toContain('Walk the dog');
	});

	it('does not treat regular lists as task lists', () => {
		const input = '- Regular item\n- Another item';
		const html = renderMarkdown(input);
		expect(html).toContain('<ul class="list-disc my-2 pl-5">');
		expect(html).not.toContain('<input');
	});
});
