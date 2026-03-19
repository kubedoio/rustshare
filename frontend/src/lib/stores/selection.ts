import { writable, derived } from 'svelte/store';
import type { File, Folder } from '$lib/api/types';

export interface SelectionState {
  selectedFileIds: Set<string>;
  selectedFolderIds: Set<string>;
}

const defaultState: SelectionState = {
  selectedFileIds: new Set(),
  selectedFolderIds: new Set()
};

function createSelectionStore() {
  const { subscribe, set, update } = writable<SelectionState>(defaultState);

  return {
    subscribe,

    toggleFile: (fileId: string) => {
      update(state => {
        const newFileIds = new Set(state.selectedFileIds);
        if (newFileIds.has(fileId)) {
          newFileIds.delete(fileId);
        } else {
          newFileIds.add(fileId);
        }
        return { ...state, selectedFileIds: newFileIds };
      });
    },

    toggleFolder: (folderId: string) => {
      update(state => {
        const newFolderIds = new Set(state.selectedFolderIds);
        if (newFolderIds.has(folderId)) {
          newFolderIds.delete(folderId);
        } else {
          newFolderIds.add(folderId);
        }
        return { ...state, selectedFolderIds: newFolderIds };
      });
    },

    selectAll: (files: File[], folders: Folder[]) => {
      update(state => ({
        selectedFileIds: new Set(files.map(f => f.id)),
        selectedFolderIds: new Set(folders.map(f => f.id))
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
  $selection => $selection.selectedFileIds.size + $selection.selectedFolderIds.size
);

export const hasSelection = derived(
  selectionCount,
  $count => $count > 0
);
