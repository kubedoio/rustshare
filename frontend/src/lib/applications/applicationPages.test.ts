import { describe, expect, it } from 'vitest';
import { getApplicationObjectHref } from './applicationPages';

describe('getApplicationObjectHref', () => {
	it('routes note files to the dedicated note editor', () => {
		expect(getApplicationObjectHref('io.elembra.notes', 'file', 'note-123')).toBe(
			'/apps/notes/note-123'
		);
	});

	it('routes meeting files to the meeting editor', () => {
		expect(getApplicationObjectHref('io.elembra.meetings', 'file', 'meet-123')).toBe(
			'/apps/meetings/meet-123'
		);
	});

	it('routes standup files to the standup editor', () => {
		expect(getApplicationObjectHref('io.elembra.standups', 'file', 'stand-123')).toBe(
			'/apps/standups/stand-123'
		);
	});

	it('routes decision files to the decision editor', () => {
		expect(getApplicationObjectHref('io.elembra.decisions', 'file', 'dec-123')).toBe(
			'/apps/decisions/dec-123'
		);
	});

	it('routes kanban files to the kanban editor', () => {
		expect(getApplicationObjectHref('io.elembra.kanban', 'file', 'kanban-123')).toBe(
			'/apps/kanban/kanban-123'
		);
	});

	it('routes brainstorming files to the brainstorming editor', () => {
		expect(getApplicationObjectHref('io.elembra.brainstorming', 'file', 'brain-123')).toBe(
			'/apps/brainstorming/brain-123'
		);
	});

	it('routes share files to the share editor', () => {
		expect(getApplicationObjectHref('io.elembra.shares', 'file', 'share-123')).toBe(
			'/apps/shares/share-123'
		);
	});

	it('routes unknown module files to the file preview UI', () => {
		expect(getApplicationObjectHref('unknown', 'file', 'file-123')).toBe('/files?preview=file-123');
	});

	it('routes module folders to their module editor', () => {
		expect(getApplicationObjectHref('io.elembra.meetings', 'folder', 'folder-123')).toBe(
			'/apps/meetings/folder-123'
		);
		expect(getApplicationObjectHref('io.elembra.standups', 'folder', 'folder-123')).toBe(
			'/apps/standups/folder-123'
		);
		expect(getApplicationObjectHref('io.elembra.kanban', 'folder', 'folder-123')).toBe(
			'/apps/kanban/folder-123'
		);
	});

	it('routes unknown module folders to the file browser', () => {
		expect(getApplicationObjectHref('unknown', 'folder', 'folder-123')).toBe(
			'/files?folder=folder-123'
		);
	});
});
