import { describe, it, expect } from 'vitest';
import {
	validateDocumentPath,
	validateSourcePath,
	validateAttachmentPath,
	validateAttachmentFilename,
	isHiddenMetadataFile,
	sanitizeAttachmentFilename,
	deduplicateFilename,
	classifyAttachmentKind
} from './validation';

// ---------------------------------------------------------------------------
// validateDocumentPath
// ---------------------------------------------------------------------------

describe('validateDocumentPath', () => {
	it('accepts valid relative paths', () => {
		expect(validateDocumentPath('Notes/my-note')).toEqual({ valid: true });
		expect(validateDocumentPath('Meetings/2026-05-01/standup')).toEqual({ valid: true });
		expect(validateDocumentPath('document.md')).toEqual({ valid: true });
	});

	it('rejects empty or non-string paths', () => {
		expect(validateDocumentPath('')).toHaveProperty('valid', false);
		expect(validateDocumentPath(null as unknown as string)).toHaveProperty('valid', false);
		expect(validateDocumentPath(undefined as unknown as string)).toHaveProperty('valid', false);
	});

	it('rejects absolute paths', () => {
		expect(validateDocumentPath('/etc/passwd')).toHaveProperty('valid', false);
		expect(validateDocumentPath('C:\\Windows\\system32')).toHaveProperty('valid', false);
	});

	it('rejects path traversal', () => {
		expect(validateDocumentPath('../secret')).toHaveProperty('valid', false);
		expect(validateDocumentPath('notes/../../../etc/passwd')).toHaveProperty('valid', false);
		expect(validateDocumentPath('notes/..\\secret')).toHaveProperty('valid', false);
		expect(validateDocumentPath('..')).toHaveProperty('valid', false);
	});
});

// ---------------------------------------------------------------------------
// validateSourcePath
// ---------------------------------------------------------------------------

describe('validateSourcePath', () => {
	it('accepts valid .md paths', () => {
		expect(validateSourcePath('index.md')).toEqual({ valid: true });
		expect(validateSourcePath('Notes/my-note/index.md')).toEqual({ valid: true });
		expect(validateSourcePath('document.md')).toEqual({ valid: true });
	});

	it('rejects non-.md paths', () => {
		expect(validateSourcePath('index.txt')).toHaveProperty('valid', false);
		expect(validateSourcePath('notes.json')).toHaveProperty('valid', false);
		expect(validateSourcePath('readme')).toHaveProperty('valid', false);
	});

	it('rejects absolute paths', () => {
		expect(validateSourcePath('/var/data/index.md')).toHaveProperty('valid', false);
	});

	it('rejects path traversal', () => {
		expect(validateSourcePath('../index.md')).toHaveProperty('valid', false);
		expect(validateSourcePath('notes/../../secret.md')).toHaveProperty('valid', false);
	});

	it('rejects empty values', () => {
		expect(validateSourcePath('')).toHaveProperty('valid', false);
	});
});

// ---------------------------------------------------------------------------
// validateAttachmentPath
// ---------------------------------------------------------------------------

describe('validateAttachmentPath', () => {
	it('accepts valid attachment paths', () => {
		expect(validateAttachmentPath('./attachments/diagram.png')).toEqual({ valid: true });
		expect(validateAttachmentPath('attachments/photo.jpg')).toEqual({ valid: true });
		expect(validateAttachmentPath('./attachments/sub/file.pdf')).toEqual({ valid: true });
	});

	it('rejects paths outside attachments/', () => {
		expect(validateAttachmentPath('./images/photo.png')).toHaveProperty('valid', false);
		expect(validateAttachmentPath('document.md')).toHaveProperty('valid', false);
		expect(validateAttachmentPath('other/attachments/file.png')).toHaveProperty('valid', false);
	});

	it('rejects absolute paths', () => {
		expect(validateAttachmentPath('/attachments/file.png')).toHaveProperty('valid', false);
	});

	it('rejects path traversal', () => {
		expect(validateAttachmentPath('./attachments/../../../etc/passwd')).toHaveProperty(
			'valid',
			false
		);
		expect(validateAttachmentPath('attachments/../../secret.txt')).toHaveProperty('valid', false);
	});

	it('rejects hidden metadata files as attachments', () => {
		expect(validateAttachmentPath('./attachments/.rustshare.json')).toHaveProperty('valid', false);
		expect(validateAttachmentPath('./attachments/index.editor.json')).toHaveProperty(
			'valid',
			false
		);
	});

	it('rejects directory-only paths', () => {
		expect(validateAttachmentPath('./attachments/')).toHaveProperty('valid', false);
	});

	it('rejects empty values', () => {
		expect(validateAttachmentPath('')).toHaveProperty('valid', false);
	});
});

