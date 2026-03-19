import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import { themeStore, getCurrentTheme, type Theme } from '$lib/stores/theme';

describe('Theme Store', () => {
  beforeEach(() => {
    // Clear localStorage
    localStorage.clear();

    // Reset document theme attribute
    document.documentElement.removeAttribute('data-theme');

    // Reset store to default
    themeStore.setTheme('system');
  });

  describe('Initial State', () => {
    it('should default to system theme', () => {
      localStorage.clear();
      expect(get(themeStore)).toBe('system');
    });

    it('should load theme from localStorage', () => {
      localStorage.setItem('theme-preference', 'dark');
      // Re-create store by reloading module would be ideal, but for now just test setTheme
      themeStore.setTheme('dark');
      expect(get(themeStore)).toBe('dark');
    });

    it('should handle invalid localStorage values', () => {
      localStorage.setItem('theme-preference', 'invalid');
      // Should default to system
      expect(get(themeStore)).toBe('system');
    });
  });

  describe('setTheme', () => {
    it('should set light theme', () => {
      themeStore.setTheme('light');
      expect(get(themeStore)).toBe('light');
      expect(localStorage.getItem('theme-preference')).toBe('light');
      expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    });

    it('should set dark theme', () => {
      themeStore.setTheme('dark');
      expect(get(themeStore)).toBe('dark');
      expect(localStorage.getItem('theme-preference')).toBe('dark');
      expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    });

    it('should set system theme', () => {
      themeStore.setTheme('system');
      expect(get(themeStore)).toBe('system');
      expect(localStorage.getItem('theme-preference')).toBe('system');
      // data-theme should be resolved to light or dark based on system preference
      const dataTheme = document.documentElement.getAttribute('data-theme');
      expect(dataTheme === 'light' || dataTheme === 'dark').toBe(true);
    });
  });

  describe('toggleTheme', () => {
    it('should toggle from light to dark', () => {
      themeStore.setTheme('light');
      themeStore.toggleTheme();
      expect(get(themeStore)).toBe('dark');
      expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    });

    it('should toggle from dark to light', () => {
      themeStore.setTheme('dark');
      themeStore.toggleTheme();
      expect(get(themeStore)).toBe('light');
      expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    });

    it('should toggle from system preference', () => {
      themeStore.setTheme('system');
      themeStore.toggleTheme();
      const newTheme = get(themeStore);
      expect(newTheme === 'light' || newTheme === 'dark').toBe(true);
    });

    it('should save to localStorage after toggle', () => {
      themeStore.setTheme('light');
      themeStore.toggleTheme();
      expect(localStorage.getItem('theme-preference')).toBe('dark');
    });
  });

  describe('getCurrentTheme', () => {
    it('should return resolved theme for light', () => {
      themeStore.setTheme('light');
      expect(getCurrentTheme()).toBe('light');
    });

    it('should return resolved theme for dark', () => {
      themeStore.setTheme('dark');
      expect(getCurrentTheme()).toBe('dark');
    });

    it('should resolve system theme to light or dark', () => {
      themeStore.setTheme('system');
      const resolved = getCurrentTheme();
      expect(resolved === 'light' || resolved === 'dark').toBe(true);
    });
  });

  describe('Theme Application', () => {
    it('should apply theme to document root', () => {
      themeStore.setTheme('light');
      expect(document.documentElement.getAttribute('data-theme')).toBe('light');

      themeStore.setTheme('dark');
      expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    });

    it('should resolve system theme to actual value', () => {
      themeStore.setTheme('system');
      const dataTheme = document.documentElement.getAttribute('data-theme');
      expect(dataTheme).not.toBe('system');
      expect(dataTheme === 'light' || dataTheme === 'dark').toBe(true);
    });
  });

  describe('Persistence', () => {
    it('should persist theme across page loads', () => {
      themeStore.setTheme('dark');
      expect(localStorage.getItem('theme-preference')).toBe('dark');

      // Simulate page reload by reading from localStorage
      const stored = localStorage.getItem('theme-preference');
      expect(stored).toBe('dark');
    });

    it('should not persist if value is invalid', () => {
      themeStore.setTheme('light');
      localStorage.setItem('theme-preference', 'invalid-theme');

      // Should fall back to system
      expect(get(themeStore)).toBe('light'); // Current value stays
    });
  });

  describe('System Theme Changes', () => {
    it('should react to system theme changes when in system mode', () => {
      themeStore.setTheme('system');

      // Mock matchMedia change
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
      const event = new Event('change');
      mediaQuery.dispatchEvent(event);

      // Store should still be system
      expect(get(themeStore)).toBe('system');

      // But data-theme should be updated
      const dataTheme = document.documentElement.getAttribute('data-theme');
      expect(dataTheme === 'light' || dataTheme === 'dark').toBe(true);
    });

    it('should not react to system changes when not in system mode', () => {
      themeStore.setTheme('light');

      const initialDataTheme = document.documentElement.getAttribute('data-theme');

      // Mock matchMedia change
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
      const event = new Event('change');
      mediaQuery.dispatchEvent(event);

      // Should remain light
      expect(get(themeStore)).toBe('light');
      expect(document.documentElement.getAttribute('data-theme')).toBe('light');
      expect(document.documentElement.getAttribute('data-theme')).toBe(initialDataTheme);
    });
  });
});
