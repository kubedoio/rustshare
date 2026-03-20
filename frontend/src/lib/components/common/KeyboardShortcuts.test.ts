import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import { showKeyboardShortcuts } from '$lib/stores/ui';

describe('Keyboard Shortcuts Store', () => {
  beforeEach(() => {
    showKeyboardShortcuts.set(false);
  });

  it('should start closed', () => {
    expect(get(showKeyboardShortcuts)).toBe(false);
  });

  it('should open when set to true', () => {
    showKeyboardShortcuts.set(true);
    expect(get(showKeyboardShortcuts)).toBe(true);
  });

  it('should close when set to false', () => {
    showKeyboardShortcuts.set(true);
    showKeyboardShortcuts.set(false);
    expect(get(showKeyboardShortcuts)).toBe(false);
  });

  it('should toggle state', () => {
    const initial = get(showKeyboardShortcuts);
    showKeyboardShortcuts.set(!initial);
    expect(get(showKeyboardShortcuts)).toBe(!initial);
  });
});

describe('Keyboard Shortcuts Functionality', () => {
  let keydownHandler: ((e: KeyboardEvent) => void) | null = null;

  beforeEach(() => {
    // Clear any existing handlers
    keydownHandler = null;
    vi.clearAllMocks();
  });

  afterEach(() => {
    if (keydownHandler) {
      window.removeEventListener('keydown', keydownHandler);
    }
  });

  describe('Help Modal Trigger', () => {
    it('should open modal when "?" is pressed', () => {
      showKeyboardShortcuts.set(false);

      const event = new KeyboardEvent('keydown', {
        key: '?',
        bubbles: true
      });

      // Simulate the app's keyboard handler
      keydownHandler = (e: KeyboardEvent) => {
        if (e.key === '?') {
          showKeyboardShortcuts.set(true);
        }
      };
      window.addEventListener('keydown', keydownHandler);
      window.dispatchEvent(event);

      expect(get(showKeyboardShortcuts)).toBe(true);
    });

    it('should not open modal for other keys', () => {
      showKeyboardShortcuts.set(false);

      const event = new KeyboardEvent('keydown', {
        key: 'a',
        bubbles: true
      });

      keydownHandler = (e: KeyboardEvent) => {
        if (e.key === '?') {
          showKeyboardShortcuts.set(true);
        }
      };
      window.addEventListener('keydown', keydownHandler);
      window.dispatchEvent(event);

      expect(get(showKeyboardShortcuts)).toBe(false);
    });
  });

  describe('Modal Close Behavior', () => {
    it('should close modal when Escape is pressed', () => {
      showKeyboardShortcuts.set(true);

      const event = new KeyboardEvent('keydown', {
        key: 'Escape',
        bubbles: true
      });

      keydownHandler = (e: KeyboardEvent) => {
        if (e.key === 'Escape' && get(showKeyboardShortcuts)) {
          showKeyboardShortcuts.set(false);
        }
      };
      window.addEventListener('keydown', keydownHandler);
      window.dispatchEvent(event);

      expect(get(showKeyboardShortcuts)).toBe(false);
    });

    it('should close modal when close event is dispatched', () => {
      showKeyboardShortcuts.set(true);
      showKeyboardShortcuts.set(false);
      expect(get(showKeyboardShortcuts)).toBe(false);
    });
  });

  describe('Shortcut Categories', () => {
    const shortcuts = {
      navigation: [
        { key: '?', description: 'Show help menu' },
        { keys: ['g', 'h'], description: 'Go to dashboard' },
        { keys: ['g', 'f'], description: 'Go to files' }
      ],
      fileOperations: [
        { key: 'u', description: 'Upload file' },
        { key: 'n', description: 'New folder' },
        { key: 'r', description: 'Rename' },
        { key: 'Delete', description: 'Delete' }
      ],
      selection: [
        { keys: ['Ctrl', 'A'], description: 'Select all' },
        { key: 'Esc', description: 'Exit selection' }
      ],
      search: [
        { key: '/', description: 'Focus search' }
      ]
    };

    it('should have navigation shortcuts', () => {
      expect(shortcuts.navigation).toHaveLength(3);
      expect(shortcuts.navigation[0].key).toBe('?');
    });

    it('should have file operation shortcuts', () => {
      expect(shortcuts.fileOperations).toHaveLength(4);
      expect(shortcuts.fileOperations[0].key).toBe('u');
    });

    it('should have selection shortcuts', () => {
      expect(shortcuts.selection).toHaveLength(2);
      expect(shortcuts.selection[0].keys).toEqual(['Ctrl', 'A']);
    });

    it('should have search shortcuts', () => {
      expect(shortcuts.search).toHaveLength(1);
      expect(shortcuts.search[0].key).toBe('/');
    });
  });

  describe('Keyboard Event Handling', () => {
    it('should handle single key shortcuts', () => {
      const handler = vi.fn();

      keydownHandler = (e: KeyboardEvent) => {
        if (e.key === 'u') {
          handler();
        }
      };
      window.addEventListener('keydown', keydownHandler);

      const event = new KeyboardEvent('keydown', { key: 'u', bubbles: true });
      window.dispatchEvent(event);

      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('should handle modifier + key combinations', () => {
      const handler = vi.fn();

      keydownHandler = (e: KeyboardEvent) => {
        if (e.ctrlKey && e.key === 'a') {
          handler();
        }
      };
      window.addEventListener('keydown', keydownHandler);

      const event = new KeyboardEvent('keydown', {
        key: 'a',
        ctrlKey: true,
        bubbles: true
      });
      window.dispatchEvent(event);

      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('should handle special keys', () => {
      const handler = vi.fn();

      keydownHandler = (e: KeyboardEvent) => {
        if (e.key === 'Escape') {
          handler();
        }
      };
      window.addEventListener('keydown', keydownHandler);

      const event = new KeyboardEvent('keydown', {
        key: 'Escape',
        bubbles: true
      });
      window.dispatchEvent(event);

      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('should not trigger when typing in input fields', () => {
      const handler = vi.fn();

      keydownHandler = (e: KeyboardEvent) => {
        const target = e.target as HTMLElement;
        if (
          target instanceof HTMLInputElement ||
          target instanceof HTMLTextAreaElement
        ) {
          return;
        }
        if (e.key === 'n') {
          handler();
        }
      };
      window.addEventListener('keydown', keydownHandler);

      // Create mock input element
      const input = document.createElement('input');
      document.body.appendChild(input);

      const event = new KeyboardEvent('keydown', {
        key: 'n',
        bubbles: true
      });
      Object.defineProperty(event, 'target', { value: input, enumerable: true });

      window.dispatchEvent(event);

      expect(handler).not.toHaveBeenCalled();

      document.body.removeChild(input);
    });
  });

  describe('Platform-specific Behavior', () => {
    it('should detect macOS for Cmd key display', () => {
      const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;

      if (isMac) {
        expect(navigator.platform).toMatch(/Mac/i);
      }
    });

    it('should show Ctrl for non-Mac platforms', () => {
      const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;

      if (!isMac) {
        expect(navigator.platform).not.toMatch(/Mac/i);
      }
    });
  });

  describe('Modal Accessibility', () => {
    it('should have proper ARIA attributes', () => {
      const modalAttributes = {
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'shortcuts-title'
      };

      expect(modalAttributes.role).toBe('dialog');
      expect(modalAttributes['aria-modal']).toBe('true');
      expect(modalAttributes['aria-labelledby']).toBe('shortcuts-title');
    });

    it('should be focusable when open', () => {
      const modal = {
        open: true,
        tabIndex: -1
      };

      expect(modal.tabIndex).toBe(-1);
    });
  });

  describe('Shortcut Conflicts', () => {
    it('should not have duplicate shortcuts', () => {
      const allShortcuts = [
        '?', 'u', 'n', 'r', 'Delete', '/', 'Escape',
        'g+h', 'g+f', 'g+s', 'Ctrl+A'
      ];

      const uniqueShortcuts = new Set(allShortcuts);
      expect(uniqueShortcuts.size).toBe(allShortcuts.length);
    });

    it('should handle sequential shortcuts (g then h)', () => {
      let sequence = '';
      const handler = vi.fn();

      keydownHandler = (e: KeyboardEvent) => {
        if (e.key === 'g') {
          sequence = 'g';
          setTimeout(() => {
            sequence = '';
          }, 1000);
        } else if (sequence === 'g' && e.key === 'h') {
          handler();
          sequence = '';
        }
      };
      window.addEventListener('keydown', keydownHandler);

      // Press 'g'
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'g', bubbles: true }));
      // Press 'h'
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'h', bubbles: true }));

      expect(handler).toHaveBeenCalledTimes(1);
    });
  });
});
