/**
 * Rich Markdown Editor — Barrel Exports
 *
 * Usage:
 *   import { RichMarkdownEditor, MarkdownDocumentPage } from '$lib/editor/components';
 *   import { markdownToHtml, createRichEditor } from '$lib/editor/adapter';
 *   import { validateAttachmentPath, createDocumentMetadata } from '$lib/editor';
 */

// Types
export type {
	EditorMode,
	EditorSaveStatus,
	AttachmentKind,
	DocumentStorageType,
	EditorPermissions,
	RichMarkdownAttachment,
	EditorCacheInfo,
	RichMarkdownDocumentMetadata,
	RichMarkdownDocumentTarget,
	ResolvedDocumentPaths,
	RichMarkdownDocument
} from './types';

export { READ_ONLY_PERMISSIONS, WRITE_PERMISSIONS } from './types';

// Validation
export type { ValidationResult } from './validation';
export {
	validateDocumentPath,
	validateSourcePath,
	validateAttachmentPath,
	validateAttachmentFilename,
	isHiddenMetadataFile,
	sanitizeAttachmentFilename,
	deduplicateFilename,
	classifyAttachmentKind
} from './validation';

// Metadata
export type {
	ParsedDocumentMetadata,
	CreateMetadataOptions,
	UpdateMetadataFields
} from './metadata';
export {
	generateDocumentId,
	generateAttachmentId,
	generateSlug,
	parseDocumentMetadata,
	createDocumentMetadata,
	updateDocumentMetadata,
	serializeDocumentMetadata,
	addAttachmentToMetadata,
	removeAttachmentFromMetadata
} from './metadata';

// Paths
export {
	resolveDocumentPaths,
	detectStorageType,
	buildAttachmentMarkdownPath,
	buildAttachmentStoragePath,
	buildImageMarkdown,
	buildFileLinkMarkdown
} from './paths';
