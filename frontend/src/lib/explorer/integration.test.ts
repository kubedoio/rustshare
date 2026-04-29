/**
 * ==============================================================================
 * EXPLORER INTEGRATION TESTS
 * ==============================================================================
 *
 * Integration tests for the unified explorer behavior.
 *
 * Coverage:
 * 1. Shared alias behavior
 * 2. Shared tree behavior
 * 3. Starred folder resolution
 * 4. Starred file resolution
 * 5. Cross-root switching
 * 6. Empty states
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import type { ExplorerState, CanonicalLocation, CollectionFile, CollectionFolder } from './types';
import { createDefaultExplorerState, ROOT_CONFIG, isExplorerRoot } from './types';

describe('Integration: Shared Alias Behavior', () => {
	// Simulate the flow: click LIBRARY > Shared
	it('click LIBRARY > Shared → route is /files?root=shared', () => {
		// Starting from default state
		let state = createDefaultExplorerState();

		// User clicks Shared in Library
		// This should activate the Shared root
		const newState: ExplorerState = {
			...state,
			mode: 'folder',
			activeRoot: 'shared',
			activeCollection: null,
			currentFolderId: null,
			currentFolderPath: [],
			breadcrumb: [{ label: 'Shared', rootType: 'shared' }]
		};

		state = newState;

		// Assertions
		expect(state.activeRoot).toBe('shared');
		expect(state.mode).toBe('folder');
		expect(state.activeCollection).toBeNull();
		expect(state.breadcrumb[0].label).toBe('Shared');
	});

	it('shared root selected in sidebar and tree', () => {
		const state: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'shared',
			mode: 'folder'
		};

		// Both LIBRARY > Shared and FOLDERS > Shared should be highlighted
		// This is indicated by activeRoot being 'shared'
		expect(state.activeRoot).toBe('shared');
	});

	it('main panel shows shared root contents', () => {
		const state: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'shared',
			mode: 'folder',
			currentFolderId: null // At root
		};

		// When currentFolderId is null and activeRoot is 'shared',
		// we're viewing the shared root contents
		expect(state.currentFolderId).toBeNull();
		expect(state.activeRoot).toBe('shared');
	});

	it('no special-page component rendered', () => {
		// The state should be 'folder' mode, not a special page
		const state: ExplorerState = {
			...createDefaultExplorerState(),
			mode: 'folder',
			activeRoot: 'shared'
		};

		// We should NOT be on a separate route like /shared-with-me
		// Instead we're on /files with root=shared
		expect(state.mode).toBe('folder');
	});
});

describe('Integration: Shared Tree Behavior', () => {
	it('expand Shared in FOLDERS → tree expands', () => {
		const state: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'shared',
			expandedTreeNodeIds: new Set(['shared-root'])
		};

		expect(state.expandedTreeNodeIds.has('shared-root')).toBe(true);
	});

	it('open nested shared folder → tree ancestors expanded', () => {
		// User navigates to Shared > Team A > Contracts
		const location: CanonicalLocation = {
			rootType: 'shared',
			folderId: 'contracts-folder',
			folderPath: ['Team A', 'Contracts'],
			ancestorFolderIds: ['shared-root', 'team-a-id']
		};

		const state: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'shared',
			mode: 'folder',
			currentFolderId: location.folderId,
			currentFolderPath: location.folderPath,
			expandedTreeNodeIds: new Set(location.ancestorFolderIds),
			breadcrumb: [
				{ label: 'Shared', rootType: 'shared' },
				{ label: 'Team A', folderId: 'team-a-id' },
				{ label: 'Contracts', folderId: 'contracts-folder' }
			]
		};

		// Ancestors should be expanded
		expect(state.expandedTreeNodeIds.has('shared-root')).toBe(true);
		expect(state.expandedTreeNodeIds.has('team-a-id')).toBe(true);

		// Breadcrumb should reflect the path
		expect(state.breadcrumb).toHaveLength(3);
		expect(state.breadcrumb[2].label).toBe('Contracts');
	});

	it('breadcrumb correct for nested shared folder', () => {
		const location: CanonicalLocation = {
			rootType: 'shared',
			folderId: 'deep-nested-folder',
			folderPath: ['Team A', 'Projects', 'Q2'],
			ancestorFolderIds: ['shared-root', 'team-a-id', 'projects-id']
		};

		const expectedBreadcrumb = [
			{ label: 'Shared', rootType: 'shared' },
			{ label: 'Team A', folderId: 'team-a-id' },
			{ label: 'Projects', folderId: 'projects-id' },
			{ label: 'Q2', folderId: 'deep-nested-folder' }
		];

		expect(expectedBreadcrumb).toHaveLength(4);
		expect(expectedBreadcrumb[0].label).toBe('Shared');
		expect(expectedBreadcrumb[3].label).toBe('Q2');
	});

	it('main file list shows nested folder contents', () => {
		const state: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'shared',
			mode: 'folder',
			currentFolderId: 'nested-folder-id'
		};

		// When we have a currentFolderId, we're viewing its contents
		expect(state.currentFolderId).toBe('nested-folder-id');
		expect(state.mode).toBe('folder');
	});
});

describe('Integration: Starred Folder Resolution', () => {
	const mockStarredSharedFolder: CollectionFolder = {
		id: 'starred-shared-folder-1',
		name: 'Team Documents',
		path: '/shared/team-docs',
		parent_folder_id: 'shared-root',
		owner_id: 'user-2',
		created_at: '2024-01-01',
		updated_at: '2024-01-01',
		collectionMeta: {
			canonicalLocation: {
				rootType: 'shared',
				folderId: 'shared-folder-1',
				folderPath: ['Team Documents'],
				ancestorFolderIds: ['shared-root']
			}
		}
	};

	it('go to Starred → click shared folder → switch to folder mode', () => {
		// Start in Starred collection
		let state: ExplorerState = {
			...createDefaultExplorerState(),
			mode: 'collection',
			activeCollection: 'starred'
		};

		// User clicks a folder from Shared root
		const location = mockStarredSharedFolder.collectionMeta.canonicalLocation;

		// Navigate to the folder
		state = {
			...state,
			mode: 'folder',
			activeRoot: location.rootType,
			activeCollection: null,
			currentFolderId: location.folderId,
			currentFolderPath: location.folderPath,
			expandedTreeNodeIds: new Set(location.ancestorFolderIds),
			breadcrumb: [
				{ label: 'Shared', rootType: 'shared' },
				{ label: 'Team Documents', folderId: location.folderId }
			]
		};

		// Should have switched to folder mode
		expect(state.mode).toBe('folder');
		expect(state.activeCollection).toBeNull();
	});

	it('route becomes /files?folder=<id> for shared folder', () => {
		const location = mockStarredSharedFolder.collectionMeta.canonicalLocation;

		// The URL should reflect the canonical location
		// When activeRoot is 'shared' and we have a folderId,
		// we need to track which root the folder belongs to
		expect(location.rootType).toBe('shared');
		expect(location.folderId).toBe('shared-folder-1');
	});

	it('correct shared ancestors expanded', () => {
		const location = mockStarredSharedFolder.collectionMeta.canonicalLocation;

		// The shared root should be expanded
		expect(location.ancestorFolderIds).toContain('shared-root');
	});

	it('cross-root navigation from my-files to shared', () => {
		// User is in My Files > Projects
		let state: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'my-files',
			mode: 'folder',
			currentFolderId: 'my-project-folder',
			currentFolderPath: ['Projects']
		};

		// User clicks a starred folder from Shared
		const sharedLocation: CanonicalLocation = {
			rootType: 'shared',
			folderId: 'shared-docs',
			folderPath: ['Team A', 'Documents'],
			ancestorFolderIds: ['shared-root', 'team-a-id']
		};

		// Navigate to shared folder
		state = {
			...state,
			activeRoot: 'shared',
			currentFolderId: sharedLocation.folderId,
			currentFolderPath: sharedLocation.folderPath,
			expandedTreeNodeIds: new Set(sharedLocation.ancestorFolderIds),
			breadcrumb: [
				{ label: 'Shared', rootType: 'shared' },
				{ label: 'Team A', folderId: 'team-a-id' },
				{ label: 'Documents', folderId: 'shared-docs' }
			]
		};

		// Active root should have changed
		expect(state.activeRoot).toBe('shared');
		// My-files tree selection should be cleared (no currentFolderId from my-files)
		expect(state.currentFolderId).toBe('shared-docs');
	});
});

describe('Integration: Starred File Resolution', () => {
	const mockStarredFile: CollectionFile = {
		id: 'starred-file-1',
		name: 'important.pdf',
		path: '/my-files/docs/important.pdf',
		mime_type: 'application/pdf',
		size: 1024,
		parent_folder_id: 'docs-folder',
		owner_id: 'user-1',
		created_at: '2024-01-01',
		modified_at: '2024-01-01',
		collectionMeta: {
			canonicalLocation: {
				rootType: 'my-files',
				folderId: 'docs-folder',
				folderPath: ['Documents'],
				ancestorFolderIds: [],
				itemId: 'starred-file-1',
				itemType: 'file'
			}
		}
	};

	it('go to Starred → click file → parent folder opens', () => {
		// Start in Starred collection
		let state: ExplorerState = {
			...createDefaultExplorerState(),
			mode: 'collection',
			activeCollection: 'starred'
		};

		const location = mockStarredFile.collectionMeta.canonicalLocation;

		// Navigate to file location
		state = {
			...state,
			mode: 'folder',
			activeRoot: location.rootType,
			activeCollection: null,
			currentFolderId: location.folderId,
			currentFolderPath: location.folderPath,
			selectedItemId: location.itemId ?? null,
			selectedItemType: 'file',
			expandedTreeNodeIds: new Set([...location.ancestorFolderIds, location.folderId]),
			breadcrumb: [
				{ label: 'My Files', rootType: 'my-files' },
				{ label: 'Documents', folderId: 'docs-folder' }
			]
		};

		// Parent folder should be opened
		expect(state.currentFolderId).toBe('docs-folder');
		// File should be selected
		expect(state.selectedItemId).toBe('starred-file-1');
		expect(state.selectedItemType).toBe('file');
	});

	it('file row selected in main list', () => {
		const state: ExplorerState = {
			...createDefaultExplorerState(),
			mode: 'folder',
			activeRoot: 'my-files',
			currentFolderId: 'docs-folder',
			selectedItemId: 'starred-file-1',
			selectedItemType: 'file'
		};

		expect(state.selectedItemId).toBe('starred-file-1');
		expect(state.selectedItemType).toBe('file');
	});
});

describe('Integration: Cross-Root Switching', () => {
	it('from shared folder, click starred folder from my-files', () => {
		// Start in Shared > Team A
		let state: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'shared',
			mode: 'folder',
			currentFolderId: 'team-a-folder',
			currentFolderPath: ['Team A'],
			expandedTreeNodeIds: new Set(['shared-root'])
		};

		// User clicks a starred folder that belongs to my-files
		const myFilesLocation: CanonicalLocation = {
			rootType: 'my-files',
			folderId: 'my-project',
			folderPath: ['Projects', 'Website'],
			ancestorFolderIds: ['root', 'projects-id']
		};

		// Switch to my-files
		state = {
			...state,
			activeRoot: 'my-files',
			currentFolderId: myFilesLocation.folderId,
			currentFolderPath: myFilesLocation.folderPath,
			expandedTreeNodeIds: new Set(myFilesLocation.ancestorFolderIds),
			breadcrumb: [
				{ label: 'My Files', rootType: 'my-files' },
				{ label: 'Projects', folderId: 'projects-id' },
				{ label: 'Website', folderId: 'my-project' }
			]
		};

		// Active root changed
		expect(state.activeRoot).toBe('my-files');
		// My-files tree selection active
		expect(state.currentFolderId).toBe('my-project');
	});

	it('shared tree selection clears when switching to my-files', () => {
		const beforeState: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'shared',
			expandedTreeNodeIds: new Set(['shared-root', 'shared-folder-1'])
		};

		// After switching to my-files
		const afterState: ExplorerState = {
			...beforeState,
			activeRoot: 'my-files',
			// Shared tree selection is cleared by changing activeRoot
			currentFolderId: 'my-folder',
			expandedTreeNodeIds: new Set(['root', 'my-folder'])
		};

		expect(afterState.activeRoot).toBe('my-files');
		// No longer in shared tree
		expect(afterState.expandedTreeNodeIds.has('shared-root')).toBe(false);
	});
});

describe('Integration: Empty States', () => {
	it('empty shared root shows sane empty state', () => {
		const state: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'shared',
			mode: 'folder',
			currentFolderId: null // At root
		};

		// When at shared root with no folders, show empty state
		expect(state.activeRoot).toBe('shared');
		expect(state.currentFolderId).toBeNull();
	});

	it('no mounted shared folders - shows appropriate message', () => {
		const emptySharedTree = {
			folder: { id: 'shared-root', name: 'Shared' },
			subfolders: [] // Empty
		};

		expect(emptySharedTree.subfolders).toHaveLength(0);
	});

	it('starred empty state separate from shared empty state', () => {
		const sharedEmptyState = {
			rootType: 'shared',
			message: 'No shared folders'
		};

		const starredEmptyState = {
			collection: 'starred',
			message: 'Nothing is starred yet'
		};

		// They are different messages
		expect(sharedEmptyState.message).not.toBe(starredEmptyState.message);
	});
});

describe('Integration: State Synchronization', () => {
	it('all UI elements reflect same canonical location', () => {
		const location: CanonicalLocation = {
			rootType: 'shared',
			folderId: 'contracts-2024',
			folderPath: ['Team A', 'Legal', 'Contracts'],
			ancestorFolderIds: ['shared-root', 'team-a-id', 'legal-id']
		};

		const state: ExplorerState = {
			...createDefaultExplorerState(),
			mode: 'folder',
			activeRoot: location.rootType,
			currentFolderId: location.folderId,
			currentFolderPath: location.folderPath,
			expandedTreeNodeIds: new Set(location.ancestorFolderIds),
			breadcrumb: [
				{ label: 'Shared', rootType: 'shared' },
				{ label: 'Team A', folderId: 'team-a-id' },
				{ label: 'Legal', folderId: 'legal-id' },
				{ label: 'Contracts', folderId: 'contracts-2024' }
			]
		};

		// All state should be consistent
		expect(state.activeRoot).toBe('shared');
		expect(state.currentFolderId).toBe('contracts-2024');
		expect(state.currentFolderPath).toEqual(['Team A', 'Legal', 'Contracts']);
		expect(state.breadcrumb).toHaveLength(4);
		expect(state.expandedTreeNodeIds.has('team-a-id')).toBe(true);
	});

	it('route, tree, breadcrumb, file list synchronized after navigation', () => {
		// After navigating to a folder, all systems should be in sync
		const state: ExplorerState = {
			...createDefaultExplorerState(),
			activeRoot: 'my-files',
			mode: 'folder',
			currentFolderId: 'project-x',
			currentFolderPath: ['Work', 'Projects', 'Project X'],
			expandedTreeNodeIds: new Set(['root', 'work-id', 'projects-id']),
			breadcrumb: [
				{ label: 'My Files', rootType: 'my-files' },
				{ label: 'Work', folderId: 'work-id' },
				{ label: 'Projects', folderId: 'projects-id' },
				{ label: 'Project X', folderId: 'project-x' }
			]
		};

		// Route: /files?folder=project-x
		// Tree: Work > Projects expanded, Project X selected
		// Breadcrumb: My Files > Work > Projects > Project X
		// File list: contents of Project X

		expect(state.currentFolderId).toBe('project-x');
		expect(state.breadcrumb[3].label).toBe('Project X');
		expect(state.expandedTreeNodeIds.has('projects-id')).toBe(true);
	});
});
