/**
 * ==============================================================================
 * EXPLORER MODULE
 * ==============================================================================
 *
 * Unified file explorer for RustShare.
 *
 * This module provides the central architecture for the refactored file explorer
 * that treats "My Files" and "Shared" as two explorer roots inside one unified shell.
 *
 * Exports:
 * - Types: ExplorerRoot, CollectionView, CanonicalLocation, ExplorerState, etc.
 * - Store: explorerStore with navigation actions
 * - Utilities: Type guards, configuration objects
 */

// Types
export type {
	ExplorerRoot,
	ExplorerMode,
	CollectionView,
	CanonicalLocation,
	CanonicalLocationInput,
	ExplorerState,
	BreadcrumbItem,
	MountedSharedFolder,
	NonMountedSharedItem,
	CollectionItemMetadata,
	CollectionFile,
	CollectionFolder,
	NavigationEvent,
	RootConfig
} from './types';

// Type guards and utilities
export {
	isExplorerRoot,
	isCollectionView,
	isAtRoot,
	isValidCanonicalLocation,
	createDefaultExplorerState,
	ROOT_CONFIG,
	COLLECTION_CONFIG
} from './types';

// Store
export { explorerStore } from './store.svelte';
export type { ExplorerState as ExplorerStoreState } from './store.svelte';
