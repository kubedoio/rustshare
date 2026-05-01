/**
 * Rich Markdown Editor — Security & Sanitization
 * 
 * Enforces strict HTML sanitization for rendered Markdown content.
 */

import DOMPurify from 'isomorphic-dompurify';

/**
 * Strict configuration for DOMPurify to prevent XSS.
 */
const SANITIZE_CONFIG: DOMPurify.Config = {
	ALLOWED_TAGS: [
		'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 
		'p', 'br', 'hr', 
		'b', 'i', 'strong', 'em', 'strike', 'u', 'code', 'pre',
		'ul', 'ol', 'li',
		'blockquote',
		'a', 'img',
		'table', 'thead', 'tbody', 'tr', 'th', 'td'
	],
	ALLOWED_ATTR: [
		'href', 'src', 'alt', 'title', 'class', 'id', 'target', 'rel',
		'checked', 'type', 'disabled'
	],
	// Ensure we block dangerous URIs by default
	ADD_ATTR: ['target', 'rel'],
	FORBID_TAGS: ['script', 'style', 'iframe', 'object', 'embed', 'form', 'button', 'input'],
	FORBID_ATTR: ['onerror', 'onclick', 'onmouseover', 'onkeydown', 'onload', 'style']
};

/**
 * Sanitizes an HTML string using DOMPurify.
 */
export function sanitizeHtml(html: string): string {
	return DOMPurify.sanitize(html, SANITIZE_CONFIG) as string;
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
