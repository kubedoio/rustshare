import { describe, it, expect } from 'vitest';
import {
	formatBytes,
	getArtifactTypeLabel,
	getArtifactHref,
	cleanArtifactName,
	todayDateString,
	getUserInitials,
	getActivityVerb,
	getApplicationColor,
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
	it('returns correct labels for canonical Application IDs', () => {
		expect(getArtifactTypeLabel('io.elembra.notes', 'file')).toBe('Note');
		expect(getArtifactTypeLabel('io.elembra.meetings', 'file')).toBe('Meeting Note');
		expect(getArtifactTypeLabel('io.elembra.standups', 'file')).toBe('Standup');
		expect(getArtifactTypeLabel('io.elembra.kanban', 'file')).toBe('Kanban Board');
		expect(getArtifactTypeLabel('io.elembra.decisions', 'file')).toBe('Decision');
		expect(getArtifactTypeLabel('io.elembra.brainstorming', 'file')).toBe('Idea Board');
		expect(getArtifactTypeLabel('io.elembra.shares', 'file')).toBe('Share');
	});

	it('falls back to Folder for unknown module with folder item_type', () => {
		expect(getArtifactTypeLabel('unknown', 'folder')).toBe('Folder');
	});

	it('falls back to File for unknown module with file item_type', () => {
		expect(getArtifactTypeLabel('unknown', 'file')).toBe('File');
	});
});

describe('getArtifactHref', () => {
	it('returns /apps/notes/{id} for notes file', () => {
		expect(
			getArtifactHref({ applicationId: 'io.elembra.notes', item_type: 'file', id: 'abc123' })
		).toBe('/apps/notes/abc123');
	});

	it('returns /apps/decisions/{id} for decisions', () => {
		expect(
			getArtifactHref({ applicationId: 'io.elembra.decisions', item_type: 'file', id: 'def456' })
		).toBe('/apps/decisions/def456');
	});

	it('returns /files?folder={id} for folder', () => {
		expect(
			getArtifactHref({ applicationId: 'io.elembra.files', item_type: 'folder', id: 'ghi789' })
		).toBe('/files?folder=ghi789');
	});

	it('returns /apps/meetings/{id} for meetings', () => {
		expect(
			getArtifactHref({ applicationId: 'io.elembra.meetings', item_type: 'file', id: 'meet456' })
		).toBe('/apps/meetings/meet456');
	});

	it('returns /apps/standups/{id} for standups', () => {
		expect(
			getArtifactHref({ applicationId: 'io.elembra.standups', item_type: 'file', id: 'stand789' })
		).toBe('/apps/standups/stand789');
	});

	it('returns /apps/brainstorming/{id} for brainstorming', () => {
		expect(
			getArtifactHref({
				applicationId: 'io.elembra.brainstorming',
				item_type: 'file',
				id: 'brain012'
			})
		).toBe('/apps/brainstorming/brain012');
	});

	it('returns /apps/kanban for kanban', () => {
		expect(
			getArtifactHref({ applicationId: 'io.elembra.kanban', item_type: 'file', id: 'kanban345' })
		).toBe('/apps/kanban');
	});

	it('returns /apps/shares/{id} for shares', () => {
		expect(
			getArtifactHref({ applicationId: 'io.elembra.shares', item_type: 'file', id: 'share789' })
		).toBe('/apps/shares/share789');
	});

	it('returns /files?preview={id} for excalidraw files', () => {
		expect(
			getArtifactHref({
				applicationId: 'io.elembra.files',
				item_type: 'file',
				id: 'exc123',
				name: 'diagram.excalidraw'
			})
		).toBe('/files?preview=exc123');
	});

	it('returns /files?preview={id} for default file', () => {
		expect(
			getArtifactHref({ applicationId: 'io.elembra.files', item_type: 'file', id: 'jkl012' })
		).toBe('/files?preview=jkl012');
	});

	it('returns /files?folder={id} for folder', () => {
		expect(
			getArtifactHref({ applicationId: 'io.elembra.files', item_type: 'folder', id: 'folder456' })
		).toBe('/files?folder=folder456');
	});

	it('routes folders to file browser even when they have a module key', () => {
		expect(
			getArtifactHref({
				applicationId: 'io.elembra.meetings',
				item_type: 'folder',
				id: 'meet-folder'
			})
		).toBe('/files?folder=meet-folder');
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
	it("returns 'created' for file_uploaded", () => {
		expect(getActivityVerb('file_uploaded')).toBe('created');
	});

	it("returns 'updated' for file_modified", () => {
		expect(getActivityVerb('file_modified')).toBe('updated');
	});

	it("returns 'shared' for share_created", () => {
		expect(getActivityVerb('share_created')).toBe('shared');
	});

	it("returns 'updated' for unknown type", () => {
		expect(getActivityVerb('unknown')).toBe('updated');
	});

	it('composes grammatically with the "You" actor', () => {
		for (const type of [
			'file_uploaded',
			'folder_created',
			'file_modified',
			'file_deleted',
			'file_renamed',
			'file_moved',
			'file_restored',
			'share_created',
			'share_revoked',
			'share_updated',
			'share_received',
			'share_permission_changed',
			'share_revoked_from_user',
			'note_created',
			'note_modified',
			'meeting_created',
			'standup_modified',
			'kanban_created',
			'decision_modified',
			'brainstorm_board_modified'
		]) {
			expect(`You ${getActivityVerb(type)}`).not.toMatch(/^You (was|were|share was|access was)/);
		}
	});
});

describe('getApplicationColor', () => {
	it('returns correct colors for known keys', () => {
		expect(getApplicationColor('notes')).toEqual({
			color: '#ea580c',
			bg: 'rgba(234, 88, 12, 0.1)'
		});
		expect(getApplicationColor('meetings')).toEqual({
			color: '#7c3aed',
			bg: 'rgba(124, 58, 237, 0.1)'
		});
		expect(getApplicationColor('standups')).toEqual({
			color: '#2563eb',
			bg: 'rgba(37, 99, 235, 0.1)'
		});
		expect(getApplicationColor('kanban')).toEqual({
			color: '#ea580c',
			bg: 'rgba(234, 88, 12, 0.1)'
		});
		expect(getApplicationColor('decisions')).toEqual({
			color: '#16a34a',
			bg: 'rgba(22, 163, 74, 0.1)'
		});
		expect(getApplicationColor('brainstorming')).toEqual({
			color: '#ca8a04',
			bg: 'rgba(202, 138, 4, 0.1)'
		});
		expect(getApplicationColor('shares')).toEqual({
			color: '#2563eb',
			bg: 'rgba(37, 99, 235, 0.1)'
		});
	});

	it('returns gray fallback for unknown key', () => {
		expect(getApplicationColor('unknown')).toEqual({
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
