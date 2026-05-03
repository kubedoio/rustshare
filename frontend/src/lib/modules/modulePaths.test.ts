import { describe, it, expect } from 'vitest';
import {
	getModuleRoot,
	getLegacyModuleRoot,
	isWorkspacePath,
	getModulePathVariants,
	resolvePathInTree
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

	describe('getModulePathVariants', () => {
		it('returns both workspace and legacy paths', () => {
			expect(getModulePathVariants('Notes')).toEqual({
				workspace: '/Workspace/Notes',
				legacy: '/Notes'
			});
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
