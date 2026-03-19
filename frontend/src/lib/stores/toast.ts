import { writable } from 'svelte/store';

export interface ToastNotification {
  id: string;
  message: string;
  type: 'success' | 'error' | 'info';
  duration?: number;
}

function createToastStore() {
  const { subscribe, update } = writable<ToastNotification[]>([]);

  return {
    subscribe,
    show: (message: string, type: 'success' | 'error' | 'info' = 'info', duration = 3000) => {
      const id = `toast-${Date.now()}-${Math.random()}`;
      const notification: ToastNotification = { id, message, type, duration };

      update(toasts => [...toasts, notification]);

      if (duration > 0) {
        setTimeout(() => {
          update(toasts => toasts.filter(t => t.id !== id));
        }, duration);
      }

      return id;
    },
    dismiss: (id: string) => {
      update(toasts => toasts.filter(t => t.id !== id));
    },
    clear: () => {
      update(() => []);
    }
  };
}

export const toastStore = createToastStore();
