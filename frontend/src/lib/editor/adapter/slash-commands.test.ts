import { describe, it, expect } from 'vitest';
import { SLASH_COMMANDS, filterSlashCommands, getSlashCommandById } from './slash-commands';

// ---------------------------------------------------------------------------
// SLASH_COMMANDS structure
// ---------------------------------------------------------------------------

describe('SLASH_COMMANDS', () => {
	it('contains expected commands', () => {
		const ids = SLASH_COMMANDS.map((c) => c.id);
		expect(ids).toContain('text');
		expect(ids).toContain('heading1');
		expect(ids).toContain('heading2');
		expect(ids).toContain('heading3');
		expect(ids).toContain('bullet-list');
		expect(ids).toContain('numbered-list');
		expect(ids).toContain('task-list');
		expect(ids).toContain('blockquote');
		expect(ids).toContain('code-block');
		expect(ids).toContain('table');
		expect(ids).toContain('divider');
		expect(ids).toContain('image');
		expect(ids).toContain('file-attachment');
	});

	it('all commands have required fields', () => {
		for (const cmd of SLASH_COMMANDS) {
			expect(cmd.id).toBeTruthy();
			expect(cmd.label).toBeTruthy();
			expect(cmd.description).toBeTruthy();
			expect(cmd.icon).toBeTruthy();
			expect(cmd.keywords.length).toBeGreaterThan(0);
			expect(cmd.group).toBeTruthy();
			expect(typeof cmd.action).toBe('function');
		}
	});

	it('media commands are marked as requiring attachment handler', () => {
		const image = SLASH_COMMANDS.find((c) => c.id === 'image');
		const file = SLASH_COMMANDS.find((c) => c.id === 'file-attachment');
		expect(image?.requiresAttachmentHandler).toBe(true);
		expect(file?.requiresAttachmentHandler).toBe(true);
	});

	it('non-media commands do not require attachment handler', () => {
		const nonMedia = SLASH_COMMANDS.filter((c) => !c.requiresAttachmentHandler);
		expect(nonMedia.length).toBeGreaterThanOrEqual(11);
	});
});

// ---------------------------------------------------------------------------
// filterSlashCommands
// ---------------------------------------------------------------------------

describe('filterSlashCommands', () => {
	it('returns all non-media commands with empty query', () => {
		const results = filterSlashCommands('');
		// Should not include image/file-attachment by default
		expect(results.find((c) => c.id === 'image')).toBeUndefined();
		expect(results.find((c) => c.id === 'file-attachment')).toBeUndefined();
		expect(results.length).toBeGreaterThanOrEqual(11);
	});

	it('returns all commands including media when handler available', () => {
		const results = filterSlashCommands('', { hasAttachmentHandler: true });
		expect(results.find((c) => c.id === 'image')).toBeDefined();
		expect(results.find((c) => c.id === 'file-attachment')).toBeDefined();
	});

	it('filters by label', () => {
		const results = filterSlashCommands('heading');
		expect(results.length).toBe(3);
		expect(results.every((c) => c.label.toLowerCase().includes('heading'))).toBe(true);
	});

	it('filters by keyword', () => {
		const results = filterSlashCommands('todo');
		expect(results.length).toBeGreaterThanOrEqual(1);
		expect(results[0].id).toBe('task-list');
	});

	it('returns empty for no match', () => {
		const results = filterSlashCommands('xyznonexistent');
		expect(results).toHaveLength(0);
	});

	it('is case insensitive', () => {
		const results = filterSlashCommands('HEADING');
		expect(results.length).toBe(3);
	});

	it('matches partial text', () => {
		const results = filterSlashCommands('bul');
		expect(results.find((c) => c.id === 'bullet-list')).toBeDefined();
	});

	it('filters "code" matches code block and inline mentions', () => {
		const results = filterSlashCommands('code');
		expect(results.find((c) => c.id === 'code-block')).toBeDefined();
	});

	it('filters "table" matches table command', () => {
		const results = filterSlashCommands('table');
		expect(results.find((c) => c.id === 'table')).toBeDefined();
	});

	it('filters "divider" or "hr" matches divider', () => {
		expect(filterSlashCommands('divider').find((c) => c.id === 'divider')).toBeDefined();
		expect(filterSlashCommands('hr').find((c) => c.id === 'divider')).toBeDefined();
	});
});

// ---------------------------------------------------------------------------
// getSlashCommandById
// ---------------------------------------------------------------------------

describe('getSlashCommandById', () => {
	it('finds commands by ID', () => {
		expect(getSlashCommandById('heading1')?.label).toBe('Heading 1');
		expect(getSlashCommandById('table')?.label).toBe('Table');
	});

	it('returns undefined for unknown ID', () => {
		expect(getSlashCommandById('nonexistent')).toBeUndefined();
	});
});
