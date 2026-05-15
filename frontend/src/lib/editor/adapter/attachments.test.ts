import { describe, it, expect } from 'vitest';
import {
	resolveAttachmentPaths,
	restoreRelativePaths,
	validateAttachmentUpload,
	prepareAttachment,
	formatFileSize,
	isInlineableImage
} from './attachments';
import type { RichMarkdownAttachment, EditorPermissions } from '../types';

const WRITE_PERMS: EditorPermissions = {
	canRead: true,
	canEdit: true,
	canUploadAttachments: true,
	canDeleteAttachments: true,
	canExport: true,
	canShare: true
};

function makeAtt(overrides: Partial<RichMarkdownAttachment> = {}): RichMarkdownAttachment {
	return {
		id: 'file-123',
		filename: 'report.pdf',
		path: 'attachments/report.pdf',
		mimeType: 'application/pdf',
		size: 1024,
		kind: 'document',
		createdAt: new Date().toISOString(),
		createdBy: 'user',
		...overrides
	};
}

describe('resolveAttachmentPaths', () => {
	it('resolves plain attachment paths to API URLs', () => {
		const markdown = '[Report](attachments/report.pdf)';
		const result = resolveAttachmentPaths(markdown, [makeAtt()]);
		expect(result).toBe('[Report](/api/v1/files/file-123/content)');
	});

	it('resolves ./ prefixed attachment paths to API URLs', () => {
		const markdown = '[Report](./attachments/report.pdf)';
		const result = resolveAttachmentPaths(markdown, [makeAtt()]);
		expect(result).toBe('[Report](/api/v1/files/file-123/content)');
	});

	it('resolves image attachment paths to preview URLs', () => {
		const markdown = '![Diagram](attachments/diagram.png)';
		const result = resolveAttachmentPaths(markdown, [
			makeAtt({ filename: 'diagram.png', path: 'attachments/diagram.png', mimeType: 'image/png' })
		]);
		expect(result).toBe('![Diagram](/api/v1/files/file-123/preview)');
	});

	it('resolves drawing paths to preview URLs', () => {
		const markdown = '![Sketch](drawings/sketch.png)';
		const result = resolveAttachmentPaths(markdown, [
			makeAtt({
				filename: 'sketch.png',
				path: 'drawings/sketch.png',
				mimeType: 'image/png'
			})
		]);
		expect(result).toBe('![Sketch](/api/v1/files/file-123/preview)');
	});

	it('handles attachment metadata with ./ prefixed path', () => {
		const markdown = '[Report](./attachments/report.pdf)';
		const result = resolveAttachmentPaths(markdown, [makeAtt({ path: './attachments/report.pdf' })]);
		expect(result).toBe('[Report](/api/v1/files/file-123/content)');
	});

	it('ignores non-matching paths', () => {
		const markdown = '[External](https://example.com/file.pdf)';
		const result = resolveAttachmentPaths(markdown, [makeAtt()]);
		expect(result).toBe('[External](https://example.com/file.pdf)');
	});

	it('returns original markdown when attachments array is empty', () => {
		const markdown = '[Report](attachments/report.pdf)';
		const result = resolveAttachmentPaths(markdown, []);
		expect(result).toBe(markdown);
	});
});

describe('restoreRelativePaths', () => {
	it('restores API content URLs to relative paths', () => {
		const markdown = '[Report](/api/v1/files/file-123/content)';
		const result = restoreRelativePaths(markdown, [makeAtt()]);
		expect(result).toBe('[Report](attachments/report.pdf)');
	});

	it('restores API preview URLs to relative paths', () => {
		const markdown = '![Diagram](/api/v1/files/file-123/preview)';
		const result = restoreRelativePaths(markdown, [
			makeAtt({ filename: 'diagram.png', path: 'attachments/diagram.png', mimeType: 'image/png' })
		]);
		expect(result).toBe('![Diagram](attachments/diagram.png)');
	});

	it('preserves original ./ prefix when restoring', () => {
		const markdown = '[Report](/api/v1/files/file-123/content)';
		const result = restoreRelativePaths(markdown, [makeAtt({ path: './attachments/report.pdf' })]);
		expect(result).toBe('[Report](./attachments/report.pdf)');
	});

	it('ignores non-matching URLs', () => {
		const markdown = '[External](https://example.com/file.pdf)';
		const result = restoreRelativePaths(markdown, [makeAtt()]);
		expect(result).toBe('[External](https://example.com/file.pdf)');
	});
});

