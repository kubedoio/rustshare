/**
 * Rich Markdown Editor — Metadata Helpers
 *
 * Read, create, and update .rustshare.json metadata sidecars while
 * preserving unknown fields for forward/backward compatibility.
 */

import type {
	RichMarkdownDocumentMetadata,
	RichMarkdownAttachment,
	EditorCacheInfo
} from './types';

// ---------------------------------------------------------------------------
// ID Generation
// ---------------------------------------------------------------------------

/** Generates a random document ID with `doc_` prefix */
export function generateDocumentId(): string {
	return `doc_${randomHex(12)}`;
}

/** Generates a random attachment ID with `att_` prefix */
export function generateAttachmentId(): string {
	return `att_${randomHex(12)}`;
}

function randomHex(length: number): string {
	const bytes = new Uint8Array(length);
	if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
		crypto.getRandomValues(bytes);
	} else {
		for (let i = 0; i < length; i++) {
			bytes[i] = Math.floor(Math.random() * 256);
		}
	}
	return Array.from(bytes)
		.map((b) => b.toString(16).padStart(2, '0'))
		.join('')
		.substring(0, length);
}

// ---------------------------------------------------------------------------
// Slug Generation
// ---------------------------------------------------------------------------

/** Generates a URL-safe slug from a title */
export function generateSlug(title: string): string {
	return (
		title
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-+|-+$/g, '')
			.substring(0, 64) || 'untitled'
	);
}

// ---------------------------------------------------------------------------
// Metadata Parse Result
// ---------------------------------------------------------------------------

/**
 * Result of parsing a .rustshare.json file.
 * `metadata` contains the typed known fields.
 * `unknownFields` preserves any extra fields for round-trip fidelity.
 */
export interface ParsedDocumentMetadata {
	metadata: RichMarkdownDocumentMetadata;
	unknownFields: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------

/**
 * Parses a .rustshare.json string into typed metadata, preserving unknown fields.
 * Returns null if the JSON is invalid.
 */
export function parseDocumentMetadata(jsonString: string): ParsedDocumentMetadata | null {
	let raw: Record<string, unknown>;
	try {
		raw = JSON.parse(jsonString);
	} catch {
		return null;
	}

	if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) {
		return null;
	}

	const knownKeys = new Set([
		'id',
		'type',
		'module',
		'title',
		'slug',
		'sourceFile',
		'attachmentsPath',
		'createdAt',
		'updatedAt',
		'schemaVersion',
		'editor',
		'attachments'
	]);

	// Collect unknown fields
	const unknownFields: Record<string, unknown> = {};
	for (const key of Object.keys(raw)) {
		if (!knownKeys.has(key)) {
			unknownFields[key] = raw[key];
		}
	}

	const now = new Date().toISOString();

	const metadata: RichMarkdownDocumentMetadata = {
		id: asString(raw.id, generateDocumentId()),
		type: 'rich-markdown.document',
		module: asString(raw.module, 'unknown'),
		title: asString(raw.title, 'Untitled'),
		slug: asString(raw.slug, generateSlug(asString(raw.title, 'untitled'))),
		sourceFile: asString(raw.sourceFile, 'index.md'),
		attachmentsPath: asString(raw.attachmentsPath, 'attachments'),
		createdAt: asString(raw.createdAt, now),
		updatedAt: asString(raw.updatedAt, now),
		schemaVersion: asString(raw.schemaVersion, '1.0'),
		editor: parseEditorCache(raw.editor),
		attachments: parseAttachments(raw.attachments)
	};

	return { metadata, unknownFields };
}

function asString(value: unknown, fallback: string): string {
	return typeof value === 'string' ? value : fallback;
}

function parseEditorCache(value: unknown): EditorCacheInfo | undefined {
	if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
	const obj = value as Record<string, unknown>;
	return {
		engine: asString(obj.engine, 'tiptap'),
		schemaVersion: asString(obj.schemaVersion, '1.0'),
		cacheFile: asString(obj.cacheFile, 'index.editor.json'),
		cacheOptional: obj.cacheOptional === true
	};
}

