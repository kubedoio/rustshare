import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import { fileSortState, setSortField, setPageSize } from '$lib/stores/fileSort';

describe('fileSort store', () => {
	beforeEach(() => {
		// Reset store to default state
		fileSortState.set({
			field: 'name',
			order: 'asc',
			pageSize: 20
		});
		// Clear localStorage
		localStorage.clear();
	});

	describe('default state', () => {
		it('should have default values', () => {
			const state = get(fileSortState);
			expect(state.field).toBe('name');
			expect(state.order).toBe('asc');
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
				pageSize: 20
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

	describe('pageSize', () => {
		it('should default to 20', () => {
			const state = get(fileSortState);
			expect(state.pageSize).toBe(20);
		});

		it('should update pageSize', () => {
			setPageSize(50);
			expect(get(fileSortState).pageSize).toBe(50);
		});
	});

	describe('localStorage persistence v3', () => {
		it('should save state to localStorage', () => {
			setSortField('size');

			const stored = localStorage.getItem('file-sort-state-v3');
			expect(stored).toBeTruthy();

			const parsed = JSON.parse(stored!);
			expect(parsed.field).toBe('size');
		});

		it('should load state from localStorage on init', async () => {
			// Manually set localStorage
			localStorage.setItem(
				'file-sort-state-v3',
				JSON.stringify({
					field: 'modified_at',
					order: 'desc',
					pageSize: 20
				})
			);

			// Import fresh to trigger loadState
			vi.resetModules();
			const { fileSortState: freshStore } = await import('./fileSort');
			const state = get(freshStore);

			expect(state.field).toBe('modified_at');
			expect(state.order).toBe('desc');
		});

		it('should handle corrupted localStorage gracefully', async () => {
			localStorage.setItem('file-sort-state-v3', 'invalid json');

			vi.resetModules();
			const { fileSortState: freshStore } = await import('./fileSort');
			const state = get(freshStore);

			// Should fall back to defaults
			expect(state.field).toBe('name');
			expect(state.order).toBe('asc');
		});

		it('should save pageSize to localStorage', () => {
			setPageSize(10);
			const stored = localStorage.getItem('file-sort-state-v3');
			expect(stored).toBeTruthy();
			expect(JSON.parse(stored!).pageSize).toBe(10);
		});

		it('should load pageSize from localStorage on init', async () => {
			localStorage.setItem(
				'file-sort-state-v3',
				JSON.stringify({
					field: 'size',
					order: 'desc',
					pageSize: 50
				})
			);

			vi.resetModules();
			const { fileSortState: freshStore } = await import('./fileSort');
			const state = get(freshStore);
			expect(state.pageSize).toBe(50);
		});
	});
});
