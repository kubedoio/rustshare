import { writable } from 'svelte/store';

export const searchQuery = writable<string>('');

export function clearSearch() {
	searchQuery.set('');
}
