import { writable } from 'svelte/store';
import { browser } from '$app/environment';

export type Theme = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'theme-preference';

// Get system theme preference
function getSystemTheme(): 'light' | 'dark' {
  if (!browser) return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

// Load theme from localStorage
function loadTheme(): Theme {
  if (!browser) return 'system';

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

// Apply theme to document
function applyTheme(theme: Theme) {
  if (!browser) return;

  const resolvedTheme = resolveTheme(theme);
  document.documentElement.setAttribute('data-theme', resolvedTheme);
}

function createThemeStore() {
  const { subscribe, set, update } = writable<Theme>(loadTheme());

  // Apply initial theme
  if (browser) {
    applyTheme(loadTheme());
  }

  // Listen for system theme changes
  if (browser) {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaQuery.addEventListener('change', () => {
      update(currentTheme => {
        if (currentTheme === 'system') {
          applyTheme('system');
        }
        return currentTheme;
      });
    });
  }

  return {
    subscribe,

    setTheme: (theme: Theme) => {
      if (browser) {
        localStorage.setItem(STORAGE_KEY, theme);
        applyTheme(theme);
      }
      set(theme);
    },

    toggleTheme: () => {
      update(currentTheme => {
        const resolvedTheme = resolveTheme(currentTheme);
        const newTheme: Theme = resolvedTheme === 'light' ? 'dark' : 'light';

        if (browser) {
          localStorage.setItem(STORAGE_KEY, newTheme);
          applyTheme(newTheme);
        }

        return newTheme;
      });
    },

    getResolvedTheme: (): 'light' | 'dark' => {
      let currentTheme: Theme = 'system';
      subscribe(value => currentTheme = value)();
      return resolveTheme(currentTheme);
    }
  };
}

export const themeStore = createThemeStore();

// Export helper to get current resolved theme
export function getCurrentTheme(): 'light' | 'dark' {
  return themeStore.getResolvedTheme();
}
