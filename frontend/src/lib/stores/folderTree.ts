import { writable, derived } from 'svelte/store';
import type { Folder } from '$lib/api/types';

export interface FolderNode extends Folder {
	children?: FolderNode[];
	isExpanded?: boolean;
	isLoading?: boolean;
}

interface FolderTreeState {
	rootFolders: FolderNode[];
	expandedIds: Set<string>;
	selectedId: string | null;
	loadingIds: Set<string>;
}

function createFolderTreeStore() {
	const { subscribe, set, update } = writable<FolderTreeState>({
		rootFolders: [],
		expandedIds: new Set(),
		selectedId: null,
		loadingIds: new Set()
	});

	return {
		subscribe,
		
		setRootFolders: (folders: Folder[]) => {
			update(state => ({
				...state,
				rootFolders: folders.map(f => ({ ...f, children: undefined, isExpanded: state.expandedIds.has(f.id) }))
			}));
		},

		selectFolder: (folderId: string | null) => {
			update(state => ({
				...state,
				selectedId: folderId
			}));
		},

		toggleExpand: (folderId: string) => {
			update(state => {
				const newExpanded = new Set(state.expandedIds);
				if (newExpanded.has(folderId)) {
					newExpanded.delete(folderId);
				} else {
					newExpanded.add(folderId);
				}
				return {
					...state,
					expandedIds: newExpanded
				};
			});
		},

		setExpanded: (folderId: string, expanded: boolean) => {
			update(state => {
				const newExpanded = new Set(state.expandedIds);
				if (expanded) {
					newExpanded.add(folderId);
				} else {
					newExpanded.delete(folderId);
				}
				return {
					...state,
					expandedIds: newExpanded
				};
			});
		},

		setFolderChildren: (folderId: string, children: Folder[]) => {
			update(state => ({
				...state,
				rootFolders: updateFolderChildren(state.rootFolders, folderId, children, state.expandedIds)
			}));
		},

		setLoading: (folderId: string, loading: boolean) => {
			update(state => {
				const newLoading = new Set(state.loadingIds);
				if (loading) {
					newLoading.add(folderId);
				} else {
					newLoading.delete(folderId);
				}
				return {
					...state,
					loadingIds: newLoading
				};
			});
		},

		reset: () => {
			set({
				rootFolders: [],
				expandedIds: new Set(),
				selectedId: null,
				loadingIds: new Set()
			});
		}
	};
}

function updateFolderChildren(
	folders: FolderNode[], 
	folderId: string, 
	children: Folder[],
	expandedIds: Set<string>
): FolderNode[] {
	return folders.map(folder => {
		if (folder.id === folderId) {
			return {
				...folder,
				children: children.map(c => ({ ...c, children: undefined, isExpanded: expandedIds.has(c.id) }))
			};
		}
		if (folder.children) {
			return {
				...folder,
				children: updateFolderChildren(folder.children, folderId, children, expandedIds)
			};
		}
		return folder;
	});
}

export const folderTreeStore = createFolderTreeStore();

// Derived store to get the current selected folder
export const selectedFolder = derived(
	folderTreeStore,
	$store => {
		if (!$store.selectedId) return null;
		return findFolderById($store.rootFolders, $store.selectedId);
	}
);

function findFolderById(folders: FolderNode[], id: string): FolderNode | null {
	for (const folder of folders) {
		if (folder.id === id) return folder;
		if (folder.children) {
			const found = findFolderById(folder.children, id);
			if (found) return found;
		}
	}
	return null;
}
