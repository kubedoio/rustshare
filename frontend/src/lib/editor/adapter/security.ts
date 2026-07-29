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
		return sanitized.replace(/(<img\b[^>]*?)\s+src=(["'])(?:https?:)?\/\/[^"']*\2/gi, '$1');
	}
	const template = document.createElement('template');
	template.innerHTML = sanitized;
	for (const img of template.content.querySelectorAll('img')) {
		const src = (img.getAttribute('src') ?? '').trim().toLowerCase();
		if (src.startsWith('http://') || src.startsWith('https://') || src.startsWith('//')) {
			img.removeAttribute('src');
		}
	}
	return template.innerHTML;
}

/**
 * Attribute used to remember the original URL of a blocked remote image.
 * DOMPurify allows data-* attributes by default, so the marker survives
 * sanitization if the HTML is sanitized again downstream.
 */
export const BLOCKED_REMOTE_IMAGE_ATTR = 'data-rustshare-blocked-src';

const REMOTE_IMAGE_SRC_PATTERN = /^(?:https?:)?\/\//i;

/**
 * Email sanitization config used when the user explicitly asked to load
 * remote images: identical to SANITIZE_CONFIG, but also keeps `srcset`
 * (DOMPurify still strips dangerous URI values from it).
 */
const EMAIL_SANITIZE_CONFIG_ALLOW_REMOTE = {
	...SANITIZE_CONFIG,
	ALLOWED_ATTR: [...SANITIZE_CONFIG.ALLOWED_ATTR, 'srcset']
};

export interface SanitizedEmailHtml {
	html: string;
	/** Number of images whose remote source was blocked. */
	blockedRemoteImages: number;
}

export interface SanitizeEmailHtmlOptions {
	allowRemoteImages?: boolean;
	/**
	 * URL prefixes identifying first-party image sources (e.g. the API base
	 * URL). In blocked mode, images whose `src` starts with one of these
	 * prefixes are treated as local and kept; relative sources are never
	 * blocked either.
	 */
	localUrlPrefixes?: string[];
}

/**
 * Normalizes local URL prefixes (lowercase, no trailing slash) for
 * case-insensitive prefix matching.
 */
function normalizeLocalUrlPrefixes(prefixes: string[] | undefined): string[] {
	if (!prefixes) return [];
	return prefixes
		.map((prefix) => prefix.trim().replace(/\/+$/, '').toLowerCase())
		.filter((prefix) => prefix.length > 0);
}

/**
 * Returns true when `src` points at one of the local URL prefixes, i.e. a
 * first-party URL that must not be treated as a remote image.
 */
function isLocalImageSrc(srcLower: string, localPrefixes: string[]): boolean {
	return localPrefixes.some((prefix) => srcLower === prefix || srcLower.startsWith(`${prefix}/`));
}

/**
 * Sanitizes an email HTML body. By default remote images are blocked: their
 * URL is moved to BLOCKED_REMOTE_IMAGE_ATTR, `src`/`srcset` are removed, and
 * the count of blocked images is reported. Pass `allowRemoteImages: true`
 * only after an explicit per-message user action. Absolute URLs under
 * `localUrlPrefixes` (e.g. the API base URL, used by rewritten cid:
 * attachment sources) are first-party and never blocked.
 */
export function sanitizeEmailHtml(
	html: string,
	opts: SanitizeEmailHtmlOptions = {}
): SanitizedEmailHtml {
	const allowRemoteImages = opts.allowRemoteImages ?? false;
	const localPrefixes = normalizeLocalUrlPrefixes(opts.localUrlPrefixes);
	const config = allowRemoteImages ? EMAIL_SANITIZE_CONFIG_ALLOW_REMOTE : SANITIZE_CONFIG;
	const sanitized = DOMPurify.sanitize(html, config) as unknown as string;
	if (allowRemoteImages) {
		return { html: sanitized, blockedRemoteImages: 0 };
	}
	let blockedRemoteImages = 0;
	if (typeof document === 'undefined') {
		const blocked = sanitized.replace(/<img\b[^>]*>/gi, (tag) => {
			const match = /\ssrc=(["'])((?:https?:)?\/\/[^"']*)\1/i.exec(tag);
			if (!match) return tag;
			if (isLocalImageSrc(match[2].trim().toLowerCase(), localPrefixes)) return tag;
			blockedRemoteImages += 1;
			return tag
				.replace(/\ssrcset=("[^"]*"|'[^']*')/i, '')
				.replace(
					/\ssrc=(["'])(?:https?:)?\/\/[^"']*\1/i,
					` ${BLOCKED_REMOTE_IMAGE_ATTR}="${match[2]}"`
				);
		});
		return { html: blocked, blockedRemoteImages };
	}
	const template = document.createElement('template');
	template.innerHTML = sanitized;
	for (const img of template.content.querySelectorAll('img')) {
		const src = (img.getAttribute('src') ?? '').trim();
		const srcLower = src.toLowerCase();
		if (REMOTE_IMAGE_SRC_PATTERN.test(srcLower) && !isLocalImageSrc(srcLower, localPrefixes)) {
			img.setAttribute(BLOCKED_REMOTE_IMAGE_ATTR, src);
			img.removeAttribute('src');
			img.removeAttribute('srcset');
			blockedRemoteImages += 1;
		}
	}
	return { html: template.innerHTML, blockedRemoteImages };
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
