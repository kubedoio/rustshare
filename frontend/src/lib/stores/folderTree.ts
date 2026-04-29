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
	version: number; // Increment to force re-renders
}

function createFolderTreeStore() {
	const { subscribe, set, update } = writable<FolderTreeState>({
		rootFolders: [],
		expandedIds: new Set(),
		selectedId: null,
		loadingIds: new Set(),
		version: 0
	});

	return {
		subscribe,

		setRootFolders: (folders: Folder[]) => {
			update((state) => ({
				...state,
				rootFolders: folders.map((f) => ({
					...f,
					children: undefined,
					isExpanded: state.expandedIds.has(f.id)
				})),
				version: state.version + 1
			}));
		},

		selectFolder: (folderId: string | null) => {
			update((state) => ({
				...state,
				selectedId: folderId
			}));
		},

		toggleExpand: (folderId: string) => {
			update((state) => {
				const newExpanded = new Set(state.expandedIds);
				if (newExpanded.has(folderId)) {
					newExpanded.delete(folderId);
				} else {
					newExpanded.add(folderId);
				}
				return {
					...state,
					expandedIds: newExpanded,
					version: state.version + 1
				};
			});
		},

		setExpanded: (folderId: string, expanded: boolean) => {
			update((state) => {
				const newExpanded = new Set(state.expandedIds);
				if (expanded) {
					newExpanded.add(folderId);
				} else {
					newExpanded.delete(folderId);
				}
				return {
					...state,
					expandedIds: newExpanded,
					version: state.version + 1
				};
			});
		},

		setFolderChildren: (folderId: string, children: Folder[]) => {
			update((state) => ({
				...state,
				rootFolders: updateFolderChildren(state.rootFolders, folderId, children, state.expandedIds),
				version: state.version + 1
			}));
		},

		setLoading: (folderId: string, loading: boolean) => {
			update((state) => {
				const newLoading = new Set(state.loadingIds);
				if (loading) {
					newLoading.add(folderId);
				} else {
					newLoading.delete(folderId);
				}
				return {
					...state,
					loadingIds: newLoading,
					version: state.version + 1
				};
			});
		},

		// Remove a folder from the tree (used after delete)
		removeFolder: (folderId: string) => {
			update((state) => ({
				...state,
				rootFolders: removeFolderFromTree(state.rootFolders, folderId),
				expandedIds: new Set([...state.expandedIds].filter((id) => id !== folderId)),
				selectedId: state.selectedId === folderId ? null : state.selectedId,
				version: state.version + 1
			}));
		},

		// Update a folder's name
		updateFolderName: (folderId: string, newName: string) => {
			update((state) => ({
				...state,
				rootFolders: updateFolderNameInTree(state.rootFolders, folderId, newName),
				version: state.version + 1
			}));
		},

		// Add a new folder to the tree (for live updates after create)
		addFolder: (folder: Folder, parentFolderId: string | null) => {
			update((state) => {
				const newFolder: FolderNode = { ...folder, children: undefined };

				if (parentFolderId === null) {
					// Add to root
					return {
						...state,
						rootFolders: [...state.rootFolders, newFolder],
						version: state.version + 1
					};
				}

				// Add to parent folder's children
				return {
					...state,
					rootFolders: addFolderToTree(state.rootFolders, newFolder, parentFolderId),
					version: state.version + 1
				};
			});
		},

		// Move a folder to a new parent
		moveFolder: (folderId: string, newParentId: string | null) => {
			update((state) => {
				// Find the folder to move
				const folderToMove = findFolderById(state.rootFolders, folderId);
				if (!folderToMove) return state;

				// Remove from old location
				const withoutFolder = removeFolderFromTree(state.rootFolders, folderId);

				// Update parent reference
				const movedFolder: FolderNode = {
					...folderToMove,
					parent_folder_id: newParentId,
					children: undefined // Reset children to lazy load
				};

				// Add to new location
				const withFolderAdded =
					newParentId === null
						? [...withoutFolder, movedFolder]
						: addFolderToTree(withoutFolder, movedFolder, newParentId);

				return {
					...state,
					rootFolders: withFolderAdded,
					expandedIds: new Set([...state.expandedIds].filter((id) => id !== folderId)),
					version: state.version + 1
				};
			});
		},

		// Refresh the tree - clears all children to force re-fetch
		refresh: () => {
			update((state) => ({
				...state,
				rootFolders: state.rootFolders.map((f) => ({ ...f, children: undefined })),
				version: state.version + 1
			}));
		},

		reset: () => {
			set({
				rootFolders: [],
				expandedIds: new Set(),
				selectedId: null,
				loadingIds: new Set(),
				version: 0
			});
		}
	};
}

function removeFolderFromTree(folders: FolderNode[], folderId: string): FolderNode[] {
	return folders
		.filter((folder) => folder.id !== folderId)
		.map((folder) => {
			if (folder.children) {
				return { ...folder, children: removeFolderFromTree(folder.children, folderId) };
			}
			return folder;
		});
}

function updateFolderNameInTree(
	folders: FolderNode[],
	folderId: string,
	newName: string
): FolderNode[] {
	return folders.map((folder) => {
		if (folder.id === folderId) {
			return { ...folder, name: newName };
		}
		if (folder.children) {
			return { ...folder, children: updateFolderNameInTree(folder.children, folderId, newName) };
		}
		return folder;
	});
}

function updateFolderChildren(
	folders: FolderNode[],
	folderId: string,
	children: Folder[],
	expandedIds: Set<string>
): FolderNode[] {
	return folders.map((folder) => {
		if (folder.id === folderId) {
			return {
				...folder,
				children: children.map((c) => ({
					...c,
					children: undefined,
					isExpanded: expandedIds.has(c.id)
				}))
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

function addFolderToTree(
	folders: FolderNode[],
	newFolder: FolderNode,
	parentId: string
): FolderNode[] {
	return folders.map((folder) => {
		if (folder.id === parentId) {
			return {
				...folder,
				children: [...(folder.children || []), newFolder]
			};
		}
		if (folder.children) {
			return {
				...folder,
				children: addFolderToTree(folder.children, newFolder, parentId)
			};
		}
		return folder;
	});
}

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

export const folderTreeStore = createFolderTreeStore();

// Derived store to get the current selected folder
export const selectedFolder = derived(folderTreeStore, ($store) => {
	if (!$store.selectedId) return null;
	return findFolderById($store.rootFolders, $store.selectedId);
});
