/**
 * ==============================================================================
 * EXPLORER DOMAIN TYPES
 * ==============================================================================
 * 
 * Central type definitions for the unified file explorer architecture.
 * 
 * Concepts:
 * - ExplorerRoot: A traversable filesystem hierarchy (my-files, shared)
 * - CollectionView: A virtual aggregation (starred, recent, photos)
 * - CanonicalLocation: The real address of any item in the filesystem
 */

// ============================================================================
// ROOT AND COLLECTION DEFINITIONS
// ============================================================================

/**
 * ExplorerRoot represents a top-level mounted hierarchy that can be traversed
 * like a normal directory tree. These are the only valid roots.
 */
export type ExplorerRoot = 'my-files' | 'shared';

/**
 * CollectionView represents a virtual list that aggregates items but does not
 * define canonical filesystem location. These are not traversable roots.
 */
export type CollectionView = 'starred' | 'recent' | 'photos';

/**
 * ExplorerMode distinguishes between folder navigation and collection views.
 */
export type ExplorerMode = 'folder' | 'collection';

// ============================================================================
// CANONICAL LOCATION
// ============================================================================

/**
 * CanonicalLocation represents the real explorer address of an item.
 * Every file/folder in a collection must expose enough metadata to resolve
 * its real location using this structure.
 */
export interface CanonicalLocation {
	/** The root type this item belongs to */
	rootType: ExplorerRoot;
	
	/** The folder ID containing this item */
	folderId: string;
	
	/** The path from root to this folder as an array of folder names */
	folderPath: string[];
	
	/** IDs of all ancestor folders from root to parent */
	ancestorFolderIds: string[];
	
	/** The item ID (if referencing a specific file/folder) */
	itemId?: string;
	
	/** The item type (if referencing a specific item) */
	itemType?: 'file' | 'folder';
}

/**
 * CanonicalLocationInput is used when navigating to a location.
 * It can be partial and will be resolved to a full CanonicalLocation.
 */
export interface CanonicalLocationInput {
	rootType: ExplorerRoot;
	folderId: string;
	itemId?: string;
	itemType?: 'file' | 'folder';
}

// ============================================================================
// SHARED FOLDER TYPES
// ============================================================================

/**
 * MountedSharedFolder represents a shared folder that is part of the user's
 * navigable hierarchy and should appear in the Shared tree.
 */
export interface MountedSharedFolder {
	id: string;
	name: string;
	path: string;
	parent_folder_id: string | null;
	owner_id: string;
	shared_by: string;
	shared_by_name: string;
	permission: 'View' | 'Edit' | 'Admin';
	created_at: string;
	children?: MountedSharedFolder[];
}

/**
 * NonMountedSharedItem represents items that should NOT appear as regular
 * tree nodes unless the backend explicitly exposes them as mounted folders.
 * Examples: single-file shares, link-only shares, pending invites.
 */
export interface NonMountedSharedItem {
	share_id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	resource_name: string;
	shared_by: string;
	shared_by_name: string;
	permission: 'View' | 'Edit' | 'Admin';
	is_mounted: false;
}

// ============================================================================
// EXPLORER STATE
// ============================================================================

/**
 * BreadcrumbItem represents a single step in the breadcrumb navigation.
 */
export interface BreadcrumbItem {
	/** Display label */
	label: string;
	
	/** Folder ID for navigation (undefined for root) */
	folderId?: string;
	
	/** Root type for root-level items */
	rootType?: ExplorerRoot;
}

/**
 * ExplorerState is the central state model for the unified explorer.
 * All navigation, selection, and view state is managed through this structure.
 */
export interface ExplorerState {
	/** Current mode: folder navigation or collection view */
	mode: ExplorerMode;
	
	/** Currently active root (my-files or shared) */
	activeRoot: ExplorerRoot;
	
	/** Currently active collection (null when in folder mode) */
	activeCollection: CollectionView | null;
	
	/** Current folder ID being viewed (null at root) */
	currentFolderId: string | null;
	
	/** Path from root to current folder as folder names */
	currentFolderPath: string[];
	
	/** Currently selected item ID */
	selectedItemId: string | null;
	
	/** Type of selected item */
	selectedItemType: 'file' | 'folder' | null;
	
	/** Set of expanded tree node IDs */
	expandedTreeNodeIds: Set<string>;
	
	/** Breadcrumb path for display */
	breadcrumb: BreadcrumbItem[];
}

// ============================================================================
// COLLECTION ITEM METADATA
// ============================================================================

