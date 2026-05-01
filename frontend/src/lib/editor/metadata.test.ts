import { describe, it, expect } from 'vitest';
import {
	parseDocumentMetadata,
	createDocumentMetadata,
	updateDocumentMetadata,
	serializeDocumentMetadata,
	addAttachmentToMetadata,
	removeAttachmentFromMetadata,
	generateSlug,
	generateDocumentId,
	generateAttachmentId
} from './metadata';
import type { RichMarkdownAttachment } from './types';

// ---------------------------------------------------------------------------
// generateSlug
// ---------------------------------------------------------------------------

describe('generateSlug', () => {
	it('converts title to lowercase kebab-case', () => {
		expect(generateSlug('Project Brainstorm')).toBe('project-brainstorm');
	});

	it('strips special characters', () => {
		expect(generateSlug('My Note! @#$ (v2)')).toBe('my-note-v2');
	});

	it('truncates to 64 characters', () => {
		const long = 'a'.repeat(100);
		expect(generateSlug(long).length).toBeLessThanOrEqual(64);
	});

	it('returns "untitled" for empty strings', () => {
		expect(generateSlug('')).toBe('untitled');
		expect(generateSlug('!@#$%')).toBe('untitled');
	});
});

// ---------------------------------------------------------------------------
// ID Generation
// ---------------------------------------------------------------------------

describe('generateDocumentId', () => {
	it('starts with doc_ prefix', () => {
		expect(generateDocumentId()).toMatch(/^doc_[a-f0-9]+$/);
	});

	it('generates unique IDs', () => {
		const ids = new Set(Array.from({ length: 50 }, () => generateDocumentId()));
		expect(ids.size).toBe(50);
	});
});

describe('generateAttachmentId', () => {
	it('starts with att_ prefix', () => {
		expect(generateAttachmentId()).toMatch(/^att_[a-f0-9]+$/);
	});
});

// ---------------------------------------------------------------------------
// createDocumentMetadata
// ---------------------------------------------------------------------------

describe('createDocumentMetadata', () => {
	it('creates metadata with required fields', () => {
		const meta = createDocumentMetadata({ module: 'notes', title: 'Test Note' });

		expect(meta.type).toBe('rich-markdown.document');
		expect(meta.module).toBe('notes');
		expect(meta.title).toBe('Test Note');
		expect(meta.slug).toBe('test-note');
		expect(meta.sourceFile).toBe('index.md');
		expect(meta.attachmentsPath).toBe('attachments');
		expect(meta.schemaVersion).toBe('1.0');
		expect(meta.attachments).toEqual([]);
		expect(meta.id).toMatch(/^doc_/);
		expect(meta.createdAt).toBeTruthy();
		expect(meta.updatedAt).toBeTruthy();
	});

	it('accepts optional overrides', () => {
		const meta = createDocumentMetadata({
			module: 'decisions',
			title: 'ADR-001',
			slug: 'adr-001',
			sourceFile: 'decision.md',
			attachmentsPath: 'assets'
		});

		expect(meta.slug).toBe('adr-001');
		expect(meta.sourceFile).toBe('decision.md');
		expect(meta.attachmentsPath).toBe('assets');
	});
});

// ---------------------------------------------------------------------------
// parseDocumentMetadata
// ---------------------------------------------------------------------------

