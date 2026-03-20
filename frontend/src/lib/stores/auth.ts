import { writable, derived } from 'svelte/store';
import type { User } from '../api/types';
import { decodeJWT, isTokenExpired } from '../utils/jwt';
import { getStoredToken, setStoredToken, logout } from '../api/auth';
import { getUserProfile } from '../api/users';
import { themeStore } from './theme';
import { initializeWebSocket, cleanupWebSocket } from '../websocket/manager';

interface AuthState {
  user: User | null;
  token: string | null;
  isAuthenticated: boolean;
  isLoading: boolean;
}

function createAuthStore() {
  let initialToken: string | null = null;
  let initialUser: User | null = null;
  let initialIsAuthenticated = false;

  // Only initialize from localStorage in browser
  if (typeof window !== 'undefined') {
    initialToken = getStoredToken();

    if (initialToken && !isTokenExpired(initialToken)) {
      // Token is valid - set authenticated to true
      // User info will be loaded from /users/me endpoint
      initialIsAuthenticated = true;

      // Try to get cached user info from localStorage
      const cachedUser = localStorage.getItem('user');
      if (cachedUser) {
        try {
          initialUser = JSON.parse(cachedUser);
        } catch (e) {
          console.error('Failed to parse cached user:', e);
        }
      }

      // Load fresh user profile from backend
      getUserProfile()
        .then(profile => {
          // Update store with fresh user data
          authStoreInstance.updateUser({
            id: profile.id,
            email: profile.email,
            display_name: profile.display_name,
            is_admin: profile.is_admin
          });
          // Cache user info
          localStorage.setItem('user', JSON.stringify({
            id: profile.id,
            email: profile.email,
            display_name: profile.display_name,
            is_admin: profile.is_admin
          }));
          // Load theme
          themeStore.loadFromBackend(profile.theme);

          // Initialize WebSocket for existing session
          if (initialToken) {
            initializeWebSocket(initialToken, profile.id).catch(err => {
              console.error('[Auth] Failed to initialize WebSocket on load:', err);
            });
          }
        })
        .catch(err => {
          console.error('Failed to load user profile:', err);
          // If 401, token was cleared by API client, logout the store
          if (err.statusCode === 401) {
            localStorage.removeItem('token');
            localStorage.removeItem('user');
            // Will be handled by layout's reactive statement
          }
          // Continue with local theme if API fails
        });
    } else if (initialToken) {
      // Token expired, clear it
      localStorage.removeItem('token');
      localStorage.removeItem('user');
      initialToken = null;  // Important: clear the variable too
    }
  }

  const { subscribe, set, update } = writable<AuthState>({
    user: initialUser,
    token: initialToken,
    isAuthenticated: initialIsAuthenticated,
    isLoading: false
  });

  let authStoreInstance: any;

  authStoreInstance = {
    subscribe,
    login: async (token: string, user: User) => {
      setStoredToken(token);
      // Cache user info
      localStorage.setItem('user', JSON.stringify(user));
      set({
        user,
        token,
        isAuthenticated: true,
        isLoading: false
      });

      // Initialize WebSocket connection for real-time sync
      try {
        await initializeWebSocket(token, user.id);
        console.log('[Auth] WebSocket initialized');
      } catch (error) {
        console.error('[Auth] Failed to initialize WebSocket:', error);
        // Don't fail login if WebSocket fails - it's non-critical
      }

      // Load theme from backend after login
      try {
        const profile = await getUserProfile();
        themeStore.loadFromBackend(profile.theme);
      } catch (err) {
        console.error('Failed to load user profile:', err);
        // Continue with local theme if API fails
      }
    },
    logout: () => {
      // Cleanup WebSocket connection
      cleanupWebSocket();
      console.log('[Auth] WebSocket cleaned up');

      logout();
      localStorage.removeItem('user');
      set({
        user: null,
        token: null,
        isAuthenticated: false,
        isLoading: false
      });
    },
    updateUser: (user: User) => {
      update(state => ({ ...state, user }));
    },
    setLoading: (loading: boolean) => {
      update(state => ({ ...state, isLoading: loading }));
    }
  };

  return authStoreInstance;
}

export const authStore = createAuthStore();

// Derived stores for easy access to auth status
export const isAuthenticated = derived(authStore, $auth => $auth.isAuthenticated);
export const currentUser = derived(authStore, $auth => $auth.user);
