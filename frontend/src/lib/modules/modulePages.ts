import { getFolderContents } from '$lib/api/folders';

export type ModuleObjectType = 'file' | 'folder';

export interface ModuleRootContents {
	folders: Awaited<ReturnType<typeof getFolderContents>>['folders'];
	files: Awaited<ReturnType<typeof getFolderContents>>['files'];
	current_folder: Awaited<ReturnType<typeof getFolderContents>>['folders'][number] | null;
}

export function getModuleObjectHref(
	moduleKey: string,
	objectType: ModuleObjectType,
	objectId: string
): string {
	if (moduleKey === 'notes' && objectType === 'file') {
		return `/notes/${objectId}`;
	}

	return objectType === 'folder' ? `/files?folder=${objectId}` : `/files?preview=${objectId}`;
}

export async function getModuleRootContents(rootPath: string): Promise<ModuleRootContents> {
	const rootContents = await getFolderContents(null);
	const rootName = rootPath.replace(/^\//, '');
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
