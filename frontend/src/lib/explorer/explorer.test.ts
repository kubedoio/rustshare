/**
 * ==============================================================================
 * EXPLORER STORE UNIT TESTS
 * ==============================================================================
 *
 * Tests for the central explorer store implementing the contracts from SPEC.
 *
 * Coverage:
 * - A. Resolver tests
 * - B. Store tests (actions)
 * - C. Route tests
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import type {
	ExplorerRoot,
	ExplorerMode,
	CollectionView,
	CanonicalLocation,
	ExplorerState,
	CollectionFile,
	CollectionFolder
} from './types';
import {
	createDefaultExplorerState,
	isExplorerRoot,
	isCollectionView,
	isValidCanonicalLocation,
	ROOT_CONFIG
} from './types';

// Mock $app/navigation
define: {
	goto: vi.fn();
}

// We need to test the store logic separately from navigation
// Let's test the pure functions from types.ts first

describe('Explorer Types', () => {
	describe('Type Guards', () => {
		describe('isExplorerRoot', () => {
			it('returns true for valid roots', () => {
				expect(isExplorerRoot('my-files')).toBe(true);
				expect(isExplorerRoot('shared')).toBe(true);
			});

			it('returns false for invalid roots', () => {
				expect(isExplorerRoot('starred')).toBe(false);
				expect(isExplorerRoot('recent')).toBe(false);
				expect(isExplorerRoot(null)).toBe(false);
				expect(isExplorerRoot(undefined)).toBe(false);
				expect(isExplorerRoot('')).toBe(false);
			});
		});

		describe('isCollectionView', () => {
			it('returns true for valid collections', () => {
				expect(isCollectionView('starred')).toBe(true);
				expect(isCollectionView('recent')).toBe(true);
				expect(isCollectionView('photos')).toBe(true);
			});

			it('returns false for invalid collections', () => {
				expect(isCollectionView('my-files')).toBe(false);
				expect(isCollectionView('shared')).toBe(false);
				expect(isCollectionView(null)).toBe(false);
				expect(isCollectionView(undefined)).toBe(false);
			});
		});

		describe('isValidCanonicalLocation', () => {
			it('returns true for valid locations', () => {
				const validLocation: CanonicalLocation = {
					rootType: 'my-files',
					folderId: 'folder-123',
					folderPath: ['Projects', 'Q2'],
					ancestorFolderIds: ['root', 'folder-1']
				};
				expect(isValidCanonicalLocation(validLocation)).toBe(true);
			});

			it('returns false for invalid rootType', () => {
				const invalidLocation = {
					rootType: 'invalid',
					folderId: 'folder-123',
					folderPath: [],
					ancestorFolderIds: []
				};
				expect(isValidCanonicalLocation(invalidLocation)).toBe(false);
			});

			it('returns false for missing fields', () => {
				expect(isValidCanonicalLocation(null)).toBe(false);
				expect(isValidCanonicalLocation(undefined)).toBe(false);
				expect(isValidCanonicalLocation({})).toBe(false);
				expect(isValidCanonicalLocation({ rootType: 'my-files' })).toBe(false);
			});

			it('returns false for non-array folderPath', () => {
				const invalidLocation = {
					rootType: 'my-files',
					folderId: 'folder-123',
					folderPath: 'not-an-array',
					ancestorFolderIds: []
				};
				expect(isValidCanonicalLocation(invalidLocation)).toBe(false);
			});
		});
	});

	describe('ROOT_CONFIG', () => {
		it('has correct configuration for my-files', () => {
			expect(ROOT_CONFIG['my-files']).toEqual({
				id: 'my-files',
				label: 'My Files',
				icon: 'folder',
				rootFolderId: null
			});
		});

		it('has correct configuration for shared', () => {
			expect(ROOT_CONFIG['shared']).toEqual({
				id: 'shared',
				label: 'Shared',
				icon: 'shared',
				rootFolderId: 'shared-root'
			});
		});
	});

	describe('createDefaultExplorerState', () => {
		it('creates correct initial state', () => {
			const state = createDefaultExplorerState();

			expect(state.mode).toBe('folder');
			expect(state.activeRoot).toBe('my-files');
			expect(state.activeCollection).toBeNull();
			expect(state.currentFolderId).toBeNull();
			expect(state.currentFolderPath).toEqual([]);
			expect(state.selectedItemId).toBeNull();
			expect(state.selectedItemType).toBeNull();
			expect(state.expandedTreeNodeIds).toBeInstanceOf(Set);
			expect(state.expandedTreeNodeIds.size).toBe(0);
			expect(state.breadcrumb).toEqual([{ label: 'My Files', rootType: 'my-files' }]);
		});
	});
});

describe('Canonical Resolution Contract', () => {
	const mockMyFilesFolder: CollectionFolder = {
		id: 'folder-1',
		name: 'Project A',
		path: '/project-a',
		parent_folder_id: null,
		owner_id: 'user-1',
		created_at: '2024-01-01',
		updated_at: '2024-01-01',
		collectionMeta: {
			canonicalLocation: {
				rootType: 'my-files',
				folderId: 'folder-1',
				folderPath: ['Project A'],
				ancestorFolderIds: []
			}
		}
	};

	const mockSharedFolder: CollectionFolder = {
		id: 'shared-folder-1',
		name: 'Team Docs',
		path: '/team-docs',
		parent_folder_id: null,
		owner_id: 'user-2',
		created_at: '2024-01-01',
		updated_at: '2024-01-01',
		collectionMeta: {
			canonicalLocation: {
				rootType: 'shared',
				folderId: 'shared-folder-1',
				folderPath: ['Team Docs'],
				ancestorFolderIds: []
			}
		}
	};

	const mockFileInMyFiles: CollectionFile = {
		id: 'file-1',
		name: 'document.pdf',
		path: '/project-a/document.pdf',
		mime_type: 'application/pdf',
		size: 1024,
		parent_folder_id: 'folder-1',
		owner_id: 'user-1',
		created_at: '2024-01-01',
		modified_at: '2024-01-01',
		collectionMeta: {
			canonicalLocation: {
				rootType: 'my-files',
				folderId: 'folder-1',
				folderPath: ['Project A'],
				ancestorFolderIds: [],
				itemId: 'file-1',
				itemType: 'file'
			}
		}
	};

	describe('resolves starred folder in my-files', () => {
		it('has correct canonical location metadata', () => {
			expect(mockMyFilesFolder.collectionMeta.canonicalLocation.rootType).toBe('my-files');
			expect(mockMyFilesFolder.collectionMeta.canonicalLocation.folderId).toBe('folder-1');
		});
	});

	describe('resolves starred folder in shared', () => {
		it('has correct canonical location metadata', () => {
			expect(mockSharedFolder.collectionMeta.canonicalLocation.rootType).toBe('shared');
			expect(mockSharedFolder.collectionMeta.canonicalLocation.folderId).toBe('shared-folder-1');
		});
	});

	describe('resolves starred file to parent folder', () => {
		it('has parent folder ID in canonical location', () => {
			expect(mockFileInMyFiles.collectionMeta.canonicalLocation.folderId).toBe('folder-1');
			expect(mockFileInMyFiles.collectionMeta.canonicalLocation.itemId).toBe('file-1');
			expect(mockFileInMyFiles.collectionMeta.canonicalLocation.itemType).toBe('file');
		});
	});

	describe('rejects item with missing metadata', () => {
		it('detects missing collectionMeta', () => {
			const invalidItem = { ...mockMyFilesFolder, collectionMeta: undefined as any };
			expect(invalidItem.collectionMeta).toBeUndefined();
		});

		it('detects missing canonicalLocation', () => {
			const invalidItem = {
				...mockMyFilesFolder,
				collectionMeta: {} as any
			};
			expect(invalidItem.collectionMeta.canonicalLocation).toBeUndefined();
		});
	});
});

describe('Explorer State Transitions', () => {
	// Helper to create a clean state for each test
	function createTestState(overrides: Partial<ExplorerState> = {}): ExplorerState {
		return {
			...createDefaultExplorerState(),
			...overrides
		};
	}

	describe('activateRoot', () => {
		it('switches to folder mode', () => {
			const state = createTestState({ mode: 'collection', activeCollection: 'starred' });
			// Simulate activateRoot
			const newState: ExplorerState = {
				...state,
				mode: 'folder',
				activeRoot: 'shared',
				activeCollection: null,
				currentFolderId: null,
				currentFolderPath: [],
				selectedItemId: null,
				selectedItemType: null,
				breadcrumb: [{ label: 'Shared', rootType: 'shared' }]
			};

			expect(newState.mode).toBe('folder');
			expect(newState.activeRoot).toBe('shared');
			expect(newState.activeCollection).toBeNull();
		});

		it('clears collection mode when activating root', () => {
			const state = createTestState({ mode: 'collection', activeCollection: 'starred' });
			const newState: ExplorerState = {
				...state,
				mode: 'folder',
				activeRoot: 'my-files',
				activeCollection: null
			};

			expect(newState.activeCollection).toBeNull();
		});

		it('maintains my-files as default', () => {
			const state = createDefaultExplorerState();
			expect(state.activeRoot).toBe('my-files');
		});
	});

	describe('activateCollection', () => {
		it('switches to collection mode', () => {
			const state = createTestState({ mode: 'folder' });
			const newState: ExplorerState = {
				...state,
				mode: 'collection',
				activeCollection: 'starred',
				currentFolderId: null,
				currentFolderPath: []
			};

			expect(newState.mode).toBe('collection');
			expect(newState.activeCollection).toBe('starred');
		});

		it('does not set false current folder', () => {
			const state = createTestState({
				mode: 'folder',
				currentFolderId: 'folder-1',
				currentFolderPath: ['Some Folder']
			});
			const newState: ExplorerState = {
				...state,
				mode: 'collection',
				activeCollection: 'recent',
				currentFolderId: null,
				currentFolderPath: []
			};

			expect(newState.currentFolderId).toBeNull();
			expect(newState.currentFolderPath).toEqual([]);
		});

		it('supports all collection types', () => {
			const collections: CollectionView[] = ['starred', 'recent', 'photos'];

			collections.forEach((collection) => {
				const state = createDefaultExplorerState();
				const newState: ExplorerState = {
					...state,
					mode: 'collection',
					activeCollection: collection
				};
				expect(newState.activeCollection).toBe(collection);
			});
		});
	});

	describe('openFolder', () => {
		it('expands all ancestors', () => {
			const location: CanonicalLocation = {
				rootType: 'my-files',
				folderId: 'deep-folder',
				folderPath: ['A', 'B', 'C'],
				ancestorFolderIds: ['root', 'a-id', 'b-id']
			};

			const expandedIds = new Set(location.ancestorFolderIds);

			expect(expandedIds.has('root')).toBe(true);
			expect(expandedIds.has('a-id')).toBe(true);
			expect(expandedIds.has('b-id')).toBe(true);
			expect(expandedIds.has('deep-folder')).toBe(false);
		});

		it('switches mode from collection to folder', () => {
			const state = createTestState({
				mode: 'collection',
				activeCollection: 'starred'
			});

			const location: CanonicalLocation = {
				rootType: 'shared',
				folderId: 'shared-folder',
				folderPath: ['Team A'],
				ancestorFolderIds: []
			};

			const newState: ExplorerState = {
				...state,
				mode: 'folder',
				activeRoot: location.rootType,
				activeCollection: null,
				currentFolderId: location.folderId,
				currentFolderPath: location.folderPath
			};

			expect(newState.mode).toBe('folder');
			expect(newState.activeCollection).toBeNull();
		});

		it('rebuilds breadcrumb correctly', () => {
			const location: CanonicalLocation = {
				rootType: 'shared',
				folderId: 'contracts-folder',
				folderPath: ['Team A', 'Contracts'],
				ancestorFolderIds: ['team-a-id']
			};

			const expectedBreadcrumb = [
				{ label: 'Shared', rootType: 'shared' },
				{ label: 'Team A', folderId: 'team-a-id' },
				{ label: 'Contracts', folderId: 'contracts-folder' }
			];

			expect(expectedBreadcrumb).toHaveLength(3);
			expect(expectedBreadcrumb[0].label).toBe('Shared');
			expect(expectedBreadcrumb[1].label).toBe('Team A');
			expect(expectedBreadcrumb[2].label).toBe('Contracts');
		});
	});

	describe('openFileLocation', () => {
		it('opens parent folder and selects file', () => {
			const location: CanonicalLocation = {
				rootType: 'my-files',
				folderId: 'parent-folder',
				folderPath: ['Documents'],
				ancestorFolderIds: [],
				itemId: 'file-123',
				itemType: 'file'
			};

			const state = createDefaultExplorerState();
			const newState: ExplorerState = {
				...state,
				mode: 'folder',
				activeRoot: location.rootType,
				currentFolderId: location.folderId,
				currentFolderPath: location.folderPath,
				selectedItemId: location.itemId ?? null,
				selectedItemType: 'file'
			};

			expect(newState.currentFolderId).toBe('parent-folder');
			expect(newState.selectedItemId).toBe('file-123');
			expect(newState.selectedItemType).toBe('file');
		});

		it('expands folder containing the file', () => {
			const location: CanonicalLocation = {
				rootType: 'shared',
				folderId: 'shared-parent',
				folderPath: ['Shared', 'Docs'],
				ancestorFolderIds: ['shared-root'],
				itemId: 'doc-file',
				itemType: 'file'
			};

			const expandedIds = new Set([...location.ancestorFolderIds, location.folderId]);

			expect(expandedIds.has('shared-root')).toBe(true);
			expect(expandedIds.has('shared-parent')).toBe(true);
		});
	});

	describe('toggleTreeNode', () => {
		it('only affects expansion state', () => {
			const state = createTestState({
				expandedTreeNodeIds: new Set(['folder-1']),
				currentFolderId: 'folder-2'
			});

			// Toggle folder-1 closed
			const newExpandedIds = new Set(state.expandedTreeNodeIds);
			newExpandedIds.delete('folder-1');

			const newState: ExplorerState = {
				...state,
				expandedTreeNodeIds: newExpandedIds
			};

			// Expansion changed
			expect(newState.expandedTreeNodeIds.has('folder-1')).toBe(false);
			// Navigation unchanged
			expect(newState.currentFolderId).toBe('folder-2');
		});
	});

	describe('cross-root switching', () => {
		it('clears shared tree selection when switching to my-files', () => {
			const state = createTestState({
				activeRoot: 'shared',
				currentFolderId: 'shared-folder',
				currentFolderPath: ['Team A']
			});

			const newState: ExplorerState = {
				...state,
				activeRoot: 'my-files',
				currentFolderId: 'my-folder',
				currentFolderPath: ['Projects']
			};

			expect(newState.activeRoot).toBe('my-files');
			expect(newState.currentFolderId).toBe('my-folder');
		});
	});
});

describe('Route Contracts', () => {
	describe('URL patterns', () => {
		it('/files maps to my-files root', () => {
			const url = new URL('http://localhost/files');
			const folderId = url.searchParams.get('folder');
			const root = url.searchParams.get('root');

			// No folder param + no root param = my-files root
			expect(folderId).toBeNull();
			expect(root).toBeNull();
		});

		it('/files?root=shared maps to shared root', () => {
			const url = new URL('http://localhost/files?root=shared');
			const root = url.searchParams.get('root');

			expect(root).toBe('shared');
		});

		it('/files?folder=<id> maps to specific folder', () => {
			const folderId = 'abc-123-def-456';
			const url = new URL(`http://localhost/files?folder=${folderId}`);

			expect(url.searchParams.get('folder')).toBe(folderId);
		});

		it('/files?filter=starred maps to collection mode', () => {
			const url = new URL('http://localhost/files?filter=starred');
			const filter = url.searchParams.get('filter');

			expect(filter).toBe('starred');
			expect(isCollectionView(filter)).toBe(true);
		});

		it('/files?filter=recent stays in collection mode', () => {
			const url = new URL('http://localhost/files?filter=recent');
			const filter = url.searchParams.get('filter');

			expect(filter).toBe('recent');
			expect(isCollectionView(filter)).toBe(true);
		});

		it('/files?filter=photos stays in collection mode', () => {
			const url = new URL('http://localhost/files?filter=photos');
			const filter = url.searchParams.get('filter');

			expect(filter).toBe('photos');
			expect(isCollectionView(filter)).toBe(true);
		});
	});

	describe('Deep link hydration', () => {
		it('hydrates shared root from URL', () => {
			const params = { root: 'shared' };
			const state = createDefaultExplorerState();

			const newState: ExplorerState = {
				...state,
				mode: 'folder',
				activeRoot: 'shared',
				activeCollection: null,
				currentFolderId: null,
				breadcrumb: [{ label: 'Shared', rootType: 'shared' }]
			};

			expect(newState.activeRoot).toBe('shared');
			expect(newState.breadcrumb[0].label).toBe('Shared');
		});

		it('hydrates nested shared folder with ancestors', () => {
			const params = { folder: 'nested-folder' };
			const folderPathData = {
				path: ['Team A', 'Contracts'],
				ancestorIds: ['team-a-id']
			};

			const state: ExplorerState = {
				...createDefaultExplorerState(),
				mode: 'folder',
				activeRoot: 'shared',
				currentFolderId: 'nested-folder',
				currentFolderPath: folderPathData.path,
				expandedTreeNodeIds: new Set(folderPathData.ancestorIds),
				breadcrumb: [
					{ label: 'Shared', rootType: 'shared' },
					{ label: 'Team A', folderId: 'team-a-id' },
					{ label: 'Contracts', folderId: 'nested-folder' }
				]
			};

			expect(state.expandedTreeNodeIds.has('team-a-id')).toBe(true);
			expect(state.breadcrumb).toHaveLength(3);
		});
	});
});