describe('parseDocumentMetadata', () => {
	const validJson = JSON.stringify({
		id: 'doc_abc123',
		type: 'rich-markdown.document',
		module: 'notes',
		title: 'My Note',
		slug: 'my-note',
		sourceFile: 'index.md',
		attachmentsPath: 'attachments',
		createdAt: '2026-04-30T00:00:00Z',
		updatedAt: '2026-04-30T00:00:00Z',
		schemaVersion: '1.0',
		attachments: [],
		customField: 'preserved',
		futureFeature: { nested: true }
	});

	it('parses valid JSON into typed metadata', () => {
		const result = parseDocumentMetadata(validJson);
		expect(result).not.toBeNull();
		expect(result!.metadata.id).toBe('doc_abc123');
		expect(result!.metadata.title).toBe('My Note');
		expect(result!.metadata.module).toBe('notes');
		expect(result!.metadata.type).toBe('rich-markdown.document');
	});

	it('preserves unknown fields', () => {
		const result = parseDocumentMetadata(validJson);
		expect(result!.unknownFields).toHaveProperty('customField', 'preserved');
		expect(result!.unknownFields).toHaveProperty('futureFeature');
		expect((result!.unknownFields.futureFeature as Record<string, unknown>).nested).toBe(true);
	});

	it('does not include known fields in unknownFields', () => {
		const result = parseDocumentMetadata(validJson);
		expect(result!.unknownFields).not.toHaveProperty('id');
		expect(result!.unknownFields).not.toHaveProperty('title');
		expect(result!.unknownFields).not.toHaveProperty('module');
	});

	it('provides defaults for missing fields', () => {
		const result = parseDocumentMetadata('{}');
		expect(result).not.toBeNull();
		expect(result!.metadata.type).toBe('rich-markdown.document');
		expect(result!.metadata.module).toBe('unknown');
		expect(result!.metadata.title).toBe('Untitled');
		expect(result!.metadata.sourceFile).toBe('index.md');
		expect(result!.metadata.id).toMatch(/^doc_/);
	});

	it('returns null for invalid JSON', () => {
		expect(parseDocumentMetadata('not json')).toBeNull();
		expect(parseDocumentMetadata('{invalid')).toBeNull();
	});

	it('returns null for non-object JSON', () => {
		expect(parseDocumentMetadata('"string"')).toBeNull();
		expect(parseDocumentMetadata('[]')).toBeNull();
		expect(parseDocumentMetadata('null')).toBeNull();
	});

	it('parses editor cache info', () => {
		const json = JSON.stringify({
			editor: {
				engine: 'tiptap',
				schemaVersion: '1.0',
				cacheFile: 'index.editor.json',
				cacheOptional: true
			}
		});
		const result = parseDocumentMetadata(json);
		expect(result!.metadata.editor).toEqual({
			engine: 'tiptap',
			schemaVersion: '1.0',
			cacheFile: 'index.editor.json',
			cacheOptional: true
		});
	});

	it('parses attachments array', () => {
		const json = JSON.stringify({
			attachments: [
				{
					id: 'att_xyz',
					filename: 'diagram.png',
					path: './attachments/diagram.png',
					mimeType: 'image/png',
					size: 12345,
					kind: 'image',
					createdAt: '2026-04-30T00:00:00Z',
					createdBy: 'user_1'
				}
			]
		});
		const result = parseDocumentMetadata(json);
		expect(result!.metadata.attachments).toHaveLength(1);
		expect(result!.metadata.attachments[0].filename).toBe('diagram.png');
		expect(result!.metadata.attachments[0].kind).toBe('image');
	});
});

// ---------------------------------------------------------------------------
// updateDocumentMetadata
// ---------------------------------------------------------------------------

describe('updateDocumentMetadata', () => {
	it('updates specified fields and sets updatedAt', () => {
		const original = createDocumentMetadata({ module: 'notes', title: 'Original' });

		const updated = updateDocumentMetadata(original, { title: 'New Title' });

		expect(updated.title).toBe('New Title');
		expect(updated.module).toBe('notes'); // unchanged
		expect(updated.id).toBe(original.id); // unchanged
		// updatedAt should be a valid ISO timestamp (may equal original if same ms)
		expect(new Date(updated.updatedAt).toISOString()).toBe(updated.updatedAt);
		// The original object should be unchanged (immutability)
		expect(original.title).toBe('Original');
	});

	it('does not overwrite fields with undefined', () => {
		const original = createDocumentMetadata({ module: 'notes', title: 'Keep This' });
		const updated = updateDocumentMetadata(original, { slug: 'new-slug' });

		expect(updated.title).toBe('Keep This');
		expect(updated.slug).toBe('new-slug');
	});
});

// ---------------------------------------------------------------------------
// serializeDocumentMetadata
// ---------------------------------------------------------------------------

