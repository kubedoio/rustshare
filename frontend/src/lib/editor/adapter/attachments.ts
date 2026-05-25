/**
 * Rich Markdown Editor — Attachment Service
 *
 * Handles attachment validation, preparation, insertion into editor,
 * and filtering. Actual upload is delegated to the parent via events
 * since it depends on document context (folder IDs, API endpoints).
 */

import type { Editor } from '@tiptap/core';
import type { RichMarkdownAttachment, AttachmentKind, EditorPermissions } from '../types';
import { classifyAttachmentKind, deduplicateFilename, isHiddenMetadataFile } from '../validation';
import { isSafeFilename } from './security';
import { generateAttachmentId } from '../metadata';
import { buildAttachmentMarkdownPath } from '../paths';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Maximum individual attachment size: 25 MB */
export const MAX_ATTACHMENT_SIZE = 25 * 1024 * 1024;

/** Maximum total attachments per document */
export const MAX_ATTACHMENTS_PER_DOC = 100;

/** Image MIME types that can be inlined */
const INLINE_IMAGE_MIMES = new Set([
	'image/png',
	'image/jpeg',
	'image/gif',
	'image/webp',
	'image/svg+xml'
]);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface AttachmentValidationResult {
	valid: boolean;
	error?: string;
	sanitizedFilename?: string;
}

export interface PreparedAttachment {
	file: File;
	metadata: RichMarkdownAttachment;
	sanitizedFilename: string;
	relativePath: string;
	isImage: boolean;
}

