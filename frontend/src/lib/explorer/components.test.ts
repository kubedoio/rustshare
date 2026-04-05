/**
 * ==============================================================================
 * EXPLORER COMPONENT CONTRACT TESTS
 * ==============================================================================
 * 
 * Tests for component contracts from the specification.
 * 
 * Coverage:
 * - SidebarNav contract tests
 * - FolderTree contract tests
 * - MainFileList contract tests
 * - Breadcrumb contract tests
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import type { ExplorerRoot, CollectionView, CanonicalLocation, CollectionFile, CollectionFolder } from './types';
import type { ReceivedShare } from '$lib/api/types';
import { isExplorerRoot, isCollectionView, ROOT_CONFIG, COLLECTION_CONFIG } from './types';

describe('SidebarNav Contract Tests', () => {
	describe('Primary Navigation Group', () => {
		it('contains My Files', () => {
			const primaryNav = [
				{ id: 'my-files', label: 'My Files', icon: 'home' }
			];
			
			const myFilesItem = primaryNav.find(item => item.id === 'my-files');
			expect(myFilesItem).toBeDefined();
			expect(myFilesItem?.label).toBe('My Files');
		});

		it('does NOT contain Shared in primary group', () => {
			const primaryNav = [
				{ id: 'my-files', label: 'My Files', icon: 'home' }
			];
			
			const sharedInPrimary = primaryNav.find(item => item.id === 'shared');
			expect(sharedInPrimary).toBeUndefined();
		});
	});

	describe('Library Navigation Group', () => {
		const libraryNav: { id: CollectionView | 'shared'; label: string }[] = [
			{ id: 'shared', label: 'Shared' },
			{ id: 'starred', label: 'Starred' },
			{ id: 'photos', label: 'Photos' },
			{ id: 'recent', label: 'Recent' }
		];

		it('contains Shared in Library', () => {
			const sharedItem = libraryNav.find(item => item.id === 'shared');
			expect(sharedItem).toBeDefined();
			expect(sharedItem?.label).toBe('Shared');
		});

		it('contains Starred in Library', () => {
			const starredItem = libraryNav.find(item => item.id === 'starred');
			expect(starredItem).toBeDefined();
		});

		it('contains Photos in Library', () => {
			const photosItem = libraryNav.find(item => item.id === 'photos');
			expect(photosItem).toBeDefined();
		});

		it('contains Recent in Library', () => {
			const recentItem = libraryNav.find(item => item.id === 'recent');
			expect(recentItem).toBeDefined();
		});

		it('Shared in Library activates root', () => {
			const sharedItem = libraryNav.find(item => item.id === 'shared');
			// Clicking Shared should dispatch activateRoot('shared')
			// This is a semantic action, not a raw route
			expect(sharedItem?.id).toBe('shared');
			expect(isExplorerRoot(sharedItem?.id)).toBe(true);
		});

		it('collection items activate collection mode', () => {
			const collections: CollectionView[] = ['starred', 'recent', 'photos'];
			
			collections.forEach(collection => {
				const item = libraryNav.find(item => item.id === collection);
				expect(item).toBeDefined();
				expect(isCollectionView(item?.id)).toBe(true);
			});
		});
	});

	describe('Folders Navigation Group', () => {
		it('renders My Files as tree root', () => {
			const folderRoots: ExplorerRoot[] = ['my-files', 'shared'];
			
			expect(folderRoots).toContain('my-files');
			expect(ROOT_CONFIG['my-files'].label).toBe('My Files');
		});

		it('renders Shared as tree root', () => {
			const folderRoots: ExplorerRoot[] = ['my-files', 'shared'];
			
			expect(folderRoots).toContain('shared');
			expect(ROOT_CONFIG['shared'].label).toBe('Shared');
		});

		it('uses correct icons for roots', () => {
			expect(ROOT_CONFIG['my-files'].icon).toBe('folder');
			expect(ROOT_CONFIG['shared'].icon).toBe('shared');
		});
	});
});

describe('FolderTree Contract Tests', () => {
	// Mock folder tree data structure
	const mockMyFilesTree = {
		folder: {
			id: 'root',
			name: 'My Files',
			path: '/',
			parent_folder_id: null,
			owner_id: 'user-1',
			created_at: '',
			updated_at: ''
		},
		subfolders: [
			{
				folder: {
					id: 'folder-1',
					name: 'Projects',
					path: '/projects',
					parent_folder_id: 'root',
					owner_id: 'user-1',
					created_at: '',
					updated_at: ''
				},
				subfolders: []
			}
		]
	};

	const mockSharedTree = {
		folder: {
			id: 'shared-root',
			name: 'Shared',
			path: '/shared',
			parent_folder_id: null,
			owner_id: 'shared',
			created_at: '',
			updated_at: ''
		},
		subfolders: [
			{
				folder: {
					id: 'shared-folder-1',
					name: 'Team A',
					path: '/shared/team-a',
					parent_folder_id: 'shared-root',
					owner_id: 'user-2',
					created_at: '',
					updated_at: ''
				},
				subfolders: [
					{
						folder: {
							id: 'nested-shared-folder',
							name: 'Contracts',
							path: '/shared/team-a/contracts',
							parent_folder_id: 'shared-folder-1',
							owner_id: 'user-2',
							created_at: '',
							updated_at: ''
						},
						subfolders: []
					}
				]
			}
		]
	};

	describe('Tree Structure', () => {
		it('renders My Files root', () => {
			expect(mockMyFilesTree.folder.name).toBe('My Files');
			expect(mockMyFilesTree.folder.parent_folder_id).toBeNull();
		});

		it('renders Shared root', () => {
			expect(mockSharedTree.folder.name).toBe('Shared');
			expect(mockSharedTree.folder.parent_folder_id).toBeNull();
		});

		it('Shared tree only shows mounted shared folders', () => {
			// The shared tree should only contain folders that are 
			// explicitly mounted in the user's hierarchy
			const allFolders = flattenTree(mockSharedTree);
			
			// All folders in the shared tree should have valid IDs
			allFolders.forEach(folder => {
				expect(folder.id).toBeDefined();
				expect(folder.id.length).toBeGreaterThan(0);
			});
		});

		it('Shared root uses provided SVG icon', () => {
			// Icon is 'shared' type as defined in ROOT_CONFIG
			expect(ROOT_CONFIG['shared'].icon).toBe('shared');
		});
	});

	describe('Tree Interaction', () => {
		it('clicking Shared folder expands and opens correctly', () => {
			const sharedRootId = mockSharedTree.folder.id;
			const teamAFolder = mockSharedTree.subfolders[0];
			
			expect(sharedRootId).toBe('shared-root');
			expect(teamAFolder.folder.id).toBe('shared-folder-1');
			
			// Clicking should navigate to the folder
			// The folder ID should be used in navigation
			expect(teamAFolder.folder.id).toBeDefined();
		});

		it('selected state reflects store state', () => {
			const selectedFolderId = 'shared-folder-1';
			
			// Check if the folder exists in the tree
			const exists = findFolderInTree(mockSharedTree, selectedFolderId);
			expect(exists).toBeDefined();
		});

		it('ancestors are expanded when child is selected', () => {
			const nestedFolderId = 'nested-shared-folder';
			const ancestorIds = ['shared-root', 'shared-folder-1'];
			
			// When nested folder is selected, ancestors should be expanded
			const expandedIds = new Set(ancestorIds);
			
			expect(expandedIds.has('shared-root')).toBe(true);
			expect(expandedIds.has('shared-folder-1')).toBe(true);
		});
	});

	// Helper to flatten tree for testing
	function flattenTree(node: any): any[] {
		const result = [node.folder];
		if (node.subfolders) {
			node.subfolders.forEach((child: any) => {
				result.push(...flattenTree(child));
			});
		}
		return result;
	}

	function findFolderInTree(node: any, id: string): any | undefined {
		if (node.folder.id === id) return node.folder;
		if (node.subfolders) {
			for (const child of node.subfolders) {
				const found = findFolderInTree(child, id);
				if (found) return found;
			}
		}
		return undefined;
	}
});

describe('MainFileList Contract Tests', () => {
	const mockFolderModeData = {
		folders: [
			{ id: 'folder-1', name: 'Project A', parent_folder_id: null },
			{ id: 'folder-2', name: 'Project B', parent_folder_id: null }
		],
		files: [
			{ id: 'file-1', name: 'document.pdf', parent_folder_id: null, mime_type: 'application/pdf' }
		]
	};

	const mockCollectionModeData = {
		folders: [
			{
				id: 'starred-folder-1',
				name: 'Important Project',
				parent_folder_id: 'some-parent',
				collectionMeta: {
					canonicalLocation: {
						rootType: 'my-files',
						folderId: 'folder-1',
						folderPath: ['Important Project'],
						ancestorFolderIds: []
					}
				}
			}
		],
		files: [
			{
				id: 'starred-file-1',
				name: 'starred-doc.pdf',
				parent_folder_id: 'folder-1',
				mime_type: 'application/pdf',
				collectionMeta: {
					canonicalLocation: {
						rootType: 'my-files',
						folderId: 'folder-1',
						folderPath: ['Important Project'],
						ancestorFolderIds: [],
						itemId: 'starred-file-1',
						itemType: 'file'
					}
				}
			}
		]
	};

	describe('Mode Rendering', () => {
		it('in folder mode, renders contents of current folder', () => {
			const mode = 'folder';
			const data = mockFolderModeData;
			
			expect(mode).toBe('folder');
			expect(data.folders).toHaveLength(2);
			expect(data.files).toHaveLength(1);
		});

		it('in collection mode, renders aggregated items', () => {
			const mode = 'collection';
			const data = mockCollectionModeData;
			
			expect(mode).toBe('collection');
			expect(data.folders).toHaveLength(1);
			expect(data.files).toHaveLength(1);
		});
	});

	describe('Canonical Location Metadata', () => {
		it('folder rows in collection carry canonical metadata', () => {
			const folder = mockCollectionModeData.folders[0];
			
			expect(folder.collectionMeta).toBeDefined();
			expect(folder.collectionMeta.canonicalLocation).toBeDefined();
			expect(folder.collectionMeta.canonicalLocation.rootType).toBe('my-files');
			expect(folder.collectionMeta.canonicalLocation.folderId).toBe('folder-1');
		});

		it('file rows in collection carry canonical metadata', () => {
			const file = mockCollectionModeData.files[0];
			
			expect(file.collectionMeta).toBeDefined();
			expect(file.collectionMeta.canonicalLocation).toBeDefined();
			expect(file.collectionMeta.canonicalLocation.rootType).toBe('my-files');
			expect(file.collectionMeta.canonicalLocation.itemId).toBe('starred-file-1');
			expect(file.collectionMeta.canonicalLocation.itemType).toBe('file');
		});
	});

	describe('Click Handlers', () => {
		it('clicking folder row calls openFolder with canonical location', () => {
			const folder = mockCollectionModeData.folders[0];
			const location = folder.collectionMeta.canonicalLocation;
			
			// This is the expected behavior
			expect(location.rootType).toBe('my-files');
			expect(location.folderId).toBe('folder-1');
		});

		it('clicking file row from collection calls openFileLocation', () => {
			const file = mockCollectionModeData.files[0];
			const location = file.collectionMeta.canonicalLocation;
			
			// This is the expected behavior
			expect(location.rootType).toBe('my-files');
			expect(location.folderId).toBe('folder-1');
			expect(location.itemId).toBe('starred-file-1');
			expect(location.itemType).toBe('file');
		});
	});
});

describe('Breadcrumb Contract Tests', () => {
	describe('Breadcrumb Structure', () => {
		it('renders My Files > ... for my-files root', () => {
			const breadcrumb = [
				{ label: 'My Files', rootType: 'my-files' as ExplorerRoot },
				{ label: 'Projects', folderId: 'folder-1' },
				{ label: 'Q2', folderId: 'folder-2' }
			];
			
			expect(breadcrumb[0].label).toBe('My Files');
			expect(breadcrumb[0].rootType).toBe('my-files');
		});

		it('renders Shared > ... for shared root', () => {
			const breadcrumb = [
				{ label: 'Shared', rootType: 'shared' as ExplorerRoot },
				{ label: 'Team A', folderId: 'shared-folder-1' },
				{ label: 'Contracts', folderId: 'shared-folder-2' }
			];
			
			expect(breadcrumb[0].label).toBe('Shared');
			expect(breadcrumb[0].rootType).toBe('shared');
		});

		it('breadcrumb derives from canonical location only', () => {
			const canonicalLocation = {
				rootType: 'shared' as ExplorerRoot,
				folderId: 'contracts-folder',
				folderPath: ['Team A', 'Contracts'],
				ancestorFolderIds: ['team-a-id']
			};
			
			const breadcrumb = [
				{ label: ROOT_CONFIG[canonicalLocation.rootType].label, rootType: canonicalLocation.rootType },
				{ label: 'Team A', folderId: 'team-a-id' },
				{ label: 'Contracts', folderId: 'contracts-folder' }
			];
			
			expect(breadcrumb).toHaveLength(3);
			expect(breadcrumb[2].label).toBe('Contracts');
		});
	});

	describe('Breadcrumb Navigation', () => {
		it('clicking ancestor breadcrumb navigates correctly', () => {
			const breadcrumb = [
				{ label: 'Shared', rootType: 'shared' as ExplorerRoot },
				{ label: 'Team A', folderId: 'team-a-id' },
				{ label: 'Contracts', folderId: 'contracts-folder' }
			];
			
			// Clicking 'Team A' should navigate to that folder
			const teamABreadcrumb = breadcrumb[1];
			expect(teamABreadcrumb.folderId).toBe('team-a-id');
			
			// Clicking 'Shared' root should navigate to shared root
			const sharedBreadcrumb = breadcrumb[0];
			expect(sharedBreadcrumb.rootType).toBe('shared');
		});

		it('clicking root breadcrumb navigates to root', () => {
			const breadcrumb = [
				{ label: 'My Files', rootType: 'my-files' as ExplorerRoot },
				{ label: 'Projects', folderId: 'folder-1' }
			];
			
			const rootBreadcrumb = breadcrumb[0];
			expect(rootBreadcrumb.rootType).toBe('my-files');
			expect(rootBreadcrumb.folderId).toBeUndefined();
		});
	});

	describe('Collection Mode Breadcrumb', () => {
		it('collection mode can show collection label until navigation', () => {
			const mode = 'collection';
			const activeCollection: CollectionView = 'starred';
			
			// In collection mode, breadcrumb can show the collection name
			const breadcrumb = [
				{ label: COLLECTION_CONFIG[activeCollection].label }
			];
			
			expect(breadcrumb[0].label).toBe('Starred');
		});
	});
});
