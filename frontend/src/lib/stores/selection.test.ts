import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
	selectionStore,
	selectionCount,
	hasSelection,
	type SelectionState
} from '$lib/stores/selection';
import type { File, Folder } from '$lib/api/types';

// Mock data
const mockFiles: File[] = [
	{
		id: 'file-1',
		name: 'document.pdf',
		path: '/document.pdf',
		size: 1024,
		mime_type: 'application/pdf',
		parent_folder_id: null,
		owner_id: 'user-1',
		current_version: 1,
		created_at: '2024-01-01T00:00:00Z',
		modified_at: '2024-01-01T00:00:00Z'
	},
	{
		id: 'file-2',
		name: 'image.png',
		path: '/image.png',
		size: 2048,
		mime_type: 'image/png',
		parent_folder_id: null,
		owner_id: 'user-1',
		current_version: 1,
		created_at: '2024-01-02T00:00:00Z',
		modified_at: '2024-01-02T00:00:00Z'
	}
];

const mockFolders: Folder[] = [
	{
		id: 'folder-1',
		name: 'Documents',
		path: '/Documents',
		parent_folder_id: null,
		owner_id: 'user-1',
		created_at: '2024-01-01T00:00:00Z',
		updated_at: '2024-01-01T00:00:00Z'
	},
	{
		id: 'folder-2',
		name: 'Photos',
		path: '/Photos',
		parent_folder_id: null,
		owner_id: 'user-1',
		created_at: '2024-01-02T00:00:00Z',
		updated_at: '2024-01-02T00:00:00Z'
	}
];

