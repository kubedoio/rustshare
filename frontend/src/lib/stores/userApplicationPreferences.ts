import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';
import {
	listUserApplicationPreferences,
	updateUserApplicationPreference,
	type UserApplicationPreference
} from '$lib/api/users';

interface UserApplicationPreferenceState {
	preferences: Record<string, boolean>;
	loaded: boolean;
	loading: boolean;
}

function createUserApplicationPreferenceStore() {
	const { subscribe, set, update } = writable<UserApplicationPreferenceState>({
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
				const prefs = await listUserApplicationPreferences();
				const map: Record<string, boolean> = {};
				for (const p of prefs) {
					map[p.application_id] = p.enabled;
				}
				set({ preferences: map, loaded: true, loading: false });
			} catch (err) {
				console.error('Failed to load user module preferences:', err);
				set({ preferences: {}, loaded: true, loading: false });
			}
		},
		async toggle(applicationId: string, enabled: boolean) {
			update((s) => ({
				...s,
				preferences: { ...s.preferences, [applicationId]: enabled }
			}));
			try {
				await updateUserApplicationPreference(applicationId, enabled);
			} catch (err) {
				console.error('Failed to update module preference:', err);
				// Revert on error
				update((s) => ({
					...s,
					preferences: { ...s.preferences, [applicationId]: !enabled }
				}));
			}
		},
		isEnabled(applicationId: string): boolean {
			// Default to true if no preference exists
			let result = true;
			subscribe((s) => {
				result = s.preferences[applicationId] ?? true;
			})();
			return result;
		}
	};
}

export const userApplicationPreferences = createUserApplicationPreferenceStore();