/**
 * CollectionItemMetadata must be attached to every item rendered in a
 * collection view. This enables navigation to the item's canonical location.
 */
export interface CollectionItemMetadata {
	/** The canonical location of this item */
	canonicalLocation: CanonicalLocation;
	
	/** When this item was starred (for starred collection) */
	starredAt?: string;
	
	/** When this item was last modified (for recent collection) */
	modifiedAt?: string;
}

/**
 * Extended File type with collection metadata.
 */
export interface CollectionFile {
	id: string;
	name: string;
	path: string;
	mime_type: string;
	size: number;
	parent_folder_id: string;
	owner_id: string;
	created_at: string;
	modified_at: string;
	/** Collection navigation metadata - REQUIRED */
	collectionMeta: CollectionItemMetadata;
}

/**
 * Extended Folder type with collection metadata.
 */
export interface CollectionFolder {
	id: string;
	name: string;
	path: string;
	parent_folder_id: string | null;
	owner_id: string;
	created_at: string;
	updated_at: string;
	/** Collection navigation metadata - REQUIRED */
	collectionMeta: CollectionItemMetadata;
}

// ============================================================================
// NAVIGATION EVENTS
// ============================================================================

/**
 * NavigationEvent represents a user navigation action.
 */
export type NavigationEvent =
	| { type: 'activateRoot'; root: ExplorerRoot }
	| { type: 'activateCollection'; collection: CollectionView }
	| { type: 'openFolder'; location: CanonicalLocation }
	| { type: 'openFileLocation'; location: CanonicalLocation }
	| { type: 'toggleTreeNode'; folderId: string };

// ============================================================================
// TYPE GUARDS
// ============================================================================

/**
 * Check if a value is a valid ExplorerRoot.
 */
export function isExplorerRoot(value: unknown): value is ExplorerRoot {
	return value === 'my-files' || value === 'shared';
}

/**
 * Check if a value is a valid CollectionView.
 */
export function isCollectionView(value: unknown): value is CollectionView {
	return value === 'starred' || value === 'recent' || value === 'photos';
}

/**
 * Check if a location is at the root level.
 */
export function isAtRoot(location: CanonicalLocation): boolean {
	return location.folderId === null || location.ancestorFolderIds.length === 0;
}

/**
 * Validate that a CanonicalLocation has all required fields.
 */
export function isValidCanonicalLocation(location: unknown): location is CanonicalLocation {
	if (typeof location !== 'object' || location === null) return false;
	const loc = location as Partial<CanonicalLocation>;
	
	return (
		isExplorerRoot(loc.rootType) &&
		typeof loc.folderId === 'string' &&
		Array.isArray(loc.folderPath) &&
		Array.isArray(loc.ancestorFolderIds) &&
		loc.folderPath.every(p => typeof p === 'string') &&
		loc.ancestorFolderIds.every(id => typeof id === 'string')
	);
}

// ============================================================================
// DEFAULT STATE
// ============================================================================

/**
 * Create the default initial explorer state.
 */
export function createDefaultExplorerState(): ExplorerState {
	return {
		mode: 'folder',
		activeRoot: 'my-files',
		activeCollection: null,
		currentFolderId: null,
		currentFolderPath: [],
		selectedItemId: null,
		selectedItemType: null,
		expandedTreeNodeIds: new Set(),
		breadcrumb: [{ label: 'My Files', rootType: 'my-files' }]
	};
}

// ============================================================================
// ROOT CONFIGURATION
// ============================================================================

/**
 * Root configuration for display purposes.
 */
export interface RootConfig {
	id: ExplorerRoot;
	label: string;
	icon: 'folder' | 'shared';
	rootFolderId: string | null;
}

/**
 * Configuration for all roots.
 */
export const ROOT_CONFIG: Record<ExplorerRoot, RootConfig> = {
	'my-files': {
		id: 'my-files',
		label: 'My Files',
		icon: 'folder',
		rootFolderId: null // My Files root has null parent
	},
	'shared': {
		id: 'shared',
		label: 'Shared',
		icon: 'shared',
		rootFolderId: 'shared-root' // Virtual root ID for shared
	}
};

/**
 * Configuration for all collections.
 */
export const COLLECTION_CONFIG: Record<CollectionView, { label: string; icon: string }> = {
	'starred': { label: 'Starred', icon: 'star' },
	'recent': { label: 'Recent', icon: 'clock' },
	'photos': { label: 'Photos', icon: 'image' }
};