describe('selection store', () => {
	beforeEach(() => {
		// Reset store to default state
		selectionStore.clear();
	});

	describe('initial state', () => {
		it('should start with empty selections', () => {
			const state = get(selectionStore);
			expect(state.selectedFileIds.size).toBe(0);
			expect(state.selectedFolderIds.size).toBe(0);
		});

		it('should have zero selection count', () => {
			expect(get(selectionCount)).toBe(0);
		});

		it('should have no selection', () => {
			expect(get(hasSelection)).toBe(false);
		});
	});

	describe('toggleFile', () => {
		it('should select a file', () => {
			selectionStore.toggleFile('file-1');
			const state = get(selectionStore);
			expect(state.selectedFileIds.has('file-1')).toBe(true);
		});

		it('should deselect a file when toggled again', () => {
			selectionStore.toggleFile('file-1');
			selectionStore.toggleFile('file-1');
			const state = get(selectionStore);
			expect(state.selectedFileIds.has('file-1')).toBe(false);
		});

		it('should select multiple files', () => {
			selectionStore.toggleFile('file-1');
			selectionStore.toggleFile('file-2');
			const state = get(selectionStore);
			expect(state.selectedFileIds.size).toBe(2);
			expect(state.selectedFileIds.has('file-1')).toBe(true);
			expect(state.selectedFileIds.has('file-2')).toBe(true);
		});
	});

	describe('toggleFolder', () => {
		it('should select a folder', () => {
			selectionStore.toggleFolder('folder-1');
			const state = get(selectionStore);
			expect(state.selectedFolderIds.has('folder-1')).toBe(true);
		});

		it('should deselect a folder when toggled again', () => {
			selectionStore.toggleFolder('folder-1');
			selectionStore.toggleFolder('folder-1');
			const state = get(selectionStore);
			expect(state.selectedFolderIds.has('folder-1')).toBe(false);
		});

		it('should select multiple folders', () => {
			selectionStore.toggleFolder('folder-1');
			selectionStore.toggleFolder('folder-2');
			const state = get(selectionStore);
			expect(state.selectedFolderIds.size).toBe(2);
		});
	});

	describe('selectAll', () => {
		it('should select all files and folders', () => {
			selectionStore.selectAll(mockFiles, mockFolders);
			const state = get(selectionStore);
			expect(state.selectedFileIds.size).toBe(2);
			expect(state.selectedFolderIds.size).toBe(2);
			expect(state.selectedFileIds.has('file-1')).toBe(true);
			expect(state.selectedFileIds.has('file-2')).toBe(true);
			expect(state.selectedFolderIds.has('folder-1')).toBe(true);
			expect(state.selectedFolderIds.has('folder-2')).toBe(true);
		});

		it('should handle empty arrays', () => {
			selectionStore.selectAll([], []);
			const state = get(selectionStore);
			expect(state.selectedFileIds.size).toBe(0);
			expect(state.selectedFolderIds.size).toBe(0);
		});

		it('should replace existing selection', () => {
			selectionStore.toggleFile('other-file');
			selectionStore.selectAll(mockFiles, mockFolders);
			const state = get(selectionStore);
			expect(state.selectedFileIds.has('other-file')).toBe(false);
			expect(state.selectedFileIds.has('file-1')).toBe(true);
		});
	});

	describe('deselectAll', () => {
		it('should clear all selections', () => {
			selectionStore.toggleFile('file-1');
			selectionStore.toggleFolder('folder-1');
			selectionStore.deselectAll();
			const state = get(selectionStore);
			expect(state.selectedFileIds.size).toBe(0);
			expect(state.selectedFolderIds.size).toBe(0);
		});
	});

	describe('clear', () => {
		it('should reset to initial state', () => {
			selectionStore.toggleFile('file-1');
			selectionStore.toggleFolder('folder-1');
			selectionStore.clear();
			const state = get(selectionStore);
			expect(state.selectedFileIds.size).toBe(0);
			expect(state.selectedFolderIds.size).toBe(0);
		});
	});

	describe('isFileSelected', () => {
		it('should return true for selected file', () => {
			selectionStore.toggleFile('file-1');
			const state = get(selectionStore);
			expect(selectionStore.isFileSelected('file-1', state)).toBe(true);
		});

		it('should return false for unselected file', () => {
			const state = get(selectionStore);
			expect(selectionStore.isFileSelected('file-1', state)).toBe(false);
		});
	});

	describe('isFolderSelected', () => {
		it('should return true for selected folder', () => {
			selectionStore.toggleFolder('folder-1');
			const state = get(selectionStore);
			expect(selectionStore.isFolderSelected('folder-1', state)).toBe(true);
		});

		it('should return false for unselected folder', () => {
			const state = get(selectionStore);
			expect(selectionStore.isFolderSelected('folder-1', state)).toBe(false);
		});
	});

	describe('selectionCount derived store', () => {
		it('should count selected items', () => {
			selectionStore.toggleFile('file-1');
			selectionStore.toggleFile('file-2');
			selectionStore.toggleFolder('folder-1');
			expect(get(selectionCount)).toBe(3);
		});

		it('should update when selection changes', () => {
			selectionStore.toggleFile('file-1');
			expect(get(selectionCount)).toBe(1);
			selectionStore.toggleFile('file-1');
			expect(get(selectionCount)).toBe(0);
		});
	});

	describe('hasSelection derived store', () => {
		it('should be false when nothing selected', () => {
			expect(get(hasSelection)).toBe(false);
		});

		it('should be true when file selected', () => {
			selectionStore.toggleFile('file-1');
			expect(get(hasSelection)).toBe(true);
		});

		it('should be true when folder selected', () => {
			selectionStore.toggleFolder('folder-1');
			expect(get(hasSelection)).toBe(true);
		});

		it('should be false after clearing', () => {
			selectionStore.toggleFile('file-1');
			selectionStore.clear();
			expect(get(hasSelection)).toBe(false);
		});
	});

	describe('mixed selection', () => {
		it('should handle files and folders together', () => {
			selectionStore.toggleFile('file-1');
			selectionStore.toggleFolder('folder-1');
			const state = get(selectionStore);
			expect(state.selectedFileIds.has('file-1')).toBe(true);
			expect(state.selectedFolderIds.has('folder-1')).toBe(true);
			expect(get(selectionCount)).toBe(2);
		});

		it('should clear both files and folders', () => {
			selectionStore.toggleFile('file-1');
			selectionStore.toggleFolder('folder-1');
			selectionStore.clear();
			expect(get(selectionCount)).toBe(0);
		});
	});
});
