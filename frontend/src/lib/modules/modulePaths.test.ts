import { describe, it, expect } from 'vitest';
import {
	getModuleRoot,
	getLegacyModuleRoot,
	isWorkspacePath,
	isLegacyModuleRoot,
	getModulePathVariants,
	getCanonicalWritePath,
	getModuleReadPaths,
	resolvePathInTree,
	LEGACY_MODULE_ROOTS
} from './modulePaths';
import type { FolderTree } from '$lib/api/folders';

describe('modulePaths', () => {
	describe('getModuleRoot', () => {
		it('returns Workspace-prefixed path for Notes', () => {
			expect(getModuleRoot('Notes')).toBe('/Workspace/Notes');
		});

		it('returns Workspace-prefixed path for Kanban', () => {
			expect(getModuleRoot('Kanban')).toBe('/Workspace/Kanban');
		});

		it('returns Workspace-prefixed path for any module', () => {
			expect(getModuleRoot('Meetings')).toBe('/Workspace/Meetings');
			expect(getModuleRoot('Standups')).toBe('/Workspace/Standups');
			expect(getModuleRoot('Decisions')).toBe('/Workspace/Decisions');
			expect(getModuleRoot('Brainstorming')).toBe('/Workspace/Brainstorming');
			expect(getModuleRoot('Shares')).toBe('/Workspace/Shares');
		});
	});

	describe('getLegacyModuleRoot', () => {
		it('returns direct root path', () => {
			expect(getLegacyModuleRoot('Notes')).toBe('/Notes');
			expect(getLegacyModuleRoot('Kanban')).toBe('/Kanban');
		});
	});

	describe('getCanonicalWritePath', () => {
		it('returns the canonical workspace path for writes', () => {
			expect(getCanonicalWritePath('Notes')).toBe('/Workspace/Notes');
			expect(getCanonicalWritePath('Decisions')).toBe('/Workspace/Decisions');
		});

		it('never returns a legacy root', () => {
			const path = getCanonicalWritePath('Meetings');
			expect(path).not.toBe('/Meetings');
			expect(path.startsWith('/Workspace/')).toBe(true);
		});
	});

	describe('getModuleReadPaths', () => {
		it('includes both canonical and legacy paths', () => {
			expect(getModuleReadPaths('Notes')).toEqual(['/Workspace/Notes', '/Notes']);
		});

		it('lists canonical path first', () => {
			const paths = getModuleReadPaths('Standups');
			expect(paths[0]).toBe('/Workspace/Standups');
			expect(paths[1]).toBe('/Standups');
		});
	});

	describe('isWorkspacePath', () => {
		it('returns true for Workspace paths', () => {
			expect(isWorkspacePath('/Workspace/Notes')).toBe(true);
			expect(isWorkspacePath('/Workspace/Kanban')).toBe(true);
			expect(isWorkspacePath('/Workspace')).toBe(true);
		});

		it('returns false for legacy paths', () => {
			expect(isWorkspacePath('/Notes')).toBe(false);
			expect(isWorkspacePath('/Kanban')).toBe(false);
		});
	});

	describe('isLegacyModuleRoot', () => {
		it('returns true for known legacy module roots', () => {
			expect(isLegacyModuleRoot('/Notes')).toBe(true);
			expect(isLegacyModuleRoot('/Meetings')).toBe(true);
			expect(isLegacyModuleRoot('/Standups')).toBe(true);
			expect(isLegacyModuleRoot('/Decisions')).toBe(true);
			expect(isLegacyModuleRoot('/Kanban')).toBe(true);
			expect(isLegacyModuleRoot('/Brainstorming')).toBe(true);
			expect(isLegacyModuleRoot('/Shares')).toBe(true);
		});

		it('returns false for workspace paths', () => {
			expect(isLegacyModuleRoot('/Workspace/Notes')).toBe(false);
			expect(isLegacyModuleRoot('/Workspace')).toBe(false);
		});

		it('returns false for nested or unknown paths', () => {
			expect(isLegacyModuleRoot('/Notes/Subfolder')).toBe(false);
			expect(isLegacyModuleRoot('/Random')).toBe(false);
			expect(isLegacyModuleRoot('/')).toBe(false);
		});
	});

	describe('LEGACY_MODULE_ROOTS', () => {
		it('contains all known legacy roots', () => {
			expect(LEGACY_MODULE_ROOTS).toContain('Notes');
			expect(LEGACY_MODULE_ROOTS).toContain('Meetings');
			expect(LEGACY_MODULE_ROOTS).toContain('Standups');
			expect(LEGACY_MODULE_ROOTS).toContain('Decisions');
			expect(LEGACY_MODULE_ROOTS).toContain('Kanban');
			expect(LEGACY_MODULE_ROOTS).toContain('Brainstorming');
			expect(LEGACY_MODULE_ROOTS).toContain('Shares');
		});
	});

	describe('getModulePathVariants', () => {
		it('returns both workspace and legacy paths', () => {
			expect(getModulePathVariants('Notes')).toEqual({
				workspace: '/Workspace/Notes',
				legacy: '/Notes'
			});
		});
	});

	describe('legacy root policy compliance', () => {
		it('read paths include legacy roots so old data remains visible', () => {
			const legacyRoots = ['Notes', 'Meetings', 'Standups', 'Decisions', 'Kanban', 'Brainstorming'];
			for (const module of legacyRoots) {
				const paths = getModuleReadPaths(module);
				expect(paths).toContain(getLegacyModuleRoot(module));
				expect(paths).toContain(getModuleRoot(module));
			}
		});

		it('canonical write path is always under /Workspace', () => {
			const allModules = [
				'Notes',
				'Meetings',
				'Standups',
				'Decisions',
				'Kanban',
				'Brainstorming',
				'Shares'
			];
			for (const module of allModules) {
				const writePath = getCanonicalWritePath(module);
				expect(writePath.startsWith('/Workspace/')).toBe(true);
				expect(writePath).not.toBe(getLegacyModuleRoot(module));
			}
		});

		it('legacy roots are never returned as write targets', () => {
			for (const module of LEGACY_MODULE_ROOTS) {
				expect(getCanonicalWritePath(module)).toBe(`/Workspace/${module}`);
			}
		});
	});

	describe('resolvePathInTree', () => {
		function makeTree(name: string, subfolders: FolderTree[] = []): FolderTree {
			return {
				folder: {
					id: `id-${name}`,
					name,
					path: name === 'root' ? '/' : `/${name}`,
					parent_folder_id: null,
					owner_id: 'u1',
					created_at: '',
					updated_at: '',
					tenant_id: 't1',
					ancestor_ids: null,
					is_shared: false,
					share_count: 0,
					share_expires_at: null,
					effective_permission: null
				},
				subfolders
			};
		}

		it('resolves single-level path', () => {
			const tree = makeTree('root');
			expect(resolvePathInTree(tree, '/root')).toBe(tree);
		});

		it('resolves nested path', () => {
			const notes = makeTree('Notes');
			const workspace = makeTree('Workspace', [notes]);
			const tree = makeTree('root', [workspace]);
			expect(resolvePathInTree(tree, '/Workspace/Notes')).toBe(notes);
		});

		it('returns null for missing segment', () => {
			const tree = makeTree('root', [makeTree('Workspace')]);
			expect(resolvePathInTree(tree, '/Workspace/Missing')).toBeNull();
		});

		it('returns null for completely missing path', () => {
			const tree = makeTree('root');
			expect(resolvePathInTree(tree, '/Missing')).toBeNull();
		});
	});
});
