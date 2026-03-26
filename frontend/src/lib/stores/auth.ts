import { writable, derived } from "svelte/store";
import type { User } from "../api/types";
import { login as loginRequest, logout as logoutRequest } from "../api/auth";
import { getUserProfile } from "../api/users";
import { replicationStore } from "./replication";
import { themeStore } from "./theme";
import { initializeWebSocket, cleanupWebSocket } from "../websocket/manager";

interface AuthState {
  user: User | null;
  isAuthenticated: boolean;
  isLoading: boolean;
}

const WEBSOCKET_TOKEN_KEY = "rustshare.websocket_token";

function saveWebSocketToken(token: string | null | undefined): void {
  if (typeof window === "undefined") {
    return;
  }

  if (token && token.trim()) {
    window.sessionStorage.setItem(WEBSOCKET_TOKEN_KEY, token.trim());
  } else {
    window.sessionStorage.removeItem(WEBSOCKET_TOKEN_KEY);
  }
}

function loadWebSocketToken(): string | null {
  if (typeof window === "undefined") {
    return null;
  }

  return window.sessionStorage.getItem(WEBSOCKET_TOKEN_KEY);
}

function toAuthUser(profile: Awaited<ReturnType<typeof getUserProfile>>): User {
  return {
    id: profile.id,
    email: profile.email,
    display_name: profile.display_name,
    is_admin: profile.is_admin,
    avatar_path: profile.avatar_path,
  };
}

function createAuthStore() {
  const { subscribe, set, update } = writable<AuthState>({
    user: null,
    isAuthenticated: false,
    isLoading: true,
  });

  async function bootstrapSession() {
    try {
      const profile = await getUserProfile();
      const user = toAuthUser(profile);

      set({
        user,
        isAuthenticated: true,
        isLoading: false,
      });

      themeStore.loadFromBackend(profile.theme);
      try {
        await initializeWebSocket(loadWebSocketToken(), profile.id);
      } catch (error) {
        console.error(
          "Failed to initialize WebSocket during bootstrap:",
          error,
        );
      }
    } catch (error: any) {
      if (error?.status !== 401) {
        console.error("Failed to bootstrap session:", error);
      }

      cleanupWebSocket();
      replicationStore.reset();
      saveWebSocketToken(null);
      set({
        user: null,
        isAuthenticated: false,
        isLoading: false,
      });
    }
  }

  if (typeof window !== "undefined") {
    void bootstrapSession();
  }

  return {
    subscribe,
    login: async (email: string, password: string) => {
      update((state) => ({ ...state, isLoading: true }));

      try {
        const response = await loginRequest(email, password);
        const user = response.user;
        saveWebSocketToken(response.token);

        set({
          user,
          isAuthenticated: true,
          isLoading: false,
        });

        try {
          await initializeWebSocket(response.token ?? null, user.id);
        } catch (error) {
          console.error("Failed to initialize WebSocket after login:", error);
        }

        try {
          const profile = await getUserProfile();
          themeStore.loadFromBackend(profile.theme);
          update((state) => ({
            ...state,
            user: toAuthUser(profile),
          }));
        } catch (error) {
          console.error("Failed to load user profile after login:", error);
        }
      } catch (error) {
        update((state) => ({ ...state, isLoading: false }));
        throw error;
      }
    },
    logout: async () => {
      cleanupWebSocket();
      replicationStore.reset();
      saveWebSocketToken(null);
      await logoutRequest();
      set({
        user: null,
        isAuthenticated: false,
        isLoading: false,
      });
    },
    updateUser: (user: User) => {
      update((state) => ({ ...state, user }));
    },
    setLoading: (loading: boolean) => {
      update((state) => ({ ...state, isLoading: loading }));
    },
    refreshSession: bootstrapSession,
  };
}

export const authStore = createAuthStore();

export const isAuthenticated = derived(
  authStore,
  ($auth) => $auth.isAuthenticated,
);
export const currentUser = derived(authStore, ($auth) => $auth.user);
