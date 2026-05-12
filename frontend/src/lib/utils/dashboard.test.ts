import { describe, it, expect } from 'vitest';
import {
	formatBytes,
	getArtifactTypeLabel,
	getArtifactHref,
	cleanArtifactName,
	todayDateString,
	getUserInitials,
	getActivityVerb,
	getModuleColor,
	getArtifactIcon
} from './dashboard';
import { Columns, Share2, FileText, CheckCircle2, Lightbulb } from 'lucide-svelte';

describe('formatBytes', () => {
	it('returns 0 B for 0', () => {
		expect(formatBytes(0)).toBe('0 B');
	});

	it('returns 1 KB for 1024', () => {
		expect(formatBytes(1024)).toBe('1 KB');
	});

	it('returns 1.5 KB for 1536', () => {
		expect(formatBytes(1536)).toBe('1.5 KB');
	});

	it('returns 1 MB for 1048576', () => {
		expect(formatBytes(1048576)).toBe('1 MB');
	});

	it('returns 1 GB for 1073741824', () => {
		expect(formatBytes(1073741824)).toBe('1 GB');
	});
});

describe('getArtifactTypeLabel', () => {
	it('returns correct labels for all known module keys', () => {
		expect(getArtifactTypeLabel('notes', 'file')).toBe('Note');
		expect(getArtifactTypeLabel('meetings', 'file')).toBe('Meeting Note');
		expect(getArtifactTypeLabel('standups', 'file')).toBe('Standup');
		expect(getArtifactTypeLabel('kanban', 'file')).toBe('Kanban Board');
		expect(getArtifactTypeLabel('decisions', 'file')).toBe('Decision');
		expect(getArtifactTypeLabel('brainstorming', 'file')).toBe('Idea Board');
		expect(getArtifactTypeLabel('shares', 'file')).toBe('Share');
	});

	it('falls back to Folder for unknown module with folder item_type', () => {
		expect(getArtifactTypeLabel('unknown', 'folder')).toBe('Folder');
	});

	it('falls back to File for unknown module with file item_type', () => {
		expect(getArtifactTypeLabel('unknown', 'file')).toBe('File');
	});
});

describe('getArtifactHref', () => {
	it('returns /modules/notes/{id} for notes file', () => {
		expect(getArtifactHref({ moduleKey: 'notes', item_type: 'file', id: 'abc123' })).toBe(
			'/modules/notes/abc123'
		);
	});

	it('returns /modules/decisions/{id} for decisions', () => {
		expect(getArtifactHref({ moduleKey: 'decisions', item_type: 'file', id: 'def456' })).toBe(
			'/modules/decisions/def456'
		);
	});

	it('returns /files?folder={id} for folder', () => {
		expect(getArtifactHref({ moduleKey: 'files', item_type: 'folder', id: 'ghi789' })).toBe(
			'/files?folder=ghi789'
		);
	});

	it('returns /modules/meetings/{id} for meetings', () => {
		expect(getArtifactHref({ moduleKey: 'meetings', item_type: 'file', id: 'meet456' })).toBe(
			'/modules/meetings/meet456'
		);
	});

	it('returns /modules/standups/{id} for standups', () => {
		expect(getArtifactHref({ moduleKey: 'standups', item_type: 'file', id: 'stand789' })).toBe(
			'/modules/standups/stand789'
		);
	});

	it('returns /modules/brainstorming/{id} for brainstorming', () => {
		expect(getArtifactHref({ moduleKey: 'brainstorming', item_type: 'file', id: 'brain012' })).toBe(
			'/modules/brainstorming/brain012'
		);
	});

	it('returns /modules/kanban?boardId={id} for kanban', () => {
		expect(getArtifactHref({ moduleKey: 'kanban', item_type: 'file', id: 'kanban345' })).toBe(
			'/modules/kanban?boardId=kanban345'
		);
	});

	it('returns /files?preview={id} for default file', () => {
		expect(getArtifactHref({ moduleKey: 'files', item_type: 'file', id: 'jkl012' })).toBe(
			'/files?preview=jkl012'
		);
	});
});

