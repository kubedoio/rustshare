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
