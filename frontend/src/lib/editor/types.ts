/**
 * Rich Markdown Editor — Shared Types
 *
 * Data model for the RustShare Rich Markdown Editor system.
 * Used across modules (Notes, Meetings, Decisions, etc.) and the file browser.
 *
 * Key invariants:
 * - Markdown is canonical (index.md or document.md)
 * - Editor JSON is optional cache, never sole source of truth
 * - Attachments are real files in an attachments/ folder
 * - Hidden metadata files (.rustshare.json, index.editor.json) are never user-visible
 */

// ---------------------------------------------------------------------------
// Enums / Unions
// ---------------------------------------------------------------------------

/** Editor display mode */
export type EditorMode = 'read' | 'edit';

/** Save status for the editor UI */
export type EditorSaveStatus = 'saved' | 'saving' | 'unsaved' | 'error';

/** Classification of an attachment file */
export type AttachmentKind = 'image' | 'pdf' | 'document' | 'spreadsheet' | 'archive' | 'other';

/** Storage layout type */
export type DocumentStorageType = 'folder-backed' | 'single-file';

// ---------------------------------------------------------------------------
// Interfaces
// ---------------------------------------------------------------------------

/** Permissions the current user has for a specific document */
export interface EditorPermissions {
	canRead: boolean;
	canEdit: boolean;
	canUploadAttachments: boolean;
	canDeleteAttachments: boolean;
	canExport: boolean;
	canShare: boolean;
}

/** Attachment metadata (matches attachment-contract.md) */
export interface RichMarkdownAttachment {
	id: string;
	filename: string;
	/** Relative path from the document folder, e.g. "./attachments/diagram.png" */
	path: string;
	mimeType: string;
	size: number;
	kind: AttachmentKind;
	createdAt: string;
	createdBy: string;
}

/** Editor cache info block within .rustshare.json */
export interface EditorCacheInfo {
	engine: string;
	schemaVersion: string;
	cacheFile: string;
	cacheOptional: boolean;
}

/**
 * Document metadata stored in .rustshare.json
 * (matches rich-document-contract.md)
 */
export interface RichMarkdownDocumentMetadata {
	id: string;
	type: 'rich-markdown.document';
	module: string;
	title: string;
	slug: string;
	sourceFile: string;
	attachmentsPath: string;
	createdAt: string;
	updatedAt: string;
	schemaVersion: string;
	editor?: EditorCacheInfo;
	attachments: RichMarkdownAttachment[];
}

/**
 * Document target — tells the editor what to open.
 * Modules construct this and pass it to MarkdownDocumentPage.
 * (matches editor-renderer-contract.md)
 */
export interface RichMarkdownDocumentTarget {
	documentId?: string;
	applicationId?: string;
	rootPath?: string;
	sourcePath: string;
	metadataPath?: string;
	attachmentsPath?: string;
	mode?: EditorMode;
	allowRawMarkdown?: boolean;
}

/** Resolved paths for the two storage layouts */
export interface ResolvedDocumentPaths {
	storageType: DocumentStorageType;
	/** Path to the Markdown source file */
	sourcePath: string;
	/** Path to .rustshare.json metadata sidecar */
	metadataPath: string;
	/** Path to the attachments directory */
	attachmentsPath: string;
	/** Path to optional editor JSON cache */
	editorCachePath: string;
	/** Parent directory of the document */
	documentDir: string;
}

/** Full runtime document model used by the editor */
export interface RichMarkdownDocument {
	target: RichMarkdownDocumentTarget;
	metadata: RichMarkdownDocumentMetadata;
	/** The canonical Markdown content */
	content: string;
	/** Optional editor JSON cache (not canonical) */
	editorJson?: unknown;
	attachments: RichMarkdownAttachment[];
	permissions: EditorPermissions;
	saveStatus: EditorSaveStatus;
	mode: EditorMode;
	revision?: number;
}

// ---------------------------------------------------------------------------
// Permission Presets
// ---------------------------------------------------------------------------

export const READ_ONLY_PERMISSIONS: EditorPermissions = {
	canRead: true,
	canEdit: false,
	canUploadAttachments: false,
	canDeleteAttachments: false,
	canExport: true,
	canShare: false
};

export const WRITE_PERMISSIONS: EditorPermissions = {
	canRead: true,
	canEdit: true,
	canUploadAttachments: true,
	canDeleteAttachments: true,
	canExport: true,
	canShare: true
};
