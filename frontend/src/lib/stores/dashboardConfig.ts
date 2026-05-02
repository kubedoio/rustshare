import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';
import { getDashboardModulesForUser } from '$lib/modules/registry';
import type { User } from '$lib/api/types';
import type { ModuleDefinition } from '$lib/modules/registry';

const STORAGE_KEY = 'dashboard-config-v1';

interface DashboardConfigState {
	enabledModules: string[];
	moduleOrder: string[];
	editMode: boolean;
}

function getDefaultOrder(allModules: ModuleDefinition[]): string[] {
	return allModules.map((m) => m.key);
}

function loadState(allModules: ModuleDefinition[]): DashboardConfigState {
	if (!browser) {
		return {
			enabledModules: getDefaultOrder(allModules),
			moduleOrder: getDefaultOrder(allModules),
			editMode: false
		};
	}

	try {
		const saved = localStorage.getItem(STORAGE_KEY);
		if (saved) {
			const parsed = JSON.parse(saved);
			// Validate against current module keys
			const validKeys = new Set(allModules.map((m) => m.key));
			const enabled = (parsed.enabledModules ?? []).filter((k: string) => validKeys.has(k));
			const order = (parsed.moduleOrder ?? []).filter((k: string) => validKeys.has(k));
			// Append any new modules not in saved order
			for (const key of validKeys) {
				if (!order.includes(key)) order.push(key);
				if (!enabled.includes(key)) enabled.push(key);
			}
			return {
				enabledModules: enabled,
				moduleOrder: order,
				editMode: false
			};
		}
	} catch {
		// Ignore parse errors
	}

	return {
		enabledModules: getDefaultOrder(allModules),
		moduleOrder: getDefaultOrder(allModules),
		editMode: false
	};
}

function persistState(state: DashboardConfigState) {
	if (!browser) return;
	try {
		localStorage.setItem(
			STORAGE_KEY,
			JSON.stringify({
				enabledModules: state.enabledModules,
				moduleOrder: state.moduleOrder
			})
		);
	} catch {
		// Ignore storage errors
	}
}

function createDashboardConfigStore() {
	// We need all modules to initialize defaults; use empty array until hydrated
	const allModules: ModuleDefinition[] = [];
	const initialState = loadState(allModules);

	const { subscribe, set, update } = writable<DashboardConfigState>(initialState);

	return {
		subscribe,

		hydrate: (modules: ModuleDefinition[]) => {
			const state = loadState(modules);
			set(state);
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
				editMode: false
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
