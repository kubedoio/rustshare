/**
 * Rich Markdown Editor — Path and Filename Validation
 *
 * Security-critical module. All attachment paths and filenames must be validated
 * before use to prevent path traversal, metadata exposure, and writes outside
 * the document folder.
 */

import type { AttachmentKind } from './types';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Prefix used for RustShare metadata files */
const RUSTSHARE_META_PREFIX = '.rustshare';

/** Maximum attachment filename length */
const MAX_FILENAME_LENGTH = 255;

/** Characters forbidden in filenames */
const FORBIDDEN_FILENAME_CHARS = /[/\\:*?"<>|\x00-\x1f]/;

/** Path traversal segments */
const PATH_TRAVERSAL = /(?:^|[/\\])\.\.(?:[/\\]|$)/;

/** Absolute path patterns (Unix and Windows) */
const ABSOLUTE_PATH = /^(?:\/|[A-Za-z]:[/\\])/;

// ---------------------------------------------------------------------------
// Validation Result
// ---------------------------------------------------------------------------

export interface ValidationResult {
	valid: boolean;
	error?: string;
}

function ok(): ValidationResult {
	return { valid: true };
}

function fail(error: string): ValidationResult {
	return { valid: false, error };
}

// ---------------------------------------------------------------------------
// Path Validation
// ---------------------------------------------------------------------------

/**
 * Validates a document path (directory or file path for a rich document).
 * Rejects path traversal, absolute paths, and empty paths.
 */
export function validateDocumentPath(path: string): ValidationResult {
	if (!path || typeof path !== 'string') {
		return fail('Document path must be a non-empty string');
	}
	if (ABSOLUTE_PATH.test(path)) {
		return fail('Document path must not be absolute');
	}
	if (PATH_TRAVERSAL.test(path)) {
		return fail('Document path must not contain path traversal (..)');
	}
	return ok();
}

/**
 * Validates a Markdown source file path.
 * Must end with .md, must not be absolute, must not traverse.
 */
export function validateSourcePath(path: string): ValidationResult {
	if (!path || typeof path !== 'string') {
		return fail('Source path must be a non-empty string');
	}
	if (!path.endsWith('.md')) {
		return fail('Source path must end with .md');
	}
	if (ABSOLUTE_PATH.test(path)) {
		return fail('Source path must not be absolute');
	}
	if (PATH_TRAVERSAL.test(path)) {
		return fail('Source path must not contain path traversal (..)');
	}
	return ok();
}

/**
 * Validates an attachment path (relative to the document folder).
 * Must start with ./attachments/ or attachments/, must not traverse,
 * must not be absolute, must not reference hidden metadata.
 */
export function validateAttachmentPath(path: string): ValidationResult {
	if (!path || typeof path !== 'string') {
		return fail('Attachment path must be a non-empty string');
	}
	if (ABSOLUTE_PATH.test(path)) {
		return fail('Attachment path must not be absolute');
	}
	if (PATH_TRAVERSAL.test(path)) {
		return fail('Attachment path must not contain path traversal (..)');
	}

	// Normalize: strip leading ./
	const normalized = path.replace(/^\.\//, '');
	if (!normalized.startsWith('attachments/')) {
		return fail('Attachment path must be within the attachments/ folder');
	}

	// Check filename component
	const filename = normalized.split('/').pop() || '';
	if (!filename) {
		return fail('Attachment path must reference a file, not a directory');
	}
	if (isHiddenMetadataFile(filename)) {
		return fail('Attachment path must not reference hidden metadata files');
	}

	return ok();
}

/**
 * Validates an attachment filename (basename only, not a path).
 * Rejects path separators, traversal, hidden metadata names, and invalid characters.
 */
export function validateAttachmentFilename(filename: string): ValidationResult {
	if (!filename || typeof filename !== 'string') {
		return fail('Attachment filename must be a non-empty string');
	}
	if (filename.length > MAX_FILENAME_LENGTH) {
		return fail(`Attachment filename must not exceed ${MAX_FILENAME_LENGTH} characters`);
	}
	if (filename.trim() !== filename) {
		return fail('Attachment filename must not have leading or trailing whitespace');
	}
	if (FORBIDDEN_FILENAME_CHARS.test(filename)) {
		return fail('Attachment filename contains forbidden characters');
	}
	if (filename.includes('..')) {
		return fail('Attachment filename must not contain path traversal (..)');
	}
	if (filename.startsWith('.')) {
		return fail('Attachment filename must not start with a dot');
	}
	if (isHiddenMetadataFile(filename)) {
		return fail('Attachment filename must not be a hidden metadata file');
	}

	return ok();
}

// ---------------------------------------------------------------------------
// Hidden File Detection
// ---------------------------------------------------------------------------

/**
 * Checks if a filename is a hidden metadata file that should never be
 * exposed to users or included in attachment listings.
 */
export function isHiddenMetadataFile(filename: string): boolean {
	if (!filename) return false;
	const lower = filename.toLowerCase();

	// .rustshare prefix (covers .rustshare.json, .rustshare-anything)
	if (lower.startsWith(RUSTSHARE_META_PREFIX)) return true;

	// *.rustshare.json suffix (e.g. "My Note.rustshare.json")
	if (lower.endsWith('.rustshare.json')) return true;

	// Editor cache files
	if (lower === 'index.editor.json') return true;
	if (lower.endsWith('.editor.json')) return true;

	return false;
}

// ---------------------------------------------------------------------------
// Filename Sanitization
// ---------------------------------------------------------------------------

/**
 * Sanitizes a filename for safe storage as an attachment.
 * Strips path components, replaces forbidden characters, removes leading dots.
 */
export function sanitizeAttachmentFilename(filename: string): string {
	if (!filename) return 'unnamed';

	// Extract basename (strip any directory components)
	let name = filename.split(/[/\\]/).pop() || 'unnamed';

	// Replace forbidden characters with underscores
	name = name.replace(/[\\:*?"<>|\x00-\x1f]/g, '_');

	// Remove leading dots
	while (name.startsWith('.')) {
		name = name.substring(1);
	}

	// Trim whitespace
	name = name.trim();

	// Truncate preserving extension
	if (name.length > MAX_FILENAME_LENGTH) {
		const dotIdx = name.lastIndexOf('.');
		if (dotIdx > 0) {
			const ext = name.substring(dotIdx);
			name = name.substring(0, MAX_FILENAME_LENGTH - ext.length) + ext;
		} else {
			name = name.substring(0, MAX_FILENAME_LENGTH);
		}
	}

	return name || 'unnamed';
}

/**
 * Generates a unique filename by appending a numeric suffix when a
 * name already exists in the given set of existing names.
 */
export function deduplicateFilename(filename: string, existingNames: Set<string>): string {
	if (!existingNames.has(filename)) return filename;

	const dotIdx = filename.lastIndexOf('.');
	const base = dotIdx > 0 ? filename.substring(0, dotIdx) : filename;
	const ext = dotIdx > 0 ? filename.substring(dotIdx) : '';

	for (let i = 2; i <= 1000; i++) {
		const candidate = `${base} (${i})${ext}`;
		if (!existingNames.has(candidate)) return candidate;
	}

	// Fallback: append timestamp
	return `${base}-${Date.now()}${ext}`;
}

// ---------------------------------------------------------------------------
// Attachment Kind Classification
// ---------------------------------------------------------------------------

/**
 * Classifies an attachment by its MIME type and filename into a kind category.
 */
export function classifyAttachmentKind(mimeType: string, filename?: string): AttachmentKind {
	const mime = (mimeType || '').toLowerCase();
	const name = (filename || '').toLowerCase();

	if (mime.startsWith('image/')) return 'image';
	if (mime === 'application/pdf' || name.endsWith('.pdf')) return 'pdf';

	if (
		mime.includes('document') ||
		mime.includes('msword') ||
		mime.includes('wordprocessing') ||
		name.endsWith('.doc') ||
		name.endsWith('.docx') ||
		name.endsWith('.odt') ||
		name.endsWith('.rtf')
	)
		return 'document';

	if (
		mime.includes('spreadsheet') ||
		mime.includes('excel') ||
		name.endsWith('.xls') ||
		name.endsWith('.xlsx') ||
		name.endsWith('.csv') ||
		name.endsWith('.ods')
	)
		return 'spreadsheet';

	if (
		mime.includes('zip') ||
		mime.includes('tar') ||
		mime.includes('gzip') ||
		mime.includes('compress') ||
		name.endsWith('.zip') ||
		name.endsWith('.tar') ||
		name.endsWith('.gz') ||
		name.endsWith('.7z') ||
		name.endsWith('.rar')
	)
		return 'archive';

	return 'other';
}