describe('validateAttachmentUpload', () => {
	it('allows a valid file', () => {
		const file = new File(['x'], 'doc.pdf', { type: 'application/pdf' });
		const result = validateAttachmentUpload(file, {
			permissions: WRITE_PERMS,
			existingAttachments: []
		});
		expect(result.valid).toBe(true);
	});

	it('rejects upload without permission', () => {
		const file = new File(['x'], 'doc.pdf', { type: 'application/pdf' });
		const result = validateAttachmentUpload(file, {
			permissions: { ...WRITE_PERMS, canUploadAttachments: false },
			existingAttachments: []
		});
		expect(result.valid).toBe(false);
		expect(result.error).toContain('permission');
	});

	it('rejects oversized file', () => {
		const file = new File(['x'], 'doc.pdf', { type: 'application/pdf' });
		Object.defineProperty(file, 'size', { value: 100 * 1024 * 1024 });
		const result = validateAttachmentUpload(file, {
			permissions: WRITE_PERMS,
			existingAttachments: []
		});
		expect(result.valid).toBe(false);
		expect(result.error).toContain('size');
	});

	it('rejects empty file', () => {
		const file = new File([], 'empty.pdf', { type: 'application/pdf' });
		const result = validateAttachmentUpload(file, {
			permissions: WRITE_PERMS,
			existingAttachments: []
		});
		expect(result.valid).toBe(false);
		expect(result.error).toContain('empty');
	});

	it('rejects forbidden filename', () => {
		const file = new File(['x'], '.env', { type: 'text/plain' });
		const result = validateAttachmentUpload(file, {
			permissions: WRITE_PERMS,
			existingAttachments: []
		});
		expect(result.valid).toBe(false);
	});
});

describe('prepareAttachment', () => {
	it('creates metadata with correct fields', () => {
		const file = new File(['hello'], 'doc.pdf', { type: 'application/pdf' });
		const prepared = prepareAttachment(file, []);
		expect(prepared.metadata.filename).toBe('doc.pdf');
		expect(prepared.metadata.mimeType).toBe('application/pdf');
		expect(prepared.metadata.path).toBe('attachments/doc.pdf');
		expect(prepared.isImage).toBe(false);
		expect(prepared.relativePath).toBe('attachments/doc.pdf');
	});

	it('deduplicates filename collisions', () => {
		const file = new File(['hello'], 'doc.pdf', { type: 'application/pdf' });
		const existing: RichMarkdownAttachment[] = [
			makeAtt({ filename: 'doc.pdf', path: 'attachments/doc.pdf' })
		];
		const prepared = prepareAttachment(file, existing);
		expect(prepared.metadata.filename).not.toBe('doc.pdf');
		expect(prepared.metadata.filename).toMatch(/^doc \(\d+\)\.pdf$/);
	});
});

describe('formatFileSize', () => {
	it('formats bytes', () => {
		expect(formatFileSize(0)).toBe('0 B');
		expect(formatFileSize(512)).toBe('512 B');
		expect(formatFileSize(1024)).toBe('1.0 KB');
		expect(formatFileSize(1536)).toBe('1.5 KB');
		expect(formatFileSize(1024 * 1024)).toBe('1.0 MB');
	});
});

describe('isInlineableImage', () => {
	it('recognizes inlineable image types', () => {
		expect(isInlineableImage('image/png')).toBe(true);
		expect(isInlineableImage('image/jpeg')).toBe(true);
		expect(isInlineableImage('image/gif')).toBe(true);
		expect(isInlineableImage('image/webp')).toBe(true);
		expect(isInlineableImage('image/svg+xml')).toBe(true);
	});

	it('rejects non-image types', () => {
		expect(isInlineableImage('application/pdf')).toBe(false);
		expect(isInlineableImage('text/plain')).toBe(false);
	});
});
