import { describe, it, expect } from 'vitest';
import {
	resolveDocumentPaths,
	detectStorageType,
	buildImageMarkdown,
	buildFileLinkMarkdown,
	buildAttachmentMarkdownPath,
	buildAttachmentStoragePath
} from './paths';
import type { RichMarkdownDocumentTarget } from './types';

// ---------------------------------------------------------------------------
// detectStorageType
// ---------------------------------------------------------------------------

describe('detectStorageType', () => {
	it('detects folder-backed from "index.md"', () => {
		expect(detectStorageType({ sourcePath: 'index.md' })).toBe('folder-backed');
	});

	it('detects folder-backed from path ending in /index.md', () => {
		expect(detectStorageType({ sourcePath: 'Notes/my-note/index.md' })).toBe('folder-backed');
	});

	it('detects folder-backed from .rustshare.json metadataPath', () => {
		expect(detectStorageType({ sourcePath: 'doc.md', metadataPath: '.rustshare.json' })).toBe(
			'folder-backed'
		);
	});

	it('detects folder-backed from module context', () => {
		expect(
			detectStorageType({
				sourcePath: 'doc.md',
				applicationId: 'io.elembra.notes',
				rootPath: '/Workspace/Notes'
			})
		).toBe('folder-backed');
	});

	it('detects single-file for plain .md paths', () => {
		expect(detectStorageType({ sourcePath: 'Documents/readme.md' })).toBe('single-file');
		expect(detectStorageType({ sourcePath: 'notes.md' })).toBe('single-file');
	});
});

// ---------------------------------------------------------------------------
// resolveDocumentPaths — folder-backed
// ---------------------------------------------------------------------------

describe('resolveDocumentPaths (folder-backed)', () => {
	it('resolves from explicit index.md path', () => {
		const target: RichMarkdownDocumentTarget = {
			sourcePath: 'Notes/brainstorm/index.md'
		};
		const paths = resolveDocumentPaths(target);

		expect(paths.storageType).toBe('folder-backed');
		expect(paths.sourcePath).toBe('Notes/brainstorm/index.md');
		expect(paths.metadataPath).toBe('Notes/brainstorm/.rustshare.json');
		expect(paths.attachmentsPath).toBe('Notes/brainstorm/attachments');
		expect(paths.editorCachePath).toBe('Notes/brainstorm/index.editor.json');
		expect(paths.documentDir).toBe('Notes/brainstorm');
	});

	it('resolves from bare index.md with rootPath', () => {
		const target: RichMarkdownDocumentTarget = {
			sourcePath: 'index.md',
			rootPath: 'Meetings/standup-2026-05-01'
		};
		const paths = resolveDocumentPaths(target);

		expect(paths.storageType).toBe('folder-backed');
		expect(paths.sourcePath).toBe('Meetings/standup-2026-05-01/index.md');
		expect(paths.documentDir).toBe('Meetings/standup-2026-05-01');
	});

	it('respects explicit metadataPath and attachmentsPath', () => {
		const target: RichMarkdownDocumentTarget = {
			sourcePath: 'Notes/doc/index.md',
			metadataPath: 'Notes/doc/custom-meta.json',
			attachmentsPath: 'Notes/doc/assets'
		};
		const paths = resolveDocumentPaths(target);

		expect(paths.metadataPath).toBe('Notes/doc/custom-meta.json');
		expect(paths.attachmentsPath).toBe('Notes/doc/assets');
	});
});

// ---------------------------------------------------------------------------
// resolveDocumentPaths — single-file
// ---------------------------------------------------------------------------

describe('resolveDocumentPaths (single-file)', () => {
	it('resolves from a plain .md file path', () => {
		const target: RichMarkdownDocumentTarget = {
			sourcePath: 'Documents/proposal.md'
		};
		const paths = resolveDocumentPaths(target);

		expect(paths.storageType).toBe('single-file');
		expect(paths.sourcePath).toBe('Documents/proposal.md');
		expect(paths.metadataPath).toBe('Documents/proposal.rustshare.json');
		expect(paths.attachmentsPath).toBe('Documents/proposal.attachments');
		expect(paths.editorCachePath).toBe('Documents/proposal.editor.json');
		expect(paths.documentDir).toBe('Documents');
	});

	it('handles root-level files', () => {
		const target: RichMarkdownDocumentTarget = {
			sourcePath: 'readme.md'
		};
		const paths = resolveDocumentPaths(target);

		expect(paths.storageType).toBe('single-file');
		expect(paths.sourcePath).toBe('readme.md');
		expect(paths.metadataPath).toBe('./readme.rustshare.json');
		expect(paths.attachmentsPath).toBe('./readme.attachments');
		expect(paths.documentDir).toBe('.');
	});
});

// ---------------------------------------------------------------------------
// Attachment path builders
// ---------------------------------------------------------------------------

describe('buildAttachmentMarkdownPath', () => {
	it('builds relative ./attachments/ path', () => {
		expect(buildAttachmentMarkdownPath('diagram.png')).toBe('./attachments/diagram.png');
	});
});

describe('buildAttachmentStoragePath', () => {
	it('builds full storage path', () => {
		const resolved = resolveDocumentPaths({ sourcePath: 'Notes/doc/index.md' });
		expect(buildAttachmentStoragePath(resolved, 'photo.jpg')).toBe(
			'Notes/doc/attachments/photo.jpg'
		);
	});
});

describe('buildImageMarkdown', () => {
	it('generates image Markdown with alt text', () => {
		expect(buildImageMarkdown('diagram.png', 'Architecture diagram')).toBe(
			'![Architecture diagram](./attachments/diagram.png)'
		);
	});

	it('uses filename as default alt text', () => {
		expect(buildImageMarkdown('photo.jpg')).toBe('![photo.jpg](./attachments/photo.jpg)');
	});
});

describe('buildFileLinkMarkdown', () => {
	it('generates file link Markdown', () => {
		expect(buildFileLinkMarkdown('spec.pdf')).toBe('[spec.pdf](./attachments/spec.pdf)');
	});
});
