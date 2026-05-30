/**
 * Shared module root path resolver.
 *
 * Legacy module root policy:
 * - Read compatibility: listing operations check both legacy and canonical paths.
 * - Write canonical: all new creates write exclusively to /Workspace/<Module>.
 * - No duplicate roots: canonical root is reused if it already exists.
 * - No runtime migration of legacy data is required.
 */

export const WORKSPACE_ROOT = 'Workspace';

/** Legacy module roots that are supported for read-fallback only. */
export const LEGACY_MODULE_ROOTS = [
	'Notes',
	'Meetings',
	'Standups',
	'Decisions',
	'Kanban',
	'Brainstorming',
	'Shares'
] as const;

/**
 * Resolve a module name to its canonical root path.
 * Example: getModuleRoot("Notes") → "/Workspace/Notes"
 */
export function getModuleRoot(moduleName: string): string {
	return `/${WORKSPACE_ROOT}/${moduleName}`;
}

/**
 * Legacy root path for a module (directly under root).
 * Used for read-fallback only.
 * Example: getLegacyModuleRoot("Notes") → "/Notes"
 */
export function getLegacyModuleRoot(moduleName: string): string {
	return `/${moduleName}`;
}

/**
 * Canonical write path for a module.
 * Alias for getModuleRoot; explicitly documents write policy.
 */
export function getCanonicalWritePath(moduleName: string): string {
	return getModuleRoot(moduleName);
}

/**
 * Check whether a path is already under Workspace.
 */
export function isWorkspacePath(path: string): boolean {
	return path.startsWith(`/${WORKSPACE_ROOT}/`) || path === `/${WORKSPACE_ROOT}`;
}

/**
 * Check whether a path is a legacy module root (directly under root).
 */
export function isLegacyModuleRoot(path: string): boolean {
	const segments = path.replace(/^\//, '').split('/').filter(Boolean);
	if (segments.length !== 1) return false;
	return LEGACY_MODULE_ROOTS.includes(segments[0] as (typeof LEGACY_MODULE_ROOTS)[number]);
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
 * Return all paths that should be consulted for reading a module.
 * Includes canonical workspace path and legacy root path.
 */
export function getModuleReadPaths(moduleName: string): string[] {
	return [getCanonicalWritePath(moduleName), getLegacyModuleRoot(moduleName)];
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
