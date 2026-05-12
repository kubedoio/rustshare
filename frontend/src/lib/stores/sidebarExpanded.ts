import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const STORAGE_KEY = 'rustshare-sidebar-expanded';

function createSidebarExpandedStore() {
	const initial = browser ? localStorage.getItem(STORAGE_KEY) === 'true' : false;
	const store = writable<boolean>(initial);

	if (browser) {
		store.subscribe((value) => {
			localStorage.setItem(STORAGE_KEY, String(value));
		});
	}

	return {
		subscribe: store.subscribe,
		toggle: () => store.update((v) => !v),
		set: (value: boolean) => store.set(value)
	};
}

export const sidebarExpanded = createSidebarExpandedStore();
