import { writable, derived } from 'svelte/store';
import type { User } from '../api/types';
import { decodeJWT, isTokenExpired } from '../utils/jwt';
import { getStoredToken, setStoredToken, logout } from '../api/auth';

interface AuthState {
  user: User | null;
  token: string | null;
  isAuthenticated: boolean;
  isLoading: boolean;
}

function createAuthStore() {
  const initialToken = getStoredToken();
  let initialUser: User | null = null;
  let initialIsAuthenticated = false;

  if (initialToken && !isTokenExpired(initialToken)) {
    initialUser = decodeJWT(initialToken);
    initialIsAuthenticated = initialUser !== null;
  } else if (initialToken) {
    // Token expired, clear it
    if (typeof window !== 'undefined') {
      localStorage.removeItem('token');
    }
  }

  const { subscribe, set, update } = writable<AuthState>({
    user: initialUser,
    token: initialToken,
    isAuthenticated: initialIsAuthenticated,
    isLoading: false
  });

  return {
    subscribe,
    login: (token: string, user: User) => {
      setStoredToken(token);
      set({
        user,
        token,
        isAuthenticated: true,
        isLoading: false
      });
    },
    logout: () => {
      logout();
      set({
        user: null,
        token: null,
        isAuthenticated: false,
        isLoading: false
      });
    },
    setLoading: (loading: boolean) => {
      update(state => ({ ...state, isLoading: loading }));
    }
  };
}

export const authStore = createAuthStore();

// Derived stores for easy access to auth status
export const isAuthenticated = derived(authStore, $auth => $auth.isAuthenticated);
export const currentUser = derived(authStore, $auth => $auth.user);