export interface AttachmentUploadOptions {
	permissions: EditorPermissions;
	existingAttachments: RichMarkdownAttachment[];
	maxSize?: number;
	maxCount?: number;
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/**
 * Validates a file for attachment upload.
 * Checks permissions, size, filename, hidden files, and count limits.
 */
export function validateAttachmentUpload(
	file: File,
	options: AttachmentUploadOptions
): AttachmentValidationResult {
	// Permission check
	if (!options.permissions.canUploadAttachments) {
		return { valid: false, error: 'You do not have permission to upload attachments' };
	}

	// Size check
	const maxSize = options.maxSize ?? MAX_ATTACHMENT_SIZE;
	if (file.size > maxSize) {
		const sizeMB = (maxSize / (1024 * 1024)).toFixed(0);
		return { valid: false, error: `File exceeds maximum size of ${sizeMB} MB` };
	}

	if (file.size === 0) {
		return { valid: false, error: 'File is empty' };
	}

	// Count check
	const maxCount = options.maxCount ?? MAX_ATTACHMENTS_PER_DOC;
	if (options.existingAttachments.length >= maxCount) {
		return { valid: false, error: `Maximum ${maxCount} attachments per document reached` };
	}

	// Validate filename for safety (traversal, hidden files, forbidden names)
	if (!isSafeFilename(file.name)) {
		return { valid: false, error: 'Invalid or forbidden filename' };
	}

	// Sanitize filename for storage
	const sanitized = file.name.replace(/[^a-zA-Z0-9._-]/g, '_');

	return { valid: true, sanitizedFilename: sanitized };
}

/**
 * Validates multiple files for batch upload.
 * Returns per-file results.
 */
export function validateBatchUpload(
	files: File[],
	options: AttachmentUploadOptions
): Map<File, AttachmentValidationResult> {
	const results = new Map<File, AttachmentValidationResult>();
	let currentCount = options.existingAttachments.length;

	for (const file of files) {
		const augmented = {
			...options,
			existingAttachments: [
				...options.existingAttachments,
				// Fake entries for count tracking
				...Array.from({ length: currentCount - options.existingAttachments.length })
			] as RichMarkdownAttachment[]
		};

		// Re-check count with running total
		if (currentCount >= (options.maxCount ?? MAX_ATTACHMENTS_PER_DOC)) {
			results.set(file, {
				valid: false,
				error: `Maximum ${options.maxCount ?? MAX_ATTACHMENTS_PER_DOC} attachments reached`
			});
			continue;
		}

		const result = validateAttachmentUpload(file, options);
		results.set(file, result);

		if (result.valid) {
			currentCount++;
		}
	}

	return results;
}

// ---------------------------------------------------------------------------
// Preparation
// ---------------------------------------------------------------------------

/**
 * Prepares a validated file for upload by creating metadata and
 * deduplicating the filename against existing attachments.
 */
export function prepareAttachment(
	file: File,
	existingAttachments: RichMarkdownAttachment[],
	userId: string = 'unknown'
): PreparedAttachment {
	// Sanitize and deduplicate
	const sanitized = file.name.replace(/[^a-zA-Z0-9._-]/g, '_');
	const existingNames = new Set(existingAttachments.map((a) => a.filename));
	const finalFilename = deduplicateFilename(sanitized, existingNames);

	const kind = classifyAttachmentKind(file.type, finalFilename);
	const relativePath = buildAttachmentMarkdownPath(finalFilename);
	const isImage = INLINE_IMAGE_MIMES.has(file.type);

	const metadata: RichMarkdownAttachment = {
		id: generateAttachmentId(),
		filename: finalFilename,
		path: relativePath,
		mimeType: file.type || 'application/octet-stream',
		size: file.size,
		kind,
		createdAt: new Date().toISOString(),
		createdBy: userId
	};

	return {
		file,
		metadata,
		sanitizedFilename: finalFilename,
		relativePath,
		isImage
	};
}

// ---------------------------------------------------------------------------
// Editor Insertion
// ---------------------------------------------------------------------------

/**
 * Inserts an image into the editor using Markdown image syntax.
 */
export function insertImageIntoEditor(
	editor: Editor,
	filename: string,
	alt?: string,
	path?: string
): void {
	const altText = alt || filename;
	const markdownPath = path || buildAttachmentMarkdownPath(filename);
	const markdown = `![${altText}](${markdownPath})`;
	editor.chain().focus().insertContent(markdown).run();
}

/**
 * Inserts a file link into the editor using Markdown link syntax.
 */
export function insertFileLinkIntoEditor(editor: Editor, filename: string, path?: string): void {
	const markdownPath = path || buildAttachmentMarkdownPath(filename);
	const markdown = `[${filename}](${markdownPath})`;
	editor.chain().focus().insertContent(markdown).run();
}

/**
 * Inserts an attachment into the editor — images are inlined, files are linked.
 */
export function insertAttachmentIntoEditor(
	editor: Editor,
	attachment: PreparedAttachment | RichMarkdownAttachment
): void {
	const filename =
		'sanitizedFilename' in attachment ? attachment.sanitizedFilename : attachment.filename;
	const isImage =
		'isImage' in attachment ? attachment.isImage : INLINE_IMAGE_MIMES.has(attachment.mimeType);
	const path = 'path' in attachment ? attachment.path : undefined;

	if (isImage) {
		insertImageIntoEditor(editor, filename, undefined, path);
	} else {
		insertFileLinkIntoEditor(editor, filename, path);
	}
}

// ---------------------------------------------------------------------------
// Filtering
// ---------------------------------------------------------------------------

/**
 * Filters out hidden metadata files from an attachment list.
 * Use when displaying attachments to users.
 */
export function filterVisibleAttachments(
	attachments: RichMarkdownAttachment[]
): RichMarkdownAttachment[] {
	return attachments.filter((a) => !isHiddenMetadataFile(a.filename));
}

/**
 * Checks if a MIME type is an inlineable image.
 */
export function isInlineableImage(mimeType: string): boolean {
	return INLINE_IMAGE_MIMES.has(mimeType);
}

// ---------------------------------------------------------------------------
// Size Formatting
// ---------------------------------------------------------------------------

/**
 * Formats a file size in bytes to a human-readable string.
 */
export function formatFileSize(bytes: number): string {
	if (bytes === 0) return '0 B';
	const units = ['B', 'KB', 'MB', 'GB'];
	const k = 1024;
	const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1);
	const value = bytes / Math.pow(k, i);
	return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[i]}`;
}

// ---------------------------------------------------------------------------
// Relative path resolution for folder-backed notes
// ---------------------------------------------------------------------------

/**
 * For folder-backed notes: replace relative attachment paths in markdown
 * with API URLs so the editor/viewer can render them.
 */
export function resolveAttachmentPaths(
	markdown: string,
	attachments: RichMarkdownAttachment[]
): string {
	if (!attachments?.length) return markdown;
	let result = markdown;
	for (const att of attachments) {
		const normalizedPath = att.path?.replace(/^\.\//, '') ?? '';
		if (!normalizedPath.startsWith('attachments/') && !normalizedPath.startsWith('drawings/'))
			continue;
		const apiUrl = att.mimeType?.startsWith('image/')
			? `/api/v1/files/${att.id}/preview`
			: `/api/v1/files/${att.id}/content`;
		const escapedPath = normalizedPath.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
		// Match both ./path and path (optional leading ./)
		result = result.replace(
			new RegExp(`!\\[([^\\]]*)\\]\\((?:\\.\\/)?${escapedPath}\\)`, 'g'),
			`![$1](${apiUrl})`
		);
		result = result.replace(
			new RegExp(`\\[([^\\]]*)\\]\\((?:\\.\\/)?${escapedPath}\\)`, 'g'),
			`[$1](${apiUrl})`
		);
	}
	return result;
}

/**
 * For folder-backed notes: replace API URLs in markdown with relative paths
 * before saving.
 */
export function restoreRelativePaths(
	markdown: string,
	attachments: RichMarkdownAttachment[]
): string {
	if (!attachments?.length) return markdown;
	let result = markdown;
	for (const att of attachments) {
		const normalizedPath = att.path?.replace(/^\.\//, '') ?? '';
		if (!normalizedPath.startsWith('attachments/') && !normalizedPath.startsWith('drawings/'))
			continue;
		const apiUrl = att.mimeType?.startsWith('image/')
			? `/api/v1/files/${att.id}/preview`
			: `/api/v1/files/${att.id}/content`;
		const escapedUrl = apiUrl.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
		result = result.replace(
			new RegExp(`!\\[([^\\]]*)\\]\\(${escapedUrl}\\)`, 'g'),
			`![$1](${att.path})`
		);
		result = result.replace(
			new RegExp(`\\[([^\\]]*)\\]\\(${escapedUrl}\\)`, 'g'),
			`[$1](${att.path})`
		);
	}
	return result;
}

/**
 * Generate a collision-safe filename in a folder.
 */
export function generateUniqueFilename(name: string, existingNames: string[]): string {
	if (!existingNames.includes(name)) return name;
	const dotIndex = name.lastIndexOf('.');
	const base = dotIndex > 0 ? name.slice(0, dotIndex) : name;
	const ext = dotIndex > 0 ? name.slice(dotIndex) : '';
	for (let i = 1; i <= 1000; i++) {
		const candidate = `${base}-${i}${ext}`;
		if (!existingNames.includes(candidate)) return candidate;
	}
	return name;
}
