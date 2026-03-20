import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import { fileSortState, setSortField, setViewMode } from '$lib/stores/fileSort';

describe('fileSort store', () => {
	beforeEach(() => {
		// Reset store to default state
		fileSortState.set({
			field: 'name',
			order: 'asc',
			viewMode: 'grid'
		});
		// Clear localStorage
		localStorage.clear();
	});

	describe('default state', () => {
		it('should have default values', () => {
			const state = get(fileSortState);
			expect(state.field).toBe('name');
			expect(state.order).toBe('asc');
			expect(state.viewMode).toBe('grid');
		});
	});

	describe('setSortField', () => {
		it('should set new field with ascending order', () => {
			setSortField('modified_at');
			const state = get(fileSortState);
			expect(state.field).toBe('modified_at');
			expect(state.order).toBe('asc');
		});

		it('should toggle order when clicking same field', () => {
			setSortField('name'); // First click
			let state = get(fileSortState);
			expect(state.order).toBe('desc'); // Toggles from asc to desc

			setSortField('name'); // Second click
			state = get(fileSortState);
			expect(state.order).toBe('asc'); // Toggles back to asc
		});

		it('should reset to ascending when switching fields', () => {
			fileSortState.set({
				field: 'name',
				order: 'desc',
				viewMode: 'grid'
			});

			setSortField('size');
			const state = get(fileSortState);
			expect(state.field).toBe('size');
			expect(state.order).toBe('asc');
		});

		it('should support all sort fields', () => {
			const fields: Array<'name' | 'modified_at' | 'size' | 'mime_type'> = [
				'name',
				'modified_at',
				'size',
				'mime_type'
			];

			fields.forEach((field) => {
				setSortField(field);
				const state = get(fileSortState);
				expect(state.field).toBe(field);
			});
		});
	});

	describe('setViewMode', () => {
		it('should switch to list view', () => {
			setViewMode('list');
			const state = get(fileSortState);
			expect(state.viewMode).toBe('list');
		});

		it('should switch to grid view', () => {
			setViewMode('grid');
			const state = get(fileSortState);
			expect(state.viewMode).toBe('grid');
		});

		it('should toggle between views', () => {
			setViewMode('list');
			expect(get(fileSortState).viewMode).toBe('list');

			setViewMode('grid');
			expect(get(fileSortState).viewMode).toBe('grid');
		});
	});

	describe('localStorage persistence', () => {
		it('should save state to localStorage', () => {
			setSortField('size');
			setViewMode('list');

			const stored = localStorage.getItem('file-sort-state');
			expect(stored).toBeTruthy();

			const parsed = JSON.parse(stored!);
			expect(parsed.field).toBe('size');
			expect(parsed.viewMode).toBe('list');
		});

		it('should load state from localStorage on init', async () => {
			// Manually set localStorage
			localStorage.setItem(
				'file-sort-state',
				JSON.stringify({
					field: 'modified_at',
					order: 'desc',
					viewMode: 'list'
				})
			);

			// Import fresh to trigger loadState
			vi.resetModules();
			const { fileSortState: freshStore } = await import('./fileSort');
			const state = get(freshStore);

			expect(state.field).toBe('modified_at');
			expect(state.order).toBe('desc');
			expect(state.viewMode).toBe('list');
		});

		it('should handle corrupted localStorage gracefully', async () => {
			localStorage.setItem('file-sort-state', 'invalid json');

			vi.resetModules();
			const { fileSortState: freshStore } = await import('./fileSort');
			const state = get(freshStore);

			// Should fall back to defaults
			expect(state.field).toBe('name');
			expect(state.order).toBe('asc');
			expect(state.viewMode).toBe('grid');
		});
	});
});
