import { describe, it, expect, beforeEach } from 'vitest';
import {
	validateAttachmentUpload,
	filterVisibleAttachments,
	MAX_ATTACHMENT_SIZE
} from './attachments';
import type { AttachmentUploadOptions } from './attachments';
import { READ_ONLY_PERMISSIONS, WRITE_PERMISSIONS } from '../types';

describe('validateAttachmentUpload', () => {
	let baseOptions: AttachmentUploadOptions;

	beforeEach(() => {
		baseOptions = {
			permissions: WRITE_PERMISSIONS,
			existingAttachments: []
		};
	});

	it('accepts valid image file', () => {
		const file = new File(['content'], 'image.png', { type: 'image/png' });
		const result = validateAttachmentUpload(file, baseOptions);
		expect(result.valid).toBe(true);
		expect(result.sanitizedFilename).toBe('image.png');
	});

	it('accepts valid document file', () => {
		const file = new File(['content'], 'document.pdf', { type: 'application/pdf' });
		const result = validateAttachmentUpload(file, baseOptions);
		expect(result.valid).toBe(true);
		expect(result.sanitizedFilename).toBe('document.pdf');
	});

	it('rejects when no upload permission', () => {
		const file = new File(['content'], 'file.txt', { type: 'text/plain' });
		const result = validateAttachmentUpload(file, {
			...baseOptions,
			permissions: READ_ONLY_PERMISSIONS
		});
		expect(result.valid).toBe(false);
		expect(result.error).toContain('permission');
	});

	it('rejects files exceeding size limit', () => {
		const largeFile = {
			size: MAX_ATTACHMENT_SIZE + 100,
			name: 'large.zip',
			type: 'application/zip'
		} as File;
		const result = validateAttachmentUpload(largeFile, baseOptions);
		expect(result.valid).toBe(false);
		expect(result.error).toContain('exceeds maximum size');
	});

	it('rejects empty files', () => {
		const file = new File([], 'empty.txt', { type: 'text/plain' });
		const result = validateAttachmentUpload(file, baseOptions);
		expect(result.valid).toBe(false);
		expect(result.error).toContain('empty');
	});

	it('rejects hidden metadata files', () => {
		const files = [
			new File(['content'], '.rustshare.json', { type: 'application/json' }),
			new File(['content'], '.DS_Store', { type: 'application/octet-stream' }),
			new File(['content'], 'index.editor.json', { type: 'application/json' })
		];

		for (const file of files) {
			const result = validateAttachmentUpload(file, baseOptions);
			expect(result.valid).toBe(false);
			expect(result.error).toContain('Invalid or forbidden filename');
		}
	});

	it('rejects when attachment count limit reached', () => {
		const file = new File(['content'], 'file.txt', { type: 'text/plain' });
		const result = validateAttachmentUpload(file, {
			...baseOptions,
			existingAttachments: Array(100).fill({}) as any
		});
		expect(result.valid).toBe(false);
		expect(result.error).toContain('Maximum 100 attachments');
	});

	it('sanitizes filenames with forbidden characters', () => {
		const file = new File(['content'], 'my<file>.png', { type: 'image/png' });
		const result = validateAttachmentUpload(file, baseOptions);
		expect(result.valid).toBe(true);
		expect(result.sanitizedFilename).toBe('my_file_.png');
	});

	it('rejects files with path traversal in name', () => {
		const files = [
			new File(['content'], '../traversal.txt', { type: 'text/plain' }),
			new File(['content'], 'folder/../../etc/passwd', { type: 'text/plain' })
		];

		for (const file of files) {
			const result = validateAttachmentUpload(file, baseOptions);
			expect(result.valid).toBe(false);
			expect(result.error).toContain('Invalid or forbidden filename');
		}
	});
});

describe('filterVisibleAttachments', () => {
	it('removes hidden metadata files', () => {
		const attachments = [
			{ filename: 'image.png' },
			{ filename: '.rustshare.json' },
			{ filename: 'doc.pdf' },
			{ filename: 'index.editor.json' }
		] as any;

		const filtered = filterVisibleAttachments(attachments);
		expect(filtered.length).toBe(2);
		expect(filtered[0].filename).toBe('image.png');
		expect(filtered[1].filename).toBe('doc.pdf');
	});
});
