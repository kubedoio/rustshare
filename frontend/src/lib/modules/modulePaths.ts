/**
 * Shared module root path resolver.
 *
 * All module-backed directories live under /Workspace.
 * Legacy paths (directly under root) are supported for read-fallback
 * but new data always writes to the Workspace subtree.
 */

export const WORKSPACE_ROOT = 'Workspace';

/**
 * Resolve a module name to its canonical root path.
 * Example: getModuleRoot("Notes") → "/Workspace/Notes"
 */
export function getModuleRoot(moduleName: string): string {
	return `/${WORKSPACE_ROOT}/${moduleName}`;
}

/**
 * Legacy root path for a module (directly under root).
 * Used for migration/fallback only.
 * Example: getLegacyModuleRoot("Notes") → "/Notes"
 */
export function getLegacyModuleRoot(moduleName: string): string {
	return `/${moduleName}`;
}

/**
 * Check whether a path is already under Workspace.
 */
export function isWorkspacePath(path: string): boolean {
	return path.startsWith(`/${WORKSPACE_ROOT}/`) || path === `/${WORKSPACE_ROOT}`;
}

/**
 * Given a module name, return both the new Workspace path and the legacy path.
 * Useful for queries that need to check both locations during migration.
 */
export function getModulePathVariants(moduleName: string): {
	workspace: string;
	legacy: string;
} {
	return {
		workspace: getModuleRoot(moduleName),
		legacy: getLegacyModuleRoot(moduleName)
	};
}

/**
 * Resolve a slash-separated folder path to a folder ID by walking the folder tree.
 * Returns null if any segment is missing.
 */
import type { FolderTree } from '$lib/api/folders';

export function resolvePathInTree(tree: FolderTree, path: string): FolderTree | null {
	const segments = path.replace(/^\//, '').split('/').filter(Boolean);
	if (segments.length === 0) return tree;

	let current: FolderTree = tree;
	for (const segment of segments) {
		// If the current tree node itself matches the segment, continue to children
		if (current.folder.name === segment) {
			continue;
		}
		const found = current.subfolders.find((sf) => sf.folder.name === segment);
		if (!found) return null;
		current = found;
	}
	return current;
}
