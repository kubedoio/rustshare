/**
 * Rich Markdown Editor — Document Path Resolution
 *
 * Resolves the file paths for the two supported storage layouts:
 *
 * 1. Folder-backed document:
 *    /{ModuleRoot}/{DocumentSlug}/
 *      index.md
 *      .rustshare.json
 *      /attachments/
 *      index.editor.json        (optional)
 *
 * 2. Single Markdown file:
 *    /path/to/document.md
 *    /path/to/document.rustshare.json   (optional)
 *    /path/to/document.attachments/     (optional)
 */

import type { RichMarkdownDocumentTarget, ResolvedDocumentPaths, DocumentStorageType } from './types';

// ---------------------------------------------------------------------------
// Path Resolution
// ---------------------------------------------------------------------------

/**
 * Resolves a document target into concrete file paths for both storage layouts.
 */
export function resolveDocumentPaths(target: RichMarkdownDocumentTarget): ResolvedDocumentPaths {
	const sourcePath = target.sourcePath;

	// Determine storage type: if sourcePath is "index.md" inside a directory,
	// or if an explicit metadataPath points to a directory-level sidecar,
	// it's folder-backed. Otherwise it's a single file.
	const storageType = detectStorageType(target);

	if (storageType === 'folder-backed') {
		return resolveFolderBacked(target);
	}
	return resolveSingleFile(target);
}

/**
 * Detects the storage type from a document target.
 *
 * Folder-backed when:
 * - sourcePath ends with /index.md
 * - sourcePath is exactly "index.md"
 * - metadataPath is .rustshare.json (relative, no filename prefix)
 * - a rootPath and moduleKey are specified (module documents are folder-backed)
 */
export function detectStorageType(target: RichMarkdownDocumentTarget): DocumentStorageType {
	const source = target.sourcePath;

	// Explicit folder-backed indicators
	if (source === 'index.md' || source.endsWith('/index.md')) {
		return 'folder-backed';
	}

	// If metadata path is ".rustshare.json" (no file prefix), it's folder-backed
	if (target.metadataPath === '.rustshare.json') {
		return 'folder-backed';
	}

	// Module documents are folder-backed by convention
	if (target.moduleKey && target.rootPath) {
		return 'folder-backed';
	}

	return 'single-file';
}

// ---------------------------------------------------------------------------
// Folder-backed Resolution
// ---------------------------------------------------------------------------

function resolveFolderBacked(target: RichMarkdownDocumentTarget): ResolvedDocumentPaths {
	// The document directory is the parent of index.md
	let documentDir: string;

	if (target.sourcePath === 'index.md') {
		// Relative — need rootPath + slug or documentId
		documentDir = target.rootPath || '.';
	} else if (target.sourcePath.endsWith('/index.md')) {
		documentDir = target.sourcePath.slice(0, -'/index.md'.length);
	} else {
		documentDir = dirname(target.sourcePath);
	}

	// Normalize trailing slash
	documentDir = documentDir.replace(/\/+$/, '');

	return {
		storageType: 'folder-backed',
		sourcePath: `${documentDir}/index.md`,
		metadataPath: target.metadataPath || `${documentDir}/.rustshare.json`,
		attachmentsPath: target.attachmentsPath || `${documentDir}/attachments`,
		editorCachePath: `${documentDir}/index.editor.json`,
		documentDir
	};
}

// ---------------------------------------------------------------------------
// Single-file Resolution
// ---------------------------------------------------------------------------

function resolveSingleFile(target: RichMarkdownDocumentTarget): ResolvedDocumentPaths {
	const sourcePath = target.sourcePath;
	const documentDir = dirname(sourcePath);
	const baseName = stripExtension(basename(sourcePath));

	return {
		storageType: 'single-file',
		sourcePath,
		metadataPath: target.metadataPath || `${documentDir}/${baseName}.rustshare.json`,
		attachmentsPath: target.attachmentsPath || `${documentDir}/${baseName}.attachments`,
		editorCachePath: `${documentDir}/${baseName}.editor.json`,
		documentDir
	};
}

// ---------------------------------------------------------------------------
// Path Utilities (no Node.js dependency)
// ---------------------------------------------------------------------------

/** Returns the directory portion of a path */
function dirname(path: string): string {
	const lastSlash = path.lastIndexOf('/');
	if (lastSlash <= 0) return '.';
	return path.substring(0, lastSlash);
}

/** Returns the filename portion of a path */
function basename(path: string): string {
	const lastSlash = path.lastIndexOf('/');
	return lastSlash >= 0 ? path.substring(lastSlash + 1) : path;
}

/** Strips the file extension */
function stripExtension(filename: string): string {
	const lastDot = filename.lastIndexOf('.');
	return lastDot > 0 ? filename.substring(0, lastDot) : filename;
}

// ---------------------------------------------------------------------------
// Attachment Path Builder
// ---------------------------------------------------------------------------

/**
 * Builds the relative Markdown path for an attachment.
 * Always uses `./attachments/filename` format for portability.
 */
export function buildAttachmentMarkdownPath(filename: string): string {
	return `./attachments/${filename}`;
}

/**
 * Builds the absolute storage path for an attachment file
 * within a resolved document.
 */
export function buildAttachmentStoragePath(
	resolved: ResolvedDocumentPaths,
	filename: string
): string {
	return `${resolved.attachmentsPath}/${filename}`;
}

/**
 * Builds the Markdown syntax for inserting an image attachment.
 */
export function buildImageMarkdown(filename: string, alt?: string): string {
	const altText = alt || filename;
	return `![${altText}](./attachments/${filename})`;
}

/**
 * Builds the Markdown syntax for inserting a file attachment link.
 */
export function buildFileLinkMarkdown(filename: string): string {
	return `[${filename}](./attachments/${filename})`;
}