describe('cleanArtifactName', () => {
	it('strips .md extension', () => {
		expect(cleanArtifactName('note.md')).toBe('note');
	});

	it('strips .json extension', () => {
		expect(cleanArtifactName('data.json')).toBe('data');
	});

	it('strips .jsonl extension', () => {
		expect(cleanArtifactName('data.jsonl')).toBe('data');
	});

	it('leaves .txt unchanged', () => {
		expect(cleanArtifactName('plain.txt')).toBe('plain.txt');
	});
});

describe('todayDateString', () => {
	it('returns a non-empty string containing the current year', () => {
		const result = todayDateString();
		expect(result).toBeTruthy();
		expect(result.length).toBeGreaterThan(0);
		expect(result).toContain(new Date().getFullYear().toString());
	});
});

describe('getUserInitials', () => {
	it('returns two initials for a full name', () => {
		expect(getUserInitials('Alex Johnson')).toBe('AJ');
	});

	it('returns first two letters for a single name', () => {
		expect(getUserInitials('Melise')).toBe('ME');
	});

	it('returns ? for undefined', () => {
		expect(getUserInitials(undefined)).toBe('?');
	});
});

describe('getActivityVerb', () => {
	it("returns 'was created' for file_uploaded", () => {
		expect(getActivityVerb('file_uploaded')).toBe('was created');
	});

	it("returns 'was updated' for file_modified", () => {
		expect(getActivityVerb('file_modified')).toBe('was updated');
	});

	it("returns 'was shared' for share_created", () => {
		expect(getActivityVerb('share_created')).toBe('was shared');
	});

	it("returns 'was updated' for unknown type", () => {
		expect(getActivityVerb('unknown')).toBe('was updated');
	});
});

describe('getModuleColor', () => {
	it('returns correct colors for known keys', () => {
		expect(getModuleColor('notes')).toEqual({ color: '#ea580c', bg: 'rgba(234, 88, 12, 0.1)' });
		expect(getModuleColor('meetings')).toEqual({ color: '#7c3aed', bg: 'rgba(124, 58, 237, 0.1)' });
		expect(getModuleColor('standups')).toEqual({ color: '#2563eb', bg: 'rgba(37, 99, 235, 0.1)' });
		expect(getModuleColor('kanban')).toEqual({ color: '#ea580c', bg: 'rgba(234, 88, 12, 0.1)' });
		expect(getModuleColor('decisions')).toEqual({ color: '#16a34a', bg: 'rgba(22, 163, 74, 0.1)' });
		expect(getModuleColor('brainstorming')).toEqual({
			color: '#ca8a04',
			bg: 'rgba(202, 138, 4, 0.1)'
		});
		expect(getModuleColor('shares')).toEqual({ color: '#2563eb', bg: 'rgba(37, 99, 235, 0.1)' });
	});

	it('returns gray fallback for unknown key', () => {
		expect(getModuleColor('unknown')).toEqual({
			color: '#6b7280',
			bg: 'rgba(107, 114, 128, 0.1)'
		});
	});
});

describe('getArtifactIcon', () => {
	it('returns correct icon components for known keys', () => {
		expect(getArtifactIcon('notes')).toBe(FileText);
		expect(getArtifactIcon('meetings')).toBe(FileText);
		expect(getArtifactIcon('standups')).toBe(FileText);
		expect(getArtifactIcon('kanban')).toBe(Columns);
		expect(getArtifactIcon('decisions')).toBe(CheckCircle2);
		expect(getArtifactIcon('brainstorming')).toBe(Lightbulb);
		expect(getArtifactIcon('shares')).toBe(Share2);
	});

	it('returns FileText for unknown key', () => {
		expect(getArtifactIcon('unknown')).toBe(FileText);
	});
});
