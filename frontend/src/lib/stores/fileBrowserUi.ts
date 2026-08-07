import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';

export type ViewMode = 'grid' | 'list';

interface FileBrowserUiState {
	viewMode: ViewMode;
	expandedFolderIds: Set<string>;
	selectedFolderId: string | null;
	searchQuery: string;
}

const STORAGE_KEY = 'file-browser-ui-v1';

function createFileBrowserUiStore() {
	// Load initial state from localStorage
	const loadState = (): FileBrowserUiState => {
		if (!browser) {
			return {
				viewMode: 'list',
				expandedFolderIds: new Set(),
				selectedFolderId: null,
				searchQuery: ''
			};
		}

		try {
			const saved = localStorage.getItem(STORAGE_KEY);
			if (saved) {
				const parsed = JSON.parse(saved);
				return {
					viewMode: parsed.viewMode || 'list',
					expandedFolderIds: new Set(parsed.expandedFolderIds || []),
					selectedFolderId: parsed.selectedFolderId || null,
					searchQuery: parsed.searchQuery || ''
				};
			}
		} catch {
			// Ignore parse errors
		}

		return {
			viewMode: 'list',
			expandedFolderIds: new Set(),
			selectedFolderId: null,
			searchQuery: ''
		};
	};

	const initialState = loadState();
	const { subscribe, set, update } = writable<FileBrowserUiState>(initialState);

	// Persist state changes to localStorage
	const persist = (state: FileBrowserUiState) => {
		if (!browser) return;
		try {
			localStorage.setItem(
				STORAGE_KEY,
				JSON.stringify({
					viewMode: state.viewMode,
					expandedFolderIds: Array.from(state.expandedFolderIds),
					selectedFolderId: state.selectedFolderId,
					searchQuery: state.searchQuery
				})
			);
		} catch {
			// Ignore storage errors
		}
	};

	return {
		subscribe,

		setViewMode: (mode: ViewMode) => {
			update((state) => {
				const newState = { ...state, viewMode: mode };
				persist(newState);
				return newState;
			});
		},

		toggleViewMode: () => {
			update((state) => {
				const newState: FileBrowserUiState = {
					...state,
					viewMode: state.viewMode === 'grid' ? 'list' : 'grid'
				};
				persist(newState);
				return newState;
			});
		},

		expandFolder: (folderId: string) => {
			update((state) => {
				const expanded = new Set(state.expandedFolderIds);
				expanded.add(folderId);
				const newState = { ...state, expandedFolderIds: expanded };
				persist(newState);
				return newState;
			});
		},

		collapseFolder: (folderId: string) => {
			update((state) => {
				const expanded = new Set(state.expandedFolderIds);
				expanded.delete(folderId);
				const newState = { ...state, expandedFolderIds: expanded };
				persist(newState);
				return newState;
			});
		},

		toggleFolderExpanded: (folderId: string) => {
			update((state) => {
				const expanded = new Set(state.expandedFolderIds);
				if (expanded.has(folderId)) {
					expanded.delete(folderId);
				} else {
					expanded.add(folderId);
				}
				const newState = { ...state, expandedFolderIds: expanded };
				persist(newState);
				return newState;
			});
		},

		selectFolder: (folderId: string | null) => {
			update((state) => {
				const newState = { ...state, selectedFolderId: folderId };
				persist(newState);
				return newState;
			});
		},

		setSearchQuery: (query: string) => {
			update((state) => {
				const newState = { ...state, searchQuery: query };
				persist(newState);
				return newState;
			});
		},

		clearSearch: () => {
			update((state) => {
				const newState = { ...state, searchQuery: '' };
				persist(newState);
				return newState;
			});
		},

		collapseAll: () => {
			update((state) => {
				const newState = { ...state, expandedFolderIds: new Set<string>() };
				persist(newState);
				return newState;
			});
		},

		reset: () => {
			const defaultState = {
				viewMode: 'list' as ViewMode,
				expandedFolderIds: new Set<string>(),
				selectedFolderId: null,
				searchQuery: ''
			};
			persist(defaultState);
			set(defaultState);
		}
	};
}

export const fileBrowserUi = createFileBrowserUiStore();

// Derived stores for convenience
export const viewMode = derived(fileBrowserUi, ($ui) => $ui.viewMode);
export const expandedFolderIds = derived(fileBrowserUi, ($ui) => $ui.expandedFolderIds);
export const selectedFolderId = derived(fileBrowserUi, ($ui) => $ui.selectedFolderId);
export const searchQuery = derived(fileBrowserUi, ($ui) => $ui.searchQuery);
