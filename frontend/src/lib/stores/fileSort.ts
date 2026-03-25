import { writable } from 'svelte/store';

export type SortField = 'name' | 'modified_at' | 'size' | 'mime_type';
export type SortOrder = 'asc' | 'desc';
export type ViewMode = 'grid' | 'list';

export interface FileSortState {
  field: SortField;
  order: SortOrder;
  viewMode: ViewMode;
}

const defaultState: FileSortState = {
  field: 'name',
  order: 'asc',
  viewMode: 'grid'
};

// Load from localStorage if available
function loadState(): FileSortState {
  if (typeof window !== 'undefined') {
    const stored = localStorage.getItem('file-sort-state');
    if (stored) {
      try {
        return { ...defaultState, ...JSON.parse(stored) };
      } catch {
        return defaultState;
      }
    }
  }
  return defaultState;
}

// Save to localStorage
function saveState(state: FileSortState) {
  if (typeof window !== 'undefined') {
    localStorage.setItem('file-sort-state', JSON.stringify(state));
  }
}

export const fileSortState = writable<FileSortState>(loadState());

// Subscribe to changes and save
fileSortState.subscribe((state) => {
  saveState(state);
});

export function setSortField(field: SortField) {
  fileSortState.update((state) => {
    // If clicking the same field, toggle order
    if (state.field === field) {
      return { ...state, order: state.order === 'asc' ? 'desc' : 'asc' };
    }
    // Otherwise, set new field with ascending order
    return { ...state, field, order: 'asc' };
  });
}

export function setViewMode(mode: ViewMode) {
  fileSortState.update((state) => ({ ...state, viewMode: mode }));
}

export function setSortOrder(order: SortOrder) {
  fileSortState.update((state) => ({ ...state, order }));
}
