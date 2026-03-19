import { derived, writable } from 'svelte/store';
import type { User } from '../api/types';
import { logout as requestLogout } from '../api/auth';
import { getUserProfile, type UserProfile } from '../api/users';
import { themeStore } from './theme';
import { cleanupWebSocket, initializeWebSocket } from '../websocket/manager';

interface AuthState {
  user: User | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  isInitialized: boolean;
}

function toUser(profile: UserProfile): User {
  return {
    id: profile.id,
    email: profile.email,
    display_name: profile.display_name,
    is_admin: profile.is_admin,
    storage_quota: profile.storage_quota,
    created_at: profile.created_at,
    updated_at: profile.updated_at
  };
}

function createAuthStore() {
  const { subscribe, set, update } = writable<AuthState>({
    user: null,
    isAuthenticated: false,
    isLoading: false,
    isInitialized: false
  });

  let initializePromise: Promise<void> | null = null;

  async function applyProfile(profile: UserProfile): Promise<void> {
    const user = toUser(profile);

    set({
      user,
      isAuthenticated: true,
      isLoading: false,
      isInitialized: true
    });

    themeStore.loadFromBackend(profile.theme);

    try {
      await initializeWebSocket(user.id);
    } catch (error) {
      console.error('[Auth] Failed to initialize WebSocket:', error);
    }
  }

  function clearAuthState(): void {
    cleanupWebSocket();
    set({
      user: null,
      isAuthenticated: false,
      isLoading: false,
      isInitialized: true
    });
  }

  return {
    subscribe,
    async initialize(): Promise<void> {
      if (initializePromise) {
        return initializePromise;
      }

      update((state) => ({ ...state, isLoading: true }));

      initializePromise = (async () => {
        try {
          const profile = await getUserProfile();
          await applyProfile(profile);
        } catch (error: any) {
          if (error?.status !== 401) {
            console.error('[Auth] Failed to bootstrap session:', error);
          }
          clearAuthState();
        } finally {
          initializePromise = null;
        }
      })();

      return initializePromise;
    },
    async login(user: User): Promise<void> {
      set({
        user,
        isAuthenticated: true,
        isLoading: false,
        isInitialized: true
      });

      try {
        const profile = await getUserProfile();
        await applyProfile(profile);
      } catch (error: any) {
        if (error?.status === 401) {
          clearAuthState();
          throw error;
        }

        console.error('[Auth] Failed to refresh profile after login:', error);

        try {
          await initializeWebSocket(user.id);
        } catch (wsError) {
          console.error('[Auth] Failed to initialize WebSocket after login:', wsError);
        }
      }
    },
    logout(): void {
      clearAuthState();
      void requestLogout().catch((error) => {
        console.error('[Auth] Failed to revoke session on logout:', error);
      });

      if (typeof window !== 'undefined') {
        window.location.href = '/login';
      }
    },
    updateUser(user: User): void {
      update((state) => ({ ...state, user }));
    },
    setLoading(loading: boolean): void {
      update((state) => ({ ...state, isLoading: loading }));
    }
  };
}

export const authStore = createAuthStore();

export const isAuthenticated = derived(authStore, ($auth) => $auth.isAuthenticated);
export const currentUser = derived(authStore, ($auth) => $auth.user);
