import { describe, it, expect } from 'vitest';
import { sanitizeHtml, isSafeFilename, isSafeUrl } from './security';

describe('Sanitization', () => {
	it('removes script tags', () => {
		const unsafe = '<p>Hello</p><script>alert("xss")</script>';
		const safe = sanitizeHtml(unsafe);
		expect(safe).toBe('<p>Hello</p>');
		expect(safe).not.toContain('<script>');
	});

	it('removes event handlers', () => {
		const unsafe = '<img src="x" onerror="alert(1)">';
		const safe = sanitizeHtml(unsafe);
		expect(safe).toBe('<img src="x">');
		expect(safe).not.toContain('onerror');
	});

	it('removes remote image sources', () => {
		const unsafe = '<p>Hi</p><img alt="tracker" src="https://tracker.example/pixel.gif">';
		const safe = sanitizeHtml(unsafe);
		expect(safe).toBe('<p>Hi</p><img alt="tracker">');
		expect(safe).not.toContain('https://tracker.example');
	});

	it('removes scheme-relative image sources', () => {
		const unsafe = '<p>Hi</p><img alt="tracker" src="//tracker.example/pixel.gif">';
		const safe = sanitizeHtml(unsafe);
		expect(safe).toBe('<p>Hi</p><img alt="tracker">');
		expect(safe).not.toContain('tracker.example');
	});

	it('removes image sources with uppercase schemes', () => {
		const unsafe = '<p>Hi</p><img alt="tracker" src="HTTPS://tracker.example/pixel.gif">';
		const safe = sanitizeHtml(unsafe);
		expect(safe).toBe('<p>Hi</p><img alt="tracker">');
		expect(safe).not.toContain('tracker.example');
	});

	it('removes style attributes', () => {
		const unsafe = '<p style="color: red; position: fixed; top: 0">Text</p>';
		const safe = sanitizeHtml(unsafe);
		expect(safe).toBe('<p>Text</p>');
		expect(safe).not.toContain('style=');
	});

	it('removes javascript: links', () => {
		const unsafe = '<a href="javascript:alert(1)">Click me</a>';
		const safe = sanitizeHtml(unsafe);
		expect(safe).toBe('<a>Click me</a>');
	});

	it('allows safe HTML', () => {
		const html = '<h1>Title</h1><p>Text <strong>bold</strong></p><ul><li>Item</li></ul>';
		const safe = sanitizeHtml(html);
		expect(safe).toBe(html);
	});

	it('preserves checkbox inputs for task lists', () => {
		const html = '<ul><li><input type="checkbox" checked disabled> Task</li></ul>';
		const safe = sanitizeHtml(html);
		expect(safe).toContain('<input');
		expect(safe).toContain('type="checkbox"');
		expect(safe).toContain('checked');
		expect(safe).toContain('disabled');
	});

	it('preserves strikethrough <s> tags', () => {
		const html = '<p>Text with <s>strikethrough</s> word</p>';
		const safe = sanitizeHtml(html);
		expect(safe).toContain('<s>strikethrough</s>');
	});
});

describe('Filename Safety', () => {
	it('rejects path traversal', () => {
		expect(isSafeFilename('../etc/passwd')).toBe(false);
		expect(isSafeFilename('..\\windows\\system32')).toBe(false);
		expect(isSafeFilename('attachments/file.txt')).toBe(false);
	});

	it('rejects hidden files', () => {
		expect(isSafeFilename('.env')).toBe(false);
		expect(isSafeFilename('.git/config')).toBe(false);
	});

	it('rejects forbidden metadata files', () => {
		expect(isSafeFilename('.rustshare.json')).toBe(false);
		expect(isSafeFilename('index.editor.json')).toBe(false);
	});

	it('allows safe filenames', () => {
		expect(isSafeFilename('document.md')).toBe(true);
		expect(isSafeFilename('image_2026.png')).toBe(true);
		expect(isSafeFilename('archive-v1.zip')).toBe(true);
	});
});

describe('URL Safety', () => {
	it('allows http and https', () => {
		expect(isSafeUrl('http://example.com')).toBe(true);
		expect(isSafeUrl('https://example.com/path')).toBe(true);
	});

	it('allows relative paths', () => {
		expect(isSafeUrl('./attachments/img.png')).toBe(true);
		expect(isSafeUrl('/api/v1/files')).toBe(true);
		expect(isSafeUrl('#anchor')).toBe(true);
	});

	it('rejects dangerous protocols', () => {
		expect(isSafeUrl('javascript:alert(1)')).toBe(false);
		expect(isSafeUrl('data:text/html,<html>')).toBe(false);
		expect(isSafeUrl('file:///etc/passwd')).toBe(false);
	});
});
