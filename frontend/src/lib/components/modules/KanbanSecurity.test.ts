/**
 * Kanban Security & Validation Tests
 *
 * Tests for:
 * - Label color validation
 * - Slug/path traversal rejection
 * - Markdown sanitization
 * - Hidden metadata file exclusion
 */
import { describe, expect, it } from 'vitest';
import { markdownToHtml } from '$lib/editor/adapter/markdown';

// ── Label color validation ───────────────────────────────────────────

const APPROVED_COLORS = ['green', 'yellow', 'orange', 'red', 'purple', 'blue', 'gray'];

function isApprovedColor(color: string): boolean {
	return APPROVED_COLORS.includes(color);
}

describe('Kanban Label Color Validation', () => {
	it('accepts approved colors', () => {
		for (const color of APPROVED_COLORS) {
			expect(isApprovedColor(color)).toBe(true);
		}
	});

	it('rejects CSS injection attempts', () => {
		expect(isApprovedColor('#ff0000')).toBe(false);
		expect(isApprovedColor('rgb(255,0,0)')).toBe(false);
		expect(isApprovedColor('hsl(0,100%,50%)')).toBe(false);
	});

	it('rejects arbitrary CSS property values', () => {
		expect(isApprovedColor('red; background: url(evil.js)')).toBe(false);
		expect(isApprovedColor('expression(alert(1))')).toBe(false);
	});

	it('rejects empty and whitespace', () => {
		expect(isApprovedColor('')).toBe(false);
		expect(isApprovedColor(' ')).toBe(false);
		expect(isApprovedColor('  green  ')).toBe(false);
	});
});

// ── Slug safety ──────────────────────────────────────────────────────

function isValidSlug(slug: string): boolean {
	if (!slug || slug.length === 0) return false;
	if (slug.includes('/') || slug.includes('\\')) return false;
	if (slug.includes('..')) return false;
	if (slug.includes('\0')) return false;
	if (slug.startsWith('.')) return false;
	// Only allow alphanumeric, dashes, and underscores
	return /^[a-zA-Z0-9][a-zA-Z0-9_-]*$/.test(slug);
}

describe('Kanban Slug Validation', () => {
	it('accepts valid slugs', () => {
		expect(isValidSlug('my-board')).toBe(true);
		expect(isValidSlug('CARD-0001-my-task')).toBe(true);
		expect(isValidSlug('board_123')).toBe(true);
	});

	it('rejects path traversal attempts', () => {
		expect(isValidSlug('../../../etc/passwd')).toBe(false);
		expect(isValidSlug('..\\..\\windows\\system32')).toBe(false);
		expect(isValidSlug('board/../../secret')).toBe(false);
	});

	it('rejects encoded traversal', () => {
		expect(isValidSlug('%2e%2e%2f')).toBe(false); // starts with %
		expect(isValidSlug('..')).toBe(false);
		expect(isValidSlug('.')).toBe(false);
	});

	it('rejects absolute paths', () => {
		expect(isValidSlug('/etc/passwd')).toBe(false);
		expect(isValidSlug('\\windows\\system32')).toBe(false);
	});

	it('rejects hidden file names', () => {
		expect(isValidSlug('.rustshare-board.json')).toBe(false);
		expect(isValidSlug('.rustshare-card.json')).toBe(false);
		expect(isValidSlug('.hidden')).toBe(false);
	});

	it('rejects null bytes', () => {
		expect(isValidSlug('board\0.json')).toBe(false);
	});

	it('rejects empty input', () => {
		expect(isValidSlug('')).toBe(false);
	});
});

// ── Hidden metadata file exclusion ───────────────────────────────────

const HIDDEN_FILES = [
	'.rustshare-board.json',
	'.rustshare-column.json',
	'.rustshare-card.json',
	'events.jsonl'
];

function isHiddenMetadataFile(name: string): boolean {
	return HIDDEN_FILES.includes(name);
}

describe('Kanban Hidden Metadata Exclusion', () => {
	it('identifies all hidden metadata files', () => {
		expect(isHiddenMetadataFile('.rustshare-board.json')).toBe(true);
		expect(isHiddenMetadataFile('.rustshare-column.json')).toBe(true);
		expect(isHiddenMetadataFile('.rustshare-card.json')).toBe(true);
		expect(isHiddenMetadataFile('events.jsonl')).toBe(true);
	});

	it('does not flag normal user files', () => {
		expect(isHiddenMetadataFile('index.md')).toBe(false);
		expect(isHiddenMetadataFile('notes.txt')).toBe(false);
		expect(isHiddenMetadataFile('image.png')).toBe(false);
	});
});

// ── Markdown XSS protection ─────────────────────────────────────────

describe('Kanban Markdown Sanitization', () => {
	it('strips inline script tags', () => {
		const html = markdownToHtml('<script>alert("xss")</script>').html;
		// HTML is first escaped to &lt;script&gt;, then sanitized by DOMPurify
		expect(html).not.toContain('<script');
	});

	it('strips dangerous event handlers from raw img tags', () => {
		const html = markdownToHtml('<img src=x onerror=alert(1)>').html;
		// The key security property: no onerror handler in the output
		expect(html).not.toContain('onerror');
	});

	it('does not render javascript: URLs as clickable links', () => {
		const html = markdownToHtml('[click](javascript:alert(1))').html;
		expect(html).not.toContain('href="javascript:');
	});

	it('does not render javascript: URLs as image sources', () => {
		const html = markdownToHtml('![img](javascript:alert(1))').html;
		expect(html).not.toContain('src="javascript:');
	});

	it('renders safe markdown correctly', () => {
		const html = markdownToHtml('# Hello\n\nThis is **bold** and *italic*.').html;
		expect(html).toContain('<h1>');
		expect(html).toContain('<strong>bold</strong>');
		expect(html).toContain('<em>italic</em>');
	});
});

// ── Attachment filename safety ──────────────────────────────────────

function isValidAttachmentName(name: string): boolean {
	if (!name || name.length === 0) return false;
	if (name.includes('/') || name.includes('\\')) return false;
	if (name === '..' || name === '.') return false;
	if (name.includes('\0')) return false;
	return true;
}

describe('Kanban Attachment Filename Validation', () => {
	it('accepts valid filenames', () => {
		expect(isValidAttachmentName('screenshot.png')).toBe(true);
		expect(isValidAttachmentName('my document.pdf')).toBe(true);
		expect(isValidAttachmentName('file-v2.tar.gz')).toBe(true);
	});

	it('rejects path traversal filenames', () => {
		expect(isValidAttachmentName('../../../etc/passwd')).toBe(false);
		expect(isValidAttachmentName('..\\..\\windows')).toBe(false);
	});

	it('rejects dot-dot and dot filenames', () => {
		expect(isValidAttachmentName('..')).toBe(false);
		expect(isValidAttachmentName('.')).toBe(false);
	});

	it('rejects null bytes in filenames', () => {
		expect(isValidAttachmentName('file\0.exe')).toBe(false);
	});

	it('rejects empty filenames', () => {
		expect(isValidAttachmentName('')).toBe(false);
	});
});