// ---------------------------------------------------------------------------
// validateAttachmentFilename
// ---------------------------------------------------------------------------

describe('validateAttachmentFilename', () => {
	it('accepts valid filenames', () => {
		expect(validateAttachmentFilename('diagram.png')).toEqual({ valid: true });
		expect(validateAttachmentFilename('my file (2).pdf')).toEqual({ valid: true });
		expect(validateAttachmentFilename('report-2026.xlsx')).toEqual({ valid: true });
		expect(validateAttachmentFilename('photo')).toEqual({ valid: true });
	});

	it('rejects empty filenames', () => {
		expect(validateAttachmentFilename('')).toHaveProperty('valid', false);
	});

	it('rejects filenames with path separators', () => {
		expect(validateAttachmentFilename('path/to/file.png')).toHaveProperty('valid', false);
		expect(validateAttachmentFilename('path\\to\\file.png')).toHaveProperty('valid', false);
	});

	it('rejects filenames with path traversal', () => {
		expect(validateAttachmentFilename('..secret')).toHaveProperty('valid', false);
		expect(validateAttachmentFilename('file..txt')).toHaveProperty('valid', false);
	});

	it('rejects filenames starting with dot', () => {
		expect(validateAttachmentFilename('.hidden')).toHaveProperty('valid', false);
		expect(validateAttachmentFilename('.gitignore')).toHaveProperty('valid', false);
	});

	it('rejects hidden metadata filenames', () => {
		expect(validateAttachmentFilename('.rustshare.json')).toHaveProperty('valid', false);
		expect(validateAttachmentFilename('index.editor.json')).toHaveProperty('valid', false);
	});

	it('rejects filenames with forbidden characters', () => {
		expect(validateAttachmentFilename('file<name>.txt')).toHaveProperty('valid', false);
		expect(validateAttachmentFilename('file|name.txt')).toHaveProperty('valid', false);
		expect(validateAttachmentFilename('file"name".txt')).toHaveProperty('valid', false);
	});

	it('rejects filenames with leading/trailing whitespace', () => {
		expect(validateAttachmentFilename(' file.txt')).toHaveProperty('valid', false);
		expect(validateAttachmentFilename('file.txt ')).toHaveProperty('valid', false);
	});

	it('rejects filenames exceeding max length', () => {
		const longName = 'a'.repeat(256);
		expect(validateAttachmentFilename(longName)).toHaveProperty('valid', false);
	});
});

// ---------------------------------------------------------------------------
// isHiddenMetadataFile
// ---------------------------------------------------------------------------

