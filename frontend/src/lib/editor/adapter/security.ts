/**
 * Rich Markdown Editor — Security & Sanitization
 *
 * Enforces strict HTML sanitization for rendered Markdown content.
 */

import DOMPurify from 'isomorphic-dompurify';

/**
 * Strict configuration for DOMPurify to prevent XSS.
 */
const SANITIZE_CONFIG = {
	ALLOWED_TAGS: [
		'h1',
		'h2',
		'h3',
		'h4',
		'h5',
		'h6',
		'p',
		'br',
		'hr',
		'b',
		'i',
		'strong',
		'em',
		'strike',
		's',
		'u',
		'code',
		'pre',
		'ul',
		'ol',
		'li',
		'input',
		'label',
		'blockquote',
		'a',
		'img',
		'table',
		'thead',
		'tbody',
		'tr',
		'th',
		'td',
		'span'
	],
	ALLOWED_ATTR: [
		'href',
		'src',
		'alt',
		'title',
		'class',
		'id',
		'target',
		'rel',
		'checked',
		'type',
		'disabled',
		'data-wikilink',
		'data-wikilink-src'
	],
	// Ensure we block dangerous URIs by default
	ADD_ATTR: ['target', 'rel'],
	FORBID_TAGS: ['script', 'style', 'iframe', 'object', 'embed', 'form', 'button'],
	FORBID_ATTR: ['onerror', 'onclick', 'onmouseover', 'onkeydown', 'onload', 'style']
};

/**
 * Sanitizes an HTML string using DOMPurify.
 */
export function sanitizeHtml(html: string): string {
	const sanitized = DOMPurify.sanitize(html, SANITIZE_CONFIG) as unknown as string;
	if (typeof document === 'undefined') {
		return sanitized.replace(/(<img\b[^>]*?)\s+src=(["'])https?:\/\/[^"']*\2/gi, '$1');
	}
	const template = document.createElement('template');
	template.innerHTML = sanitized;
	for (const img of template.content.querySelectorAll('img')) {
		const src = img.getAttribute('src') ?? '';
		if (src.startsWith('http://') || src.startsWith('https://')) {
			img.removeAttribute('src');
		}
	}
	return template.innerHTML;
}

/**
 * Validates a filename to prevent path traversal and ensure it's safe.
 */
export function isSafeFilename(filename: string): boolean {
	if (!filename || filename.startsWith('.') || filename.includes('/') || filename.includes('\\')) {
		return false;
	}

	const forbidden = ['.rustshare.json', '.ds_store', 'thumbs.db', 'index.editor.json'];
	if (forbidden.includes(filename.toLowerCase())) {
		return false;
	}

	return true;
}

/**
 * Checks if a URL is safe for external linking.
 */
export function isSafeUrl(url: string): boolean {
	try {
		const parsed = new URL(url, window.location.origin);
		return ['http:', 'https:', 'mailto:', 'tel:'].includes(parsed.protocol);
	} catch {
		return url.startsWith('./') || url.startsWith('/') || url.startsWith('#');
	}
}