function parseAttachments(value: unknown): RichMarkdownAttachment[] {
	if (!Array.isArray(value)) return [];
	return value
		.filter((item): item is Record<string, unknown> => typeof item === 'object' && item !== null)
		.map((item) => ({
			id: asString(item.id, generateAttachmentId()),
			filename: asString(item.filename, 'unknown'),
			path: asString(item.path, ''),
			mimeType: asString(item.mimeType, 'application/octet-stream'),
			size: typeof item.size === 'number' ? item.size : 0,
			kind: parseAttachmentKind(item.kind),
			createdAt: asString(item.createdAt, new Date().toISOString()),
			createdBy: asString(item.createdBy, 'unknown')
		}));
}

function parseAttachmentKind(
	value: unknown
): 'image' | 'pdf' | 'document' | 'spreadsheet' | 'archive' | 'other' {
	const validKinds = new Set(['image', 'pdf', 'document', 'spreadsheet', 'archive', 'other']);
	return typeof value === 'string' && validKinds.has(value)
		? (value as RichMarkdownAttachment['kind'])
		: 'other';
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

export interface CreateMetadataOptions {
	module: string;
	title: string;
	slug?: string;
	sourceFile?: string;
	attachmentsPath?: string;
}

/**
 * Creates a new .rustshare.json metadata object with sensible defaults.
 */
export function createDocumentMetadata(
	options: CreateMetadataOptions
): RichMarkdownDocumentMetadata {
	const now = new Date().toISOString();
	return {
		id: generateDocumentId(),
		type: 'rich-markdown.document',
		module: options.module,
		title: options.title,
		slug: options.slug || generateSlug(options.title),
		sourceFile: options.sourceFile || 'index.md',
		attachmentsPath: options.attachmentsPath || 'attachments',
		createdAt: now,
		updatedAt: now,
		schemaVersion: '1.0',
		attachments: []
	};
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

export interface UpdateMetadataFields {
	title?: string;
	slug?: string;
	editor?: EditorCacheInfo;
	attachments?: RichMarkdownAttachment[];
}

/**
 * Returns a new metadata object with the specified fields updated
 * and `updatedAt` set to now.
 */
export function updateDocumentMetadata(
	existing: RichMarkdownDocumentMetadata,
	updates: UpdateMetadataFields
): RichMarkdownDocumentMetadata {
	const cleaned: Partial<RichMarkdownDocumentMetadata> = {};
	for (const [key, value] of Object.entries(updates)) {
		if (value !== undefined) {
			(cleaned as Record<string, unknown>)[key] = value;
		}
	}
	return {
		...existing,
		...cleaned,
		updatedAt: new Date().toISOString()
	};
}

// ---------------------------------------------------------------------------
// Serialize
// ---------------------------------------------------------------------------

/**
 * Serializes metadata to a JSON string suitable for writing to .rustshare.json.
 * If `existingUnknownFields` is provided, those fields are preserved in the output.
 */
export function serializeDocumentMetadata(
	metadata: RichMarkdownDocumentMetadata,
	existingUnknownFields?: Record<string, unknown>
): string {
	const output: Record<string, unknown> = {};

	// Start with unknown fields (preserving them)
	if (existingUnknownFields) {
		Object.assign(output, existingUnknownFields);
	}

	// Overlay known fields (these take precedence)
	output.id = metadata.id;
	output.type = metadata.type;
	output.module = metadata.module;
	output.title = metadata.title;
	output.slug = metadata.slug;
	output.sourceFile = metadata.sourceFile;
	output.attachmentsPath = metadata.attachmentsPath;
	output.createdAt = metadata.createdAt;
	output.updatedAt = metadata.updatedAt;
	output.schemaVersion = metadata.schemaVersion;

	if (metadata.editor) {
		output.editor = metadata.editor;
	}

	output.attachments = metadata.attachments;

	return JSON.stringify(output, null, 2);
}

// ---------------------------------------------------------------------------
// Attachment Helpers
// ---------------------------------------------------------------------------

/**
 * Adds an attachment to the metadata, returning a new metadata object.
 */
export function addAttachmentToMetadata(
	metadata: RichMarkdownDocumentMetadata,
	attachment: RichMarkdownAttachment
): RichMarkdownDocumentMetadata {
	return updateDocumentMetadata(metadata, {
		attachments: [...metadata.attachments, attachment]
	});
}

/**
 * Removes an attachment from the metadata by ID, returning a new metadata object.
 */
export function removeAttachmentFromMetadata(
	metadata: RichMarkdownDocumentMetadata,
	attachmentId: string
): RichMarkdownDocumentMetadata {
	return updateDocumentMetadata(metadata, {
		attachments: metadata.attachments.filter((a) => a.id !== attachmentId)
	});
}
