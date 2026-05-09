/**
 * ==============================================================================
 * SHARED BREADCRUMB UTILITIES
 * ==============================================================================
 *
 * Centralized tree-walking and breadcrumb-building functions used by:
 * - files/+page.svelte (My Files / Shared breadcrumb paths)
 * - SidebarNav.svelte (tree ancestor highlighting / expansion)
 * - explorerStore (canonical breadcrumb item construction)
 * - shared-with-me/[type]/[id]/+page.svelte (shared folder nested paths)
 */

import type { FolderTree as FolderTreeType } from '$lib/api/folders';
import type { Folder } from '$lib/api/types';
import type { ExplorerRoot, BreadcrumbItem } from './types';
import { ROOT_CONFIG } from './types';

// ============================================================================
// TREE WALKING
// ============================================================================

/**
 * Find the path from the root of a tree to a target folder.
 * Returns an array of Folder objects from root to target (inclusive).
 */
export function findFolderPathInTree(tree: FolderTreeType, targetId: string): Folder[] {
	function search(node: FolderTreeType): Folder[] {
		if (node.folder.id === targetId) {
			return [node.folder];
		}
		if (node.subfolders) {
			for (const child of node.subfolders) {
				const path = search(child);
				if (path.length > 0) {
					return [node.folder, ...path];
				}
			}
		}
		return [];
	}
	return search(tree);
}

/**
 * Search multiple trees for a target folder path.
 * Returns the first matching path found.
 */
export function findFolderPathInSharedTrees(targetId: string, trees: FolderTreeType[]): Folder[] {
	for (const tree of trees) {
		const path = findFolderPathInTree(tree, targetId);
		if (path.length > 0) {
			return path;
		}
	}
	return [];
}

/**
 * Find all ancestor IDs of a target folder within a tree.
 * Returns a Set containing every folder ID on the path from root to target
 * (excluding the target itself).
 */
export function findAncestorIds(tree: FolderTreeType, targetId: string): Set<string> {
	const ancestors = new Set<string>();

	function findPath(node: FolderTreeType, target: string, path: string[]): boolean {
		if (node.folder.id === target) {
			path.forEach((id) => ancestors.add(id));
			return true;
		}
		if (node.subfolders) {
			for (const child of node.subfolders) {
				if (findPath(child, target, [...path, node.folder.id])) {
					return true;
				}
			}
		}
		return false;
	}

	findPath(tree, targetId, []);
	return ancestors;
}

/**
 * Expand all ancestor nodes on the path to a target folder.
 * Calls `expandFn` for each ancestor (excluding the target itself).
 * Returns true if the target was found.
 */
export function expandPathToFolder(
	tree: FolderTreeType,
	targetId: string,
	expandFn: (id: string) => void
): boolean {
	function findAndExpand(node: FolderTreeType, target: string): boolean {
		if (node.folder.id === target) {
			return true;
		}
		if (node.subfolders) {
			for (const child of node.subfolders) {
				if (findAndExpand(child, target)) {
					expandFn(node.folder.id);
					return true;
				}
			}
		}
		return false;
	}

	return findAndExpand(tree, targetId);
}

// ============================================================================
// BREADCRUMB CONSTRUCTION
// ============================================================================

/**
 * Build breadcrumb items from root type, folder names, and folder IDs.
 * This is the canonical breadcrumb builder used by explorerStore.
 */
export function buildBreadcrumbItems(
	root: ExplorerRoot,
	folderPath: string[],
	folderIds: string[]
): BreadcrumbItem[] {
	const items: BreadcrumbItem[] = [{ label: ROOT_CONFIG[root].label, rootType: root }];

	for (let i = 0; i < folderPath.length; i++) {
		items.push({
			label: folderPath[i],
			folderId: folderIds[i] || undefined
		});
	}

	return items;
}

/**
 * Build breadcrumb items from an array of Folder objects.
 * Useful when the full Folder objects are already available.
 */
export function buildBreadcrumbItemsFromFolderPath(
	root: ExplorerRoot,
	folderPath: Folder[]
): BreadcrumbItem[] {
	const items: BreadcrumbItem[] = [{ label: ROOT_CONFIG[root].label, rootType: root }];
	for (const folder of folderPath) {
		items.push({ label: folder.name, folderId: folder.id });
	}
	return items;
}
