import { writable } from 'svelte/store';

export interface ToastNotification {
  id: string;
  message: string;
  type: 'success' | 'error' | 'info';
  duration?: number;
  actionLabel?: string;
  actionHref?: string;
}

export interface ToastOptions {
  duration?: number;
  actionLabel?: string;
  actionHref?: string;
}

function createToastStore() {
  const { subscribe, update } = writable<ToastNotification[]>([]);

  return {
    subscribe,
    show: (
      message: string,
      type: 'success' | 'error' | 'info' = 'info',
      options: number | ToastOptions = 3000
    ) => {
      const normalized =
        typeof options === 'number'
          ? { duration: options }
          : { duration: 3000, ...options };
      const id = `toast-${Date.now()}-${Math.random()}`;
      const notification: ToastNotification = { id, message, type, ...normalized };

      update(toasts => [...toasts, notification]);

      if ((normalized.duration ?? 0) > 0) {
        setTimeout(() => {
          update(toasts => toasts.filter(t => t.id !== id));
        }, normalized.duration);
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
