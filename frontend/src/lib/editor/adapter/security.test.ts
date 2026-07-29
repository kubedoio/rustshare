import { describe, it, expect } from 'vitest';
import { sanitizeHtml, sanitizeEmailHtml, isSafeFilename, isSafeUrl } from './security';

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

describe('Email Sanitization (remote images)', () => {
	it('blocks remote images and moves the URL to a marker attribute', () => {
		const raw = '<p>Hi</p><img alt="tracker" src="https://tracker.example/pixel.gif">';
		const { html, blockedRemoteImages } = sanitizeEmailHtml(raw);
		expect(blockedRemoteImages).toBe(1);
		expect(html).toContain('data-rustshare-blocked-src="https://tracker.example/pixel.gif"');
		expect(html).not.toContain(' src="https://tracker.example');
	});

	it('counts multiple blocked remote images only', () => {
		const raw =
			'<img src="https://a.example/1.png"><img src="//b.example/2.png"><img src="cid:inline"><img src="/local.png">';
		const { blockedRemoteImages } = sanitizeEmailHtml(raw);
		expect(blockedRemoteImages).toBe(2);
	});

	it('blocks remote images with uppercase schemes', () => {
		const raw = '<img src="HTTPS://tracker.example/pixel.gif">';
		const { html, blockedRemoteImages } = sanitizeEmailHtml(raw);
		expect(blockedRemoteImages).toBe(1);
		expect(html).not.toContain(' src="HTTPS://tracker.example');
	});

	it('keeps remote images when explicitly allowed', () => {
		const raw = '<img alt="t" src="https://tracker.example/pixel.gif">';
		const { html, blockedRemoteImages } = sanitizeEmailHtml(raw, { allowRemoteImages: true });
		expect(blockedRemoteImages).toBe(0);
		expect(html).toContain('src="https://tracker.example/pixel.gif"');
	});

	it('strips srcset in blocked mode', () => {
		const raw =
			'<img src="https://cdn.example/a.png" srcset="https://cdn.example/a.png 1x, https://cdn.example/a@2x.png 2x">';
		const { html, blockedRemoteImages } = sanitizeEmailHtml(raw);
		expect(blockedRemoteImages).toBe(1);
		expect(html).not.toContain('srcset');
	});

	it('keeps remote srcset only when explicitly allowed', () => {
		const raw = '<img src="https://cdn.example/a.png" srcset="https://cdn.example/a.png 1x">';
		const allowed = sanitizeEmailHtml(raw, { allowRemoteImages: true });
		expect(allowed.html).toContain('srcset="https://cdn.example/a.png 1x"');
	});

	it('blocks tracking-pixel-sized remote images', () => {
		const raw = '<img src="https://tracker.example/open.gif" width="1" height="1">';
		const { html, blockedRemoteImages } = sanitizeEmailHtml(raw);
		expect(blockedRemoteImages).toBe(1);
		expect(html).not.toContain(' src="https://tracker.example/open.gif"');
		expect(html).toContain('data-rustshare-blocked-src="https://tracker.example/open.gif"');
	});

	it('handles malformed HTML without crashing or leaking remote loads', () => {
		// Unterminated tag: parser drops it entirely; no remote src may survive.
		const broken = sanitizeEmailHtml('<p>Unclosed<img src="https://tracker.example/x.png"');
		expect(broken.html).not.toContain('tracker.example');
		// Malformed markup around a complete remote image: still blocked.
		const nested = sanitizeEmailHtml('<img src="https://tracker.example/x.png"><div><p>broken');
		expect(nested.blockedRemoteImages).toBe(1);
		expect(nested.html).not.toContain(' src="https://tracker.example');
	});

	it('removes scripts and event handlers in blocked mode', () => {
		const raw = '<img src="https://t.example/x.png" onerror="alert(1)"><script>alert(1)</script>';
		const { html } = sanitizeEmailHtml(raw);
		expect(html).not.toContain('<script>');
		expect(html).not.toContain('onerror');
	});

	it('removes scripts and event handlers when remote images are allowed', () => {
		const raw = '<img src="https://t.example/x.png" onerror="alert(1)"><script>alert(1)</script>';
		const { html } = sanitizeEmailHtml(raw, { allowRemoteImages: true });
		expect(html).not.toContain('<script>');
		expect(html).not.toContain('onerror');
		expect(html).toContain('src="https://t.example/x.png"');
	});

	it('removes javascript: URLs in both modes', () => {
		const raw = '<a href="javascript:alert(1)">Click</a>';
		expect(sanitizeEmailHtml(raw).html).not.toContain('javascript:');
		expect(sanitizeEmailHtml(raw, { allowRemoteImages: true }).html).not.toContain('javascript:');
	});

	it('keeps the existing data: URI policy in both modes', () => {
		// DOMPurify's default policy allows data: URIs on <img>; that policy is
		// unchanged and data: sources are not counted as blocked remote images.
		const raw = '<img src="data:image/png;base64,AAAA">';
		const blocked = sanitizeEmailHtml(raw);
		expect(blocked.blockedRemoteImages).toBe(0);
		expect(blocked.html).toContain('src="data:image/png;base64,AAAA"');
		const allowed = sanitizeEmailHtml(raw, { allowRemoteImages: true });
		expect(allowed.html).toContain('src="data:image/png;base64,AAAA"');
	});

	it('passes plain text through unchanged', () => {
		const { html, blockedRemoteImages } = sanitizeEmailHtml('Hello, plain text');
		expect(html).toBe('Hello, plain text');
		expect(blockedRemoteImages).toBe(0);
	});

	it('keeps cid: embedded image sources', () => {
		const raw = '<img src="cid:logo@example.com">';
		const { html, blockedRemoteImages } = sanitizeEmailHtml(raw);
		expect(blockedRemoteImages).toBe(0);
		expect(html).toContain('src="cid:logo@example.com"');
	});

	it('does not block absolute URLs under a local URL prefix (rewritten cid: attachments)', () => {
		const raw =
			'<img src="http://localhost:8080/api/v1/mail/accounts/a/messages/1/attachments/2?folder=INBOX">';
		const { html, blockedRemoteImages } = sanitizeEmailHtml(raw, {
			localUrlPrefixes: ['http://localhost:8080/api/v1']
		});
		expect(blockedRemoteImages).toBe(0);
		expect(html).toContain(
			'src="http://localhost:8080/api/v1/mail/accounts/a/messages/1/attachments/2?folder=INBOX"'
		);
	});

	it('still blocks truly external absolute URLs when local URL prefixes are set', () => {
		const raw = '<img src="https://tracker.example/pixel.gif">';
		const { html, blockedRemoteImages } = sanitizeEmailHtml(raw, {
			localUrlPrefixes: ['http://localhost:8080/api/v1']
		});
		expect(blockedRemoteImages).toBe(1);
		expect(html).toContain('data-rustshare-blocked-src="https://tracker.example/pixel.gif"');
		expect(html).not.toContain(' src="https://tracker.example');
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
