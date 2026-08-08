import { writable } from 'svelte/store';

export type SortField = 'name' | 'modified_at' | 'size' | 'mime_type';
export type SortOrder = 'asc' | 'desc';
export type PageSize = 10 | 20 | 50;

export interface FileSortState {
	field: SortField;
	order: SortOrder;
	pageSize: PageSize;
}

const defaultState: FileSortState = {
	field: 'name',
	order: 'asc',
	pageSize: 20
};

const SORT_FIELDS: readonly SortField[] = ['name', 'modified_at', 'size', 'mime_type'];
const SORT_ORDERS: readonly SortOrder[] = ['asc', 'desc'];
const PAGE_SIZES: readonly PageSize[] = [10, 20, 50];

// Load from localStorage if available
function loadState(): FileSortState {
	if (typeof window !== 'undefined') {
		const stored = localStorage.getItem('file-sort-state-v3');
		if (stored) {
			try {
				const parsed = JSON.parse(stored) as Partial<FileSortState>;
				return {
					field: SORT_FIELDS.includes(parsed.field as SortField)
						? (parsed.field as SortField)
						: defaultState.field,
					order: SORT_ORDERS.includes(parsed.order as SortOrder)
						? (parsed.order as SortOrder)
						: defaultState.order,
					pageSize: PAGE_SIZES.includes(parsed.pageSize as PageSize)
						? (parsed.pageSize as PageSize)
						: defaultState.pageSize
				};
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
		localStorage.setItem('file-sort-state-v3', JSON.stringify(state));
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

export function setSortOrder(order: SortOrder) {
	fileSortState.update((state) => ({ ...state, order }));
}

export function setPageSize(size: PageSize) {
	fileSortState.update((state) => ({ ...state, pageSize: size }));
}