describe('isHiddenMetadataFile', () => {
	it('detects .rustshare.json', () => {
		expect(isHiddenMetadataFile('.rustshare.json')).toBe(true);
	});

	it('detects .rustshare-prefixed files', () => {
		expect(isHiddenMetadataFile('.rustshare-config')).toBe(true);
		expect(isHiddenMetadataFile('.rustshare.backup')).toBe(true);
	});

	it('detects *.rustshare.json sidecars', () => {
		expect(isHiddenMetadataFile('My Note.rustshare.json')).toBe(true);
		expect(isHiddenMetadataFile('document.rustshare.json')).toBe(true);
	});

	it('detects editor cache files', () => {
		expect(isHiddenMetadataFile('index.editor.json')).toBe(true);
		expect(isHiddenMetadataFile('document.editor.json')).toBe(true);
	});

	it('does not flag normal files', () => {
		expect(isHiddenMetadataFile('diagram.png')).toBe(false);
		expect(isHiddenMetadataFile('report.pdf')).toBe(false);
		expect(isHiddenMetadataFile('index.md')).toBe(false);
		expect(isHiddenMetadataFile('notes.json')).toBe(false);
	});

	it('handles edge cases', () => {
		expect(isHiddenMetadataFile('')).toBe(false);
		expect(isHiddenMetadataFile(null as unknown as string)).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// sanitizeAttachmentFilename
// ---------------------------------------------------------------------------

describe('sanitizeAttachmentFilename', () => {
	it('preserves valid filenames', () => {
		expect(sanitizeAttachmentFilename('diagram.png')).toBe('diagram.png');
		expect(sanitizeAttachmentFilename('my report.pdf')).toBe('my report.pdf');
	});

	it('strips directory components', () => {
		expect(sanitizeAttachmentFilename('path/to/file.png')).toBe('file.png');
		expect(sanitizeAttachmentFilename('C:\\Users\\file.txt')).toBe('file.txt');
	});

	it('replaces forbidden characters', () => {
		expect(sanitizeAttachmentFilename('file<name>.txt')).toBe('file_name_.txt');
		expect(sanitizeAttachmentFilename('file"name"')).toBe('file_name_');
	});

	it('removes leading dots', () => {
		expect(sanitizeAttachmentFilename('.hidden')).toBe('hidden');
		expect(sanitizeAttachmentFilename('..secret')).toBe('secret');
	});

	it('handles empty and whitespace', () => {
		expect(sanitizeAttachmentFilename('')).toBe('unnamed');
		expect(sanitizeAttachmentFilename('   ')).toBe('unnamed');
	});

	it('truncates long filenames preserving extension', () => {
		const longBase = 'a'.repeat(300);
		const result = sanitizeAttachmentFilename(`${longBase}.pdf`);
		expect(result.length).toBeLessThanOrEqual(255);
		expect(result.endsWith('.pdf')).toBe(true);
	});
});

// ---------------------------------------------------------------------------
// deduplicateFilename
// ---------------------------------------------------------------------------

describe('deduplicateFilename', () => {
	it('returns original if no collision', () => {
		expect(deduplicateFilename('photo.png', new Set())).toBe('photo.png');
		expect(deduplicateFilename('photo.png', new Set(['other.png']))).toBe('photo.png');
	});

	it('appends numeric suffix on collision', () => {
		expect(deduplicateFilename('photo.png', new Set(['photo.png']))).toBe('photo (2).png');
	});

	it('increments suffix for multiple collisions', () => {
		const existing = new Set(['photo.png', 'photo (2).png', 'photo (3).png']);
		expect(deduplicateFilename('photo.png', existing)).toBe('photo (4).png');
	});

	it('handles files without extension', () => {
		expect(deduplicateFilename('README', new Set(['README']))).toBe('README (2)');
	});
});

// ---------------------------------------------------------------------------
// classifyAttachmentKind
// ---------------------------------------------------------------------------

describe('classifyAttachmentKind', () => {
	it('classifies images', () => {
		expect(classifyAttachmentKind('image/png')).toBe('image');
		expect(classifyAttachmentKind('image/jpeg')).toBe('image');
		expect(classifyAttachmentKind('image/svg+xml')).toBe('image');
	});

	it('classifies PDFs', () => {
		expect(classifyAttachmentKind('application/pdf')).toBe('pdf');
		expect(classifyAttachmentKind('application/octet-stream', 'report.pdf')).toBe('pdf');
	});

	it('classifies documents', () => {
		expect(classifyAttachmentKind('application/msword')).toBe('document');
		expect(
			classifyAttachmentKind(
				'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
			)
		).toBe('document');
	});

	it('classifies spreadsheets', () => {
		expect(classifyAttachmentKind('application/vnd.ms-excel')).toBe('spreadsheet');
		expect(classifyAttachmentKind('application/octet-stream', 'data.xlsx')).toBe('spreadsheet');
	});

	it('classifies archives', () => {
		expect(classifyAttachmentKind('application/zip')).toBe('archive');
		expect(classifyAttachmentKind('application/gzip')).toBe('archive');
		expect(classifyAttachmentKind('application/octet-stream', 'backup.tar.gz')).toBe('archive');
	});

	it('defaults to other for unknown types', () => {
		expect(classifyAttachmentKind('application/octet-stream')).toBe('other');
		expect(classifyAttachmentKind('video/mp4')).toBe('other');
	});
});
