import { writable, derived } from 'svelte/store';
import type { User } from '../api/types';
import { login as loginRequest, logout as logoutRequest } from '../api/auth';
import { getUserProfile } from '../api/users';
import { replicationStore } from './replication';
import { themeStore } from './theme';
import { initializeWebSocket, cleanupWebSocket } from '../websocket/manager';
import { queryClient } from '../query-client';

interface AuthState {
	user: User | null;
	isAuthenticated: boolean;
	isLoading: boolean;
}

const WEBSOCKET_TOKEN_KEY = 'rustshare.websocket_token';

function isApiError(error: unknown): error is { status: number } {
	return typeof error === 'object' && error !== null && 'status' in error;
}

function clearLegacyWebSocketToken(): void {
	if (typeof window === 'undefined') {
		return;
	}

	window.sessionStorage.removeItem(WEBSOCKET_TOKEN_KEY);
}

function toAuthUser(profile: Awaited<ReturnType<typeof getUserProfile>>): User {
	return {
		id: profile.id,
		tenant_id: profile.tenant_id,
		email: profile.email,
		display_name: profile.display_name,
		is_admin: profile.is_admin,
		avatar_path: profile.avatar_path,
		storage_quota: profile.storage_quota,
		storage_used: profile.storage_used
	};
}

function createAuthStore() {
	const { subscribe, set, update } = writable<AuthState>({
		user: null,
		isAuthenticated: false,
		isLoading: true
	});

	let sessionGeneration = 0;

	function nextGeneration(): number {
		return ++sessionGeneration;
	}

	async function bootstrapSession() {
		const myGeneration = nextGeneration();
		try {
			const profile = await getUserProfile();
			if (myGeneration !== sessionGeneration) return;

			const user = toAuthUser(profile);

			set({
				user,
				isAuthenticated: true,
				isLoading: false
			});

			themeStore.loadFromBackend(profile.theme);
			clearLegacyWebSocketToken();
			try {
				await initializeWebSocket(null, profile.id);
			} catch (error) {
				console.error('Failed to initialize WebSocket during bootstrap:', error);
			}
		} catch (error: unknown) {
			if (!isApiError(error) || error.status !== 401) {
				console.error('Failed to bootstrap session:', error);
			}

			cleanupWebSocket();
			replicationStore.reset();
			clearLegacyWebSocketToken();
			if (myGeneration !== sessionGeneration) return;
			set({
				user: null,
				isAuthenticated: false,
				isLoading: false
			});
		}
	}

	if (typeof window !== 'undefined') {
		void bootstrapSession();
	}

	return {
		subscribe,
		login: async (email: string, password: string) => {
			const myGeneration = nextGeneration();
			update((state) => ({ ...state, isLoading: true }));

			try {
				const response = await loginRequest(email, password);
				const user = response.user;
				clearLegacyWebSocketToken();

				if (myGeneration !== sessionGeneration) return;

				set({
					user,
					isAuthenticated: true,
					isLoading: false
				});

				queryClient.invalidateQueries();

				try {
					await initializeWebSocket(null, user.id);
				} catch (error) {
					console.error('Failed to initialize WebSocket after login:', error);
				}

				try {
					const profile = await getUserProfile();
					themeStore.loadFromBackend(profile.theme);
					if (myGeneration !== sessionGeneration) return;
					update((state) => ({
						...state,
						user: toAuthUser(profile)
					}));
				} catch (error) {
					console.error('Failed to load user profile after login:', error);
				}
			} catch (error) {
				if (myGeneration !== sessionGeneration) return;
				update((state) => ({ ...state, isLoading: false }));
				throw error;
			}
		},
		logout: async () => {
			const myGeneration = nextGeneration();
			cleanupWebSocket();
			replicationStore.reset();
			clearLegacyWebSocketToken();
			await logoutRequest();
			if (myGeneration !== sessionGeneration) return;
			set({
				user: null,
				isAuthenticated: false,
				isLoading: false
			});
			queryClient.clear();
		},
		updateUser: (user: User) => {
			update((state) => ({ ...state, user }));
		},
		setLoading: (loading: boolean) => {
			update((state) => ({ ...state, isLoading: loading }));
		},
		refreshSession: bootstrapSession
	};
}

export const authStore = createAuthStore();

export const isAuthenticated = derived(authStore, ($auth) => $auth.isAuthenticated);
export const currentUser = derived(authStore, ($auth) => $auth.user);
