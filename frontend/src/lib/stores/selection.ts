import { writable, derived } from 'svelte/store';
import type { File, Folder } from '$lib/api/types';

export interface SelectionState {
	selectedFileIds: Set<string>;
	selectedFolderIds: Set<string>;
	anchorFileId: string | null;
	anchorFolderId: string | null;
}

const defaultState: SelectionState = {
	selectedFileIds: new Set(),
	selectedFolderIds: new Set(),
	anchorFileId: null,
	anchorFolderId: null
};

function createSelectionStore() {
	const { subscribe, set, update } = writable<SelectionState>(defaultState);

	return {
		subscribe,

		toggleFile: (fileId: string, isShiftKey = false, allFileIds: string[] = []) => {
			update((state) => {
				const newFileIds = new Set(state.selectedFileIds);

				if (isShiftKey && state.anchorFileId && allFileIds.length > 0) {
					// Range selection
					const anchorIndex = allFileIds.indexOf(state.anchorFileId);
					const currentIndex = allFileIds.indexOf(fileId);

					if (anchorIndex !== -1 && currentIndex !== -1) {
						const start = Math.min(anchorIndex, currentIndex);
						const end = Math.max(anchorIndex, currentIndex);

						for (let i = start; i <= end; i++) {
							newFileIds.add(allFileIds[i]);
						}
					}
				} else {
					// Toggle single file and set as anchor
					if (newFileIds.has(fileId)) {
						newFileIds.delete(fileId);
					} else {
						newFileIds.add(fileId);
					}
				}

				return {
					...state,
					selectedFileIds: newFileIds,
					anchorFileId: fileId,
					anchorFolderId: null
				};
			});
		},

		toggleFolder: (folderId: string, isShiftKey = false, allFolderIds: string[] = []) => {
			update((state) => {
				const newFolderIds = new Set(state.selectedFolderIds);

				if (isShiftKey && state.anchorFolderId && allFolderIds.length > 0) {
					// Range selection
					const anchorIndex = allFolderIds.indexOf(state.anchorFolderId);
					const currentIndex = allFolderIds.indexOf(folderId);

					if (anchorIndex !== -1 && currentIndex !== -1) {
						const start = Math.min(anchorIndex, currentIndex);
						const end = Math.max(anchorIndex, currentIndex);

						for (let i = start; i <= end; i++) {
							newFolderIds.add(allFolderIds[i]);
						}
					}
				} else {
					// Toggle single folder and set as anchor
					if (newFolderIds.has(folderId)) {
						newFolderIds.delete(folderId);
					} else {
						newFolderIds.add(folderId);
					}
				}

				return {
					...state,
					selectedFolderIds: newFolderIds,
					anchorFolderId: folderId,
					anchorFileId: null
				};
			});
		},

		selectAll: (files: File[], folders: Folder[]) => {
			update((state) => ({
				selectedFileIds: new Set(files.map((f) => f.id)),
				selectedFolderIds: new Set(folders.map((f) => f.id)),
				anchorFileId: null,
				anchorFolderId: null
			}));
		},

		deselectAll: () => {
			set(defaultState);
		},

		isFileSelected: (fileId: string, state: SelectionState) => {
			return state.selectedFileIds.has(fileId);
		},

		isFolderSelected: (folderId: string, state: SelectionState) => {
			return state.selectedFolderIds.has(folderId);
		},

		clear: () => {
			set(defaultState);
		}
	};
}

export const selectionStore = createSelectionStore();

export const selectionCount = derived(
	selectionStore,
	($selection) => $selection.selectedFileIds.size + $selection.selectedFolderIds.size
);

export const hasSelection = derived(selectionCount, ($count) => $count > 0);
