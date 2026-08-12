import { getFolderContents, getFolderTree } from '$lib/api/folders';
import { resolvePathInTree } from './applicationPaths';

export type ApplicationObjectType = 'file' | 'folder';

export interface ApplicationRootContents {
	folders: Awaited<ReturnType<typeof getFolderContents>>['folders'];
	files: Awaited<ReturnType<typeof getFolderContents>>['files'];
	current_folder: Awaited<ReturnType<typeof getFolderContents>>['folders'][number] | null;
}

export function getApplicationObjectHref(
	applicationId: string,
	objectType: ApplicationObjectType,
	objectId: string
): string {
	// Known Application IDs — route to their dedicated editor regardless of
	// whether the underlying storage is a file or a folder.
	const applicationRouteMap: Record<string, string> = {
		'io.elembra.notes': `/apps/notes/${objectId}`,
		'io.elembra.meetings': `/apps/meetings/${objectId}`,
		'io.elembra.standups': `/apps/standups/${objectId}`,
		'io.elembra.decisions': `/apps/decisions/${objectId}`,
		'io.elembra.kanban': `/apps/kanban/${objectId}`,
		'io.elembra.brainstorming': `/apps/brainstorming/${objectId}`,
		'io.elembra.shares': `/apps/shares/${objectId}`,
		'io.elembra.mail': `/apps/mail/messages/${objectId}`,
		'io.elembra.chat': `/apps/chat?message=${objectId}`
	};

	if (applicationRouteMap[applicationId]) {
		return applicationRouteMap[applicationId];
	}

	if (objectType === 'folder') {
		return `/files?folder=${objectId}`;
	}

	return `/files?preview=${objectId}`;
}

/**
 * Resolve a module root path (e.g. "/Workspace/Notes") to the corresponding
 * folder ID by walking the folder tree.
 * Falls back to the legacy direct-root path if the Workspace path is missing.
 * Returns null if neither path exists.
 */
export async function resolveApplicationFolderId(rootPath: string): Promise<string | null> {
	if (!rootPath) return null;
	try {
		const tree = await getFolderTree();
		const node = resolvePathInTree(tree, rootPath);
		if (node) return node.folder.id;

		// Fallback: try legacy direct-root path (e.g. /Notes)
		const legacyName = rootPath.replace(/^\/Workspace\//, '/');
		if (legacyName !== rootPath) {
			const legacyNode = resolvePathInTree(tree, legacyName);
			if (legacyNode) return legacyNode.folder.id;
		}
		return null;
	} catch {
		return null;
	}
}

/**
 * Get the contents of a module root folder.
 * Supports nested paths like "/Workspace/Notes" by walking the tree.
 */
export async function getApplicationRootContents(
	rootPath: string
): Promise<ApplicationRootContents> {
	if (!rootPath) {
		return { folders: [], files: [], current_folder: null };
	}

	try {
		const tree = await getFolderTree();
		const node = resolvePathInTree(tree, rootPath);

		if (!node) {
			return { folders: [], files: [], current_folder: null };
		}

		const contents = await getFolderContents(node.folder.id);
		return {
			...contents,
			current_folder: node.folder
		};
	} catch {
		// Fallback: try single-level lookup for legacy paths
		const rootName = rootPath.replace(/^\//, '');
		const rootContents = await getFolderContents(null);
		const folder = rootContents.folders?.find((item) => item.name === rootName) ?? null;

		if (!folder) {
			return { folders: [], files: [], current_folder: null };
		}

		const contents = await getFolderContents(folder.id);
		return {
			...contents,
			current_folder: folder
		};
	}
}
