import { describe, it, expect } from 'vitest';
import {
	getAllModules,
	getEnabledModules,
	getSidebarModulesForUser,
	isValidIconKey,
	moduleConfigToDefinition
} from './registry';

describe('Module Registry', () => {
	it('all predefined modules exist', () => {
		const modules = getAllModules();
		expect(modules.length).toBeGreaterThanOrEqual(6);
		const keys = modules.map((m) => m.key);
		expect(keys).toContain('notes');
		expect(keys).toContain('meetings');
		expect(keys).toContain('standups');
		expect(keys).toContain('kanban');
		expect(keys).toContain('decisions');
		expect(keys).toContain('shares');
	});

	it('keys are unique', () => {
		const modules = getAllModules();
		const keys = new Set(modules.map((m) => m.key));
		expect(keys.size).toBe(modules.length);
	});

	it('disabled modules are filtered out', () => {
		const modules = getEnabledModules();
		// Right now all 6 predefined are enabled, so we just verify none have enabled=false
		for (const m of modules) {
			expect(m.enabled).toBe(true);
		}
	});

	it('sidebar order works', () => {
		const user = {
			id: '1',
			email: 'test@example.com',
			display_name: 'Test User',
			is_admin: true
		};
		const sidebarModules = getSidebarModulesForUser(user);
		for (let i = 1; i < sidebarModules.length; i++) {
			expect(sidebarModules[i].ui.sidebar.order).toBeGreaterThanOrEqual(
				sidebarModules[i - 1].ui.sidebar.order
			);
		}
	});

	it('invalid icons rejected', () => {
		expect(isValidIconKey('layout-dashboard')).toBe(true);
		expect(isValidIconKey('sticky-note')).toBe(true);
		expect(isValidIconKey('invalid-random-icon')).toBe(false);
		expect(isValidIconKey('script<alert>1</alert>')).toBe(false);
	});

	it('does not drift from canonical workspace root paths', () => {
		const modules = getAllModules();
		const expectedRoots: Record<string, string> = {
			notes: '/Workspace/Notes',
			meetings: '/Workspace/Meetings',
			standups: '/Workspace/Standups',
			kanban: '/Workspace/Kanban',
			decisions: '/Workspace/Decisions',
			brainstorming: '/Workspace/Brainstorming',
			shares: '/Workspace/Shares'
		};

		for (const module of modules) {
			const expected = expectedRoots[module.key];
			if (expected) {
				expect(module.rootPath).toBe(expected);
			}
		}
	});

	it('moduleConfigToDefinition reflects backend enabled flag', () => {
		const backendModule = {
			id: 'module_notes',
			module_key: 'notes',
			display_name: 'Notes',
			description: 'Notes module',
			enabled: false,
			root_path: '/Workspace/Notes',
			renderer: 'notes',
			default_template: 'template_default_note',
			icon: 'sticky-note',
			schema_version: '1.0',
			permissions: {
				admin_can_configure: true,
				workspace_members_can_use: true,
				allow_public_share: false,
				allow_internal_share: true
			},
			ai_indexing: { enabled: true },
			audit: { enabled: true },
			created_at: '2026-04-30T00:00:00Z',
			updated_at: '2026-04-30T00:00:00Z'
		};

		const def = moduleConfigToDefinition(backendModule as any);
		expect(def.enabled).toBe(false);
		expect(def.key).toBe('notes');
		expect(def.displayName).toBe('Notes');
	});

	it('does not drift from approved icon registry', () => {
		const modules = getAllModules();
		const approved = new Set([
			'layout-dashboard',
			'folder',
			'file-text',
			'sticky-note',
			'calendar-days',
			'clipboard-list',
			'columns',
			'git-branch',
			'path-separation',
			'share-2',
			'lock',
			'globe',
			'settings',
			'lightbulb',
			'activity',
			'mail'
		]);

		for (const module of modules) {
			expect(
				approved.has(module.icon),
				`module ${module.key} uses unapproved icon: ${module.icon}`
			).toBe(true);
		}
	});
});
