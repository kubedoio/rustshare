import { describe, it, expect } from 'vitest';
import { isInternalRustShareFile } from './artifactVisibility';

describe('isInternalRustShareFile', () => {
	it('returns true for system files', () => {
		expect(isInternalRustShareFile('.rustshare.json')).toBe(true);
		expect(isInternalRustShareFile('metadata.json')).toBe(true);
		expect(isInternalRustShareFile('__primary__.md')).toBe(true);
		expect(isInternalRustShareFile('.editor.json')).toBe(true);
	});

	it('returns false for user-visible bundle folders', () => {
		expect(isInternalRustShareFile('attachments')).toBe(false);
		expect(isInternalRustShareFile('drawings')).toBe(false);
		expect(isInternalRustShareFile('exports')).toBe(false);
		expect(isInternalRustShareFile('_rustshare')).toBe(false);
	});

	it('returns false for normal user files', () => {
		expect(isInternalRustShareFile('document.pdf')).toBe(false);
		expect(isInternalRustShareFile('image.png')).toBe(false);
		expect(isInternalRustShareFile('note.md')).toBe(false);
	});
});