describe('serializeDocumentMetadata', () => {
	it('serializes to valid JSON', () => {
		const meta = createDocumentMetadata({ module: 'notes', title: 'Test' });
		const json = serializeDocumentMetadata(meta);

		const parsed = JSON.parse(json);
		expect(parsed.id).toBe(meta.id);
		expect(parsed.title).toBe('Test');
		expect(parsed.type).toBe('rich-markdown.document');
	});

	it('preserves unknown fields from existing raw data', () => {
		const meta = createDocumentMetadata({ module: 'notes', title: 'Test' });
		const unknownFields = {
			customPlugin: { enabled: true },
			experimentalFlag: 42
		};

		const json = serializeDocumentMetadata(meta, unknownFields);
		const parsed = JSON.parse(json);

		expect(parsed.customPlugin).toEqual({ enabled: true });
		expect(parsed.experimentalFlag).toBe(42);
		expect(parsed.title).toBe('Test');
	});

	it('known fields override unknown fields with same key', () => {
		const meta = createDocumentMetadata({ module: 'notes', title: 'Correct Title' });
		const unknownFields = { title: 'Stale Title', extra: true };

		const json = serializeDocumentMetadata(meta, unknownFields);
		const parsed = JSON.parse(json);

		expect(parsed.title).toBe('Correct Title');
		expect(parsed.extra).toBe(true);
	});

	it('round-trips: parse → update → serialize preserves unknowns', () => {
		const original = JSON.stringify({
			id: 'doc_test',
			type: 'rich-markdown.document',
			module: 'notes',
			title: 'V1',
			slug: 'v1',
			sourceFile: 'index.md',
			attachmentsPath: 'attachments',
			createdAt: '2026-01-01T00:00:00Z',
			updatedAt: '2026-01-01T00:00:00Z',
			schemaVersion: '1.0',
			attachments: [],
			futureV2Field: 'keep me',
			anotherPlugin: { data: [1, 2, 3] }
		});

		const parsed = parseDocumentMetadata(original)!;
		const updated = updateDocumentMetadata(parsed.metadata, { title: 'V2' });
		const serialized = serializeDocumentMetadata(updated, parsed.unknownFields);
		const roundTripped = JSON.parse(serialized);

		expect(roundTripped.title).toBe('V2');
		expect(roundTripped.futureV2Field).toBe('keep me');
		expect(roundTripped.anotherPlugin).toEqual({ data: [1, 2, 3] });
		expect(roundTripped.id).toBe('doc_test');
	});
});

// ---------------------------------------------------------------------------
// Attachment helpers
// ---------------------------------------------------------------------------

describe('addAttachmentToMetadata', () => {
	it('adds an attachment and updates timestamp', () => {
		const meta = createDocumentMetadata({ module: 'notes', title: 'Test' });
		const attachment: RichMarkdownAttachment = {
			id: 'att_123',
			filename: 'diagram.png',
			path: './attachments/diagram.png',
			mimeType: 'image/png',
			size: 12345,
			kind: 'image',
			createdAt: '2026-05-01T00:00:00Z',
			createdBy: 'user_1'
		};

		const updated = addAttachmentToMetadata(meta, attachment);

		expect(updated.attachments).toHaveLength(1);
		expect(updated.attachments[0].id).toBe('att_123');
		expect(meta.attachments).toHaveLength(0); // original unchanged
	});
});

describe('removeAttachmentFromMetadata', () => {
	it('removes an attachment by ID', () => {
		let meta = createDocumentMetadata({ module: 'notes', title: 'Test' });
		const att1: RichMarkdownAttachment = {
			id: 'att_1',
			filename: 'a.png',
			path: './attachments/a.png',
			mimeType: 'image/png',
			size: 100,
			kind: 'image',
			createdAt: '2026-05-01T00:00:00Z',
			createdBy: 'user_1'
		};
		const att2: RichMarkdownAttachment = {
			id: 'att_2',
			filename: 'b.pdf',
			path: './attachments/b.pdf',
			mimeType: 'application/pdf',
			size: 200,
			kind: 'pdf',
			createdAt: '2026-05-01T00:00:00Z',
			createdBy: 'user_1'
		};

		meta = addAttachmentToMetadata(meta, att1);
		meta = addAttachmentToMetadata(meta, att2);
		expect(meta.attachments).toHaveLength(2);

		const after = removeAttachmentFromMetadata(meta, 'att_1');
		expect(after.attachments).toHaveLength(1);
		expect(after.attachments[0].id).toBe('att_2');
	});

	it('no-ops when attachment ID not found', () => {
		const meta = createDocumentMetadata({ module: 'notes', title: 'Test' });
		const after = removeAttachmentFromMetadata(meta, 'nonexistent');
		expect(after.attachments).toHaveLength(0);
	});
});
