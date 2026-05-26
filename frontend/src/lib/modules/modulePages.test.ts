import { describe, expect, it } from 'vitest';
import { getModuleObjectHref } from './modulePages';

describe('getModuleObjectHref', () => {
	it('routes note files to the dedicated note editor', () => {
		expect(getModuleObjectHref('notes', 'file', 'note-123')).toBe('/modules/notes/note-123');
	});

	it('routes meeting files to the meeting editor', () => {
		expect(getModuleObjectHref('meetings', 'file', 'meet-123')).toBe('/modules/meetings/meet-123');
	});

	it('routes standup files to the standup editor', () => {
		expect(getModuleObjectHref('standups', 'file', 'stand-123')).toBe(
			'/modules/standups/stand-123'
		);
	});

	it('routes decision files to the decision editor', () => {
		expect(getModuleObjectHref('decisions', 'file', 'dec-123')).toBe('/modules/decisions/dec-123');
	});

	it('routes kanban files to the kanban editor', () => {
		expect(getModuleObjectHref('kanban', 'file', 'kanban-123')).toBe('/modules/kanban/kanban-123');
	});

	it('routes brainstorming files to the brainstorming editor', () => {
		expect(getModuleObjectHref('brainstorming', 'file', 'brain-123')).toBe(
			'/modules/brainstorming/brain-123'
		);
	});

	it('routes share files to the share editor', () => {
		expect(getModuleObjectHref('shares', 'file', 'share-123')).toBe('/modules/shares/share-123');
	});

	it('routes unknown module files to the file preview UI', () => {
		expect(getModuleObjectHref('unknown', 'file', 'file-123')).toBe('/files?preview=file-123');
	});

	it('routes module folders to their module editor', () => {
		expect(getModuleObjectHref('meetings', 'folder', 'folder-123')).toBe(
			'/modules/meetings/folder-123'
		);
		expect(getModuleObjectHref('standups', 'folder', 'folder-123')).toBe(
			'/modules/standups/folder-123'
		);
		expect(getModuleObjectHref('kanban', 'folder', 'folder-123')).toBe(
			'/modules/kanban/folder-123'
		);
	});

	it('routes unknown module folders to the file browser', () => {
		expect(getModuleObjectHref('unknown', 'folder', 'folder-123')).toBe('/files?folder=folder-123');
	});
});
