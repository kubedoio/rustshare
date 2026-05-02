import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';
import {
	listUserModulePreferences,
	updateUserModulePreference,
	type UserModulePreference
} from '$lib/api/users';

interface UserModulePreferenceState {
	preferences: Record<string, boolean>;
	loaded: boolean;
	loading: boolean;
}

function createUserModulePreferenceStore() {
	const { subscribe, set, update } = writable<UserModulePreferenceState>({
		preferences: {},
		loaded: false,
		loading: false
	});

	return {
		subscribe,
		async load() {
			if (!browser) return;
			update((s) => ({ ...s, loading: true }));
			try {
				const prefs = await listUserModulePreferences();
				const map: Record<string, boolean> = {};
				for (const p of prefs) {
					map[p.module_key] = p.enabled;
				}
				set({ preferences: map, loaded: true, loading: false });
			} catch (err) {
				console.error('Failed to load user module preferences:', err);
				set({ preferences: {}, loaded: true, loading: false });
			}
		},
		async toggle(moduleKey: string, enabled: boolean) {
			update((s) => ({
				...s,
				preferences: { ...s.preferences, [moduleKey]: enabled }
			}));
			try {
				await updateUserModulePreference(moduleKey, enabled);
			} catch (err) {
				console.error('Failed to update module preference:', err);
				// Revert on error
				update((s) => ({
					...s,
					preferences: { ...s.preferences, [moduleKey]: !enabled }
				}));
			}
		},
		isEnabled(moduleKey: string): boolean {
			// Default to true if no preference exists
			let result = true;
			subscribe((s) => {
				result = s.preferences[moduleKey] ?? true;
			})();
			return result;
		}
	};
}

export const userModulePreferences = createUserModulePreferenceStore();
