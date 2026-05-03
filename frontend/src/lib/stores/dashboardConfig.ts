import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';
import { getDashboardModulesForUser } from '$lib/modules/registry';
import type { User } from '$lib/api/types';
import type { ModuleDefinition } from '$lib/modules/registry';

import { getDashboardConfig, updateDashboardConfig } from '$lib/api/users';
import type { DashboardConfig } from '$lib/api/users';

const STORAGE_KEY = 'dashboard-config-v1';

interface DashboardConfigState {
	enabledModules: string[];
	moduleOrder: string[];
	editMode: boolean;
	loading: boolean;
}

function getDefaultOrder(allModules: ModuleDefinition[]): string[] {
	return allModules.map((m) => m.key);
}

let updateTimeout: any = null;

function persistState(state: DashboardConfigState) {
	if (!browser) return;

	// Debounce updates to the server
	if (updateTimeout) clearTimeout(updateTimeout);
	updateTimeout = setTimeout(async () => {
		try {
			await updateDashboardConfig({
				enabled_modules: state.enabledModules,
				module_order: state.moduleOrder,
				sections: [] // Currently not used
			});
		} catch (error) {
			console.error('Failed to persist dashboard config to server:', error);
		}
	}, 1000);
}

function createDashboardConfigStore() {
	const initialState: DashboardConfigState = {
		enabledModules: [],
		moduleOrder: [],
		editMode: false,
		loading: true
	};

	const { subscribe, set, update } = writable<DashboardConfigState>(initialState);

	return {
		subscribe,

		hydrate: async (allModules: ModuleDefinition[]) => {
			update((s) => ({ ...s, loading: true }));
			try {
				const config = await getDashboardConfig();

				const validKeys = new Set(allModules.map((m) => m.key));
				let enabled = (config.enabled_modules ?? []).filter((k) => validKeys.has(k));
				let order = (config.module_order ?? []).filter((k) => validKeys.has(k));

				// If empty (new user), use defaults
				if (enabled.length === 0 && order.length === 0) {
					enabled = getDefaultOrder(allModules);
					order = getDefaultOrder(allModules);
				} else {
					// Append any new modules not in saved order
					for (const key of validKeys) {
						if (!order.includes(key)) order.push(key);
						if (!enabled.includes(key)) enabled.push(key);
					}
				}

				set({
					enabledModules: enabled,
					moduleOrder: order,
					editMode: false,
					loading: false
				});
			} catch (error) {
				console.error('Failed to hydrate dashboard config from server:', error);
				// Fallback to defaults
				set({
					enabledModules: getDefaultOrder(allModules),
					moduleOrder: getDefaultOrder(allModules),
					editMode: false,
					loading: false
				});
			}
		},

		toggleModule: (key: string) => {
			update((state) => {
				const enabled = new Set(state.enabledModules);
				if (enabled.has(key)) {
					enabled.delete(key);
				} else {
					enabled.add(key);
				}
				const newState = {
					...state,
					enabledModules: Array.from(enabled)
				};
				persistState(newState);
				return newState;
			});
		},

		moveModule: (key: string, direction: 'up' | 'down') => {
			update((state) => {
				const order = [...state.moduleOrder];
				const idx = order.indexOf(key);
				if (idx === -1) return state;
				if (direction === 'up' && idx > 0) {
					[order[idx], order[idx - 1]] = [order[idx - 1], order[idx]];
				} else if (direction === 'down' && idx < order.length - 1) {
					[order[idx], order[idx + 1]] = [order[idx + 1], order[idx]];
				}
				const newState = { ...state, moduleOrder: order };
				persistState(newState);
				return newState;
			});
		},

		setEditMode: (value: boolean) => {
			update((state) => ({ ...state, editMode: value }));
		},

		reset: (modules: ModuleDefinition[]) => {
			const defaultState = {
				enabledModules: getDefaultOrder(modules),
				moduleOrder: getDefaultOrder(modules),
				editMode: false,
				loading: false
			};
			persistState(defaultState);
			set(defaultState);
		}
	};
}

export const dashboardConfig = createDashboardConfigStore();

export function getVisibleModules(
	allModules: ModuleDefinition[],
	config: DashboardConfigState
): ModuleDefinition[] {
	const enabledSet = new Set(config.enabledModules);
	const moduleMap = new Map(allModules.map((m) => [m.key, m]));
	const result: ModuleDefinition[] = [];
	for (const key of config.moduleOrder) {
		const mod = moduleMap.get(key);
		if (mod && enabledSet.has(key)) {
			result.push(mod);
		}
	}
	return result;
}

export const editMode = derived(dashboardConfig, ($c) => $c.editMode);
