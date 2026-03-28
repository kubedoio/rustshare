import { writable } from 'svelte/store';
import { updateUserTheme } from '$lib/api/users';

export type Theme = 'light' | 'dark' | 'system';
type DaisyTheme = 'rustshare-light' | 'rustshare-dark';

const STORAGE_KEY = 'theme-preference';

function canUseDom(): boolean {
	return typeof window !== 'undefined' && typeof document !== 'undefined';
}

// Get system theme preference
function getSystemTheme(): 'light' | 'dark' {
	if (!canUseDom()) return 'light';
	return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

// Load theme from localStorage
function loadTheme(): Theme {
	if (!canUseDom()) return 'system';

	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored === 'light' || stored === 'dark' || stored === 'system') {
		return stored;
	}
	return 'system';
}

// Resolve theme to actual light/dark value
function resolveTheme(theme: Theme): 'light' | 'dark' {
	if (theme === 'system') {
		return getSystemTheme();
	}
	return theme;
}

function resolveDocumentTheme(theme: Theme): DaisyTheme {
	return resolveTheme(theme) === 'dark' ? 'rustshare-dark' : 'rustshare-light';
}

// Apply theme to document
function applyTheme(theme: Theme) {
	if (!canUseDom()) return;

	document.documentElement.setAttribute('data-theme', resolveDocumentTheme(theme));
}

function createThemeStore() {
	const { subscribe, set, update } = writable<Theme>(loadTheme());

	// Apply initial theme
	if (canUseDom()) {
		applyTheme(loadTheme());
	}

	// Listen for system theme changes
	if (canUseDom()) {
		const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
		mediaQuery.addEventListener('change', () => {
			update((currentTheme) => {
				if (currentTheme === 'system') {
					applyTheme('system');
				}
				return currentTheme;
			});
		});
	}

	return {
		subscribe,

		setTheme: (theme: Theme, syncToBackend = false) => {
			if (canUseDom()) {
				localStorage.setItem(STORAGE_KEY, theme);
				applyTheme(theme);
			}
			set(theme);

			// Sync to backend if requested
			if (syncToBackend && canUseDom()) {
				updateUserTheme(theme).catch((err) => {
					console.error('Failed to sync theme to backend:', err);
					// Continue with local theme even if sync fails
				});
			}
		},

		toggleTheme: (syncToBackend = true) => {
			update((currentTheme) => {
				const resolvedTheme = resolveTheme(currentTheme);
				const newTheme: Theme = resolvedTheme === 'light' ? 'dark' : 'light';

				if (canUseDom()) {
					localStorage.setItem(STORAGE_KEY, newTheme);
					applyTheme(newTheme);
				}

				// Sync to backend
				if (syncToBackend && canUseDom()) {
					updateUserTheme(newTheme).catch((err) => {
						console.error('Failed to sync theme to backend:', err);
						// Continue with local theme even if sync fails
					});
				}

				return newTheme;
			});
		},

		loadFromBackend: (theme: Theme) => {
			// Load theme from backend (called after login)
			if (canUseDom()) {
				localStorage.setItem(STORAGE_KEY, theme);
				applyTheme(theme);
			}
			set(theme);
		},

		getResolvedTheme: (): 'light' | 'dark' => {
			let currentTheme: Theme = 'system';
			subscribe((value) => (currentTheme = value))();
			return resolveTheme(currentTheme);
		}
	};
}

export const themeStore = createThemeStore();

// Export helper to get current resolved theme
export function getCurrentTheme(): 'light' | 'dark' {
	return themeStore.getResolvedTheme();
}
