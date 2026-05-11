import { describe, expect, it } from 'vitest';
import { getModuleObjectHref } from './modulePages';

describe('getModuleObjectHref', () => {
	it('routes note files to the dedicated note editor', () => {
		expect(getModuleObjectHref('notes', 'file', 'note-123')).toBe('/modules/notes/note-123');
	});

	it('routes non-note files to the file preview UI', () => {
		expect(getModuleObjectHref('decisions', 'file', 'file-123')).toBe('/files?preview=file-123');
	});

	it('routes folders to the file browser', () => {
		expect(getModuleObjectHref('kanban', 'folder', 'folder-123')).toBe('/files?folder=folder-123');
	});
});
