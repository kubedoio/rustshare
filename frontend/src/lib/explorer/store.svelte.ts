/**
 * ==============================================================================
 * EXPLORER STORE / CONTROLLER
 * ==============================================================================
 * 
 * Central navigation controller for the unified file explorer.
 * 
 * This store implements the contracts defined in the specification:
 * - activateRoot(rootType): Switch to a root (my-files or shared)
 * - activateCollection(view): Switch to a collection view
 * - openFolder(location): Navigate to a specific folder
 * - openFileLocation(location): Navigate to a file's parent folder and select it
 * - toggleTreeNode(folderId): Toggle tree expansion without navigation
 * - resolveCollectionItem(itemId): Resolve an item's canonical location
 * 
 * Non-negotiable rule:
 * There must be one central navigation resolver that turns any click from any 
 * view into canonical explorer state.
 */

import { writable, derived, get } from 'svelte/store';
import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import type {
	ExplorerState,
	ExplorerRoot,
	ExplorerMode,
	CollectionView,
	CanonicalLocation,
	CanonicalLocationInput,
	BreadcrumbItem,
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

// ============================================================================
// STORE CREATION
// ============================================================================

function createExplorerStore() {
	// Internal state
	const state = writable<ExplorerState>(createDefaultExplorerState());
	
	// Derived stores for convenience
	const mode = derived(state, $s => $s.mode);
	const activeRoot = derived(state, $s => $s.activeRoot);
	const activeCollection = derived(state, $s => $s.activeCollection);
	const currentFolderId = derived(state, $s => $s.currentFolderId);
	const currentFolderPath = derived(state, $s => $s.currentFolderPath);
	const selectedItemId = derived(state, $s => $s.selectedItemId);
	const selectedItemType = derived(state, $s => $s.selectedItemType);
	const expandedTreeNodeIds = derived(state, $s => $s.expandedTreeNodeIds);
	const breadcrumb = derived(state, $s => $s.breadcrumb);
	const isAtRoot = derived(state, $s => $s.currentFolderId === null);

	// ============================================================================
	// PRIVATE HELPERS
	// ============================================================================

	/**
	 * Build breadcrumb from current state.
	 */
	function buildBreadcrumb(
		root: ExplorerRoot,
		folderPath: string[],
		folderIds: string[]
	): BreadcrumbItem[] {
		const items: BreadcrumbItem[] = [
			{ label: ROOT_CONFIG[root].label, rootType: root }
		];

		for (let i = 0; i < folderPath.length; i++) {
			items.push({
				label: folderPath[i],
				folderId: folderIds[i] || undefined
			});
		}

		return items;
	}

	/**
	 * Expand all ancestors in the tree.
	 */
	function expandAncestors(ancestorIds: string[]): Set<string> {
		return new Set(ancestorIds);
	}

	/**
	 * Navigate to the corresponding URL for the current state.
	 */
	function navigateToState(state: ExplorerState, options?: { replaceState?: boolean }) {
		if (!browser) return;

		let url: string;

		if (state.mode === 'collection' && state.activeCollection) {
			// Collection mode: /files?filter=<collection>
			url = `/files?filter=${state.activeCollection}`;
		} else {
			// Folder mode: /files?folder=<id> or /files
			if (state.currentFolderId) {
				url = `/files?folder=${state.currentFolderId}`;
			} else {
				// At root - use root-specific URL
				if (state.activeRoot === 'shared') {
					url = '/files?root=shared';
				} else {
					url = '/files';
				}
			}
		}

		goto(url, { replaceState: options?.replaceState ?? false });
	}

	// ============================================================================
	// CONTRACT IMPLEMENTATION: ACTION 1
	// ============================================================================

	/**
	 * activateRoot - Contract A
	 * 
	 * Input: rootType ('my-files' | 'shared')
	 * Output: 
	 * - explorer enters folder mode
	 * - activeRoot is set
	 * - activeCollection cleared
	 * - root contents loaded (via URL change)
	 */
	function activateRoot(rootType: ExplorerRoot, options?: { replaceState?: boolean }) {
		if (!isExplorerRoot(rootType)) {
			console.error(`[explorerStore] Invalid root type: ${rootType}`);
			return;
		}

		state.update(s => {
			const newState: ExplorerState = {
				...s,
				mode: 'folder',
				activeRoot: rootType,
				activeCollection: null,
				currentFolderId: null,
				currentFolderPath: [],
				selectedItemId: null,
				selectedItemType: null,
				breadcrumb: [{ label: ROOT_CONFIG[rootType].label, rootType }]
			};

			// Navigate to the corresponding URL
			navigateToState(newState, options);

			return newState;
		});
	}

	// ============================================================================
	// CONTRACT IMPLEMENTATION: ACTION 2
	// ============================================================================

	/**
	 * activateCollection - Contract B
	 * 
	 * Input: collection name ('starred' | 'recent' | 'photos')
	 * Output:
	 * - explorer enters collection mode
	 * - activeCollection set
	 * - no tree node is falsely highlighted as current folder
	 */
	function activateCollection(collection: CollectionView, options?: { replaceState?: boolean }) {
		if (!isCollectionView(collection)) {
			console.error(`[explorerStore] Invalid collection: ${collection}`);
			return;
		}

		state.update(s => {
			const newState: ExplorerState = {
				...s,
				mode: 'collection',
				activeCollection: collection,
				// Clear folder selection so no tree node is falsely highlighted
				currentFolderId: null,
				currentFolderPath: [],
				selectedItemId: null,
				selectedItemType: null,
				breadcrumb: [{ label: 'Library', rootType: s.activeRoot }]
			};

			// Navigate to the corresponding URL
			navigateToState(newState, options);

			return newState;
		});
	}

	// ============================================================================
	// CONTRACT IMPLEMENTATION: ACTION 3
	// ============================================================================

	/**
	 * openFolder - Contract C
	 * 
	 * Input: canonical location for folder
	 * Output:
	 * - mode becomes folder
	 * - activeRoot set from location
	 * - activeCollection cleared
	 * - ancestors expanded
	 * - current folder set
	 * - selected item becomes folder
	 * - breadcrumb rebuilt
	 * - folder contents loaded (via URL change)
	 */
	function openFolder(location: CanonicalLocation, options?: { replaceState?: boolean }) {
		if (!isValidCanonicalLocation(location)) {
			console.error('[explorerStore] Invalid canonical location:', location);
			return;
		}

		state.update(s => {
			const newExpandedIds = new Set(s.expandedTreeNodeIds);
			
			// Expand all ancestors
			location.ancestorFolderIds.forEach(id => newExpandedIds.add(id));

			const newState: ExplorerState = {
				...s,
				mode: 'folder',
				activeRoot: location.rootType,
				activeCollection: null,
				currentFolderId: location.folderId,
				currentFolderPath: location.folderPath,
				expandedTreeNodeIds: newExpandedIds,
				// Select the folder itself
				selectedItemId: location.itemId || location.folderId,
				selectedItemType: 'folder',
				breadcrumb: buildBreadcrumb(
					location.rootType,
					location.folderPath,
					location.ancestorFolderIds
				)
			};

			// Navigate to the corresponding URL
			navigateToState(newState, options);

			return newState;
		});
	}

	// ============================================================================
	// CONTRACT IMPLEMENTATION: ACTION 4
	// ============================================================================

	/**
	 * openFileLocation - Contract D
	 * 
	 * Input: canonical location for file
	 * Output:
	 * - mode becomes folder
	 * - activeRoot set from location
	 * - activeCollection cleared
	 * - ancestors expanded
	 * - parent folder opened
	 * - selected item becomes file
	 * - breadcrumb rebuilt using parent folder
	 * - file visible/selected in main panel
	 */
	function openFileLocation(location: CanonicalLocation, options?: { replaceState?: boolean }) {
		if (!isValidCanonicalLocation(location)) {
			console.error('[explorerStore] Invalid canonical location:', location);
			return;
		}

		if (!location.itemId || location.itemType !== 'file') {
			console.error('[explorerStore] openFileLocation requires a file itemId and itemType:"file"');
			return;
		}

		state.update(s => {
			const newExpandedIds = new Set(s.expandedTreeNodeIds);
			
			// Expand all ancestors (including the folder containing the file)
			location.ancestorFolderIds.forEach(id => newExpandedIds.add(id));
			newExpandedIds.add(location.folderId);

			const newState: ExplorerState = {
				...s,
				mode: 'folder',
				activeRoot: location.rootType,
				activeCollection: null,
				currentFolderId: location.folderId,
				currentFolderPath: location.folderPath,
				expandedTreeNodeIds: newExpandedIds,
				// Select the file
				selectedItemId: location.itemId ?? null,
				selectedItemType: 'file',
				breadcrumb: buildBreadcrumb(
					location.rootType,
					location.folderPath,
					location.ancestorFolderIds
				)
			};

			// Navigate to the corresponding URL
			navigateToState(newState, options);

			return newState;
		});
	}

	// ============================================================================
	// CONTRACT IMPLEMENTATION: ACTION 5
	// ============================================================================

	/**
	 * toggleTreeNode - Contract E
	 * 
	 * Input: folderId
	 * Output: only expansion state changes, no navigation
	 */
	function toggleTreeNode(folderId: string) {
		state.update(s => {
			const newExpandedIds = new Set(s.expandedTreeNodeIds);
			if (newExpandedIds.has(folderId)) {
				newExpandedIds.delete(folderId);
			} else {
				newExpandedIds.add(folderId);
			}
			return {
				...s,
				expandedTreeNodeIds: newExpandedIds
			};
		});
	}

	/**
	 * expandTreeNode - Expand a specific tree node without navigation
	 */
	function expandTreeNode(folderId: string) {
		state.update(s => {
			const newExpandedIds = new Set(s.expandedTreeNodeIds);
			newExpandedIds.add(folderId);
			return {
				...s,
				expandedTreeNodeIds: newExpandedIds
			};
		});
	}

	/**
	 * collapseTreeNode - Collapse a specific tree node without navigation
	 */
	function collapseTreeNode(folderId: string) {
		state.update(s => {
			const newExpandedIds = new Set(s.expandedTreeNodeIds);
			newExpandedIds.delete(folderId);
			return {
				...s,
				expandedTreeNodeIds: newExpandedIds
			};
		});
	}

	// ============================================================================
	// CONTRACT IMPLEMENTATION: ACTION 6
	// ============================================================================

	/**
	 * resolveCollectionItem - Contract F
	 * 
	 * Input: item from a collection (file or folder)
	 * Output: valid canonical location
	 * Failure state must be explicit and handled.
	 */
	function resolveCollectionItem(
		item: CollectionFile | CollectionFolder
	): CanonicalLocation | null {
		if (!item.collectionMeta?.canonicalLocation) {
			console.error('[explorerStore] Item missing collection metadata:', item);
			return null;
		}

		const location = item.collectionMeta.canonicalLocation;
		
		if (!isValidCanonicalLocation(location)) {
			console.error('[explorerStore] Invalid canonical location in item:', item);
			return null;
		}

		return location;
	}

	/**
	 * Resolve and navigate to a collection item.
	 * This is the central navigation resolver for collection clicks.
	 */
	function navigateToCollectionItem(
		item: CollectionFile | CollectionFolder,
		options?: { replaceState?: boolean }
	): boolean {
		const location = resolveCollectionItem(item);
		
		if (!location) {
			return false;
		}

		if (item.collectionMeta.canonicalLocation.itemType === 'file') {
			openFileLocation(location, options);
		} else {
			openFolder(location, options);
		}

		return true;
	}

	// ============================================================================
	// ADDITIONAL STATE MANAGEMENT
	// ============================================================================

	/**
	 * Clear selection.
	 */
	function clearSelection() {
		state.update(s => ({
			...s,
			selectedItemId: null,
			selectedItemType: null
		}));
	}

	/**
	 * Select a specific item.
	 */
	function selectItem(itemId: string, itemType: 'file' | 'folder') {
		state.update(s => ({
			...s,
			selectedItemId: itemId,
			selectedItemType: itemType
		}));
	}

	/**
	 * Reset the entire explorer state.
	 */
	function reset() {
		state.set(createDefaultExplorerState());
	}

	/**
	 * Hydrate state from URL parameters.
	 * Call this on page load to sync state with URL.
	 */
	function hydrateFromUrl(
		params: { 
			folder?: string | null; 
			filter?: string | null;
			root?: string | null;
		},
		folderPathData?: { path: string[]; ancestorIds: string[] }
	) {
		// Check for collection mode
		if (params.filter && isCollectionView(params.filter)) {
			state.update(s => ({
				...s,
				mode: 'collection',
				activeCollection: params.filter as CollectionView,
				currentFolderId: null,
				currentFolderPath: []
			}));
			return;
		}

		// Folder mode - determine root
		const rootType: ExplorerRoot = params.root === 'shared' ? 'shared' : 'my-files';
		
		if (params.folder) {
			// In a specific folder
			state.update(s => {
				const folderPath = folderPathData?.path || [];
				const ancestorIds = folderPathData?.ancestorIds || [];
				
				return {
					...s,
					mode: 'folder',
					activeRoot: rootType,
					activeCollection: null,
					currentFolderId: params.folder ?? null,
					currentFolderPath: folderPath,
					expandedTreeNodeIds: expandAncestors(ancestorIds),
					breadcrumb: buildBreadcrumb(rootType, folderPath, ancestorIds)
				};
			});
		} else {
			// At root
			state.update(s => ({
				...s,
				mode: 'folder',
				activeRoot: rootType,
				activeCollection: null,
				currentFolderId: null,
				currentFolderPath: [],
				breadcrumb: [{ label: ROOT_CONFIG[rootType].label, rootType }]
			}));
		}
	}

	// ============================================================================
	// STORE API
	// ============================================================================

	return {
		// Subscribe to state
		subscribe: state.subscribe,
		
		// Derived state
		mode,
		activeRoot,
		activeCollection,
		currentFolderId,
		currentFolderPath,
		selectedItemId,
		selectedItemType,
		expandedTreeNodeIds,
		breadcrumb,
		isAtRoot,
		
		// Core navigation actions (contracts)
		activateRoot,
		activateCollection,
		openFolder,
		openFileLocation,
		toggleTreeNode,
		expandTreeNode,
		collapseTreeNode,
		resolveCollectionItem,
		navigateToCollectionItem,
		
		// Additional state management
		clearSelection,
		selectItem,
		reset,
		hydrateFromUrl,
		
		// Get current state (for imperative access)
		getState: () => get(state)
	};
}

// ============================================================================
// SINGLETON EXPORT
// ============================================================================

export const explorerStore = createExplorerStore();

// Re-export types for convenience
export type { ExplorerState, CanonicalLocation, CollectionView, ExplorerRoot };
