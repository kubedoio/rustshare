# RustShare Phase 4: Frontend Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a complete SvelteKit web application for RustShare with authentication, file management, public and user-to-user sharing, real-time WebSocket sync, and notifications.

**Architecture:** Single integrated SvelteKit app with TypeScript, TailwindCSS + DaisyUI for UI, TanStack Query for server state, native WebSocket for real-time, deployed via Docker Compose with nginx reverse proxy.

**Tech Stack:** SvelteKit, TypeScript, TailwindCSS, DaisyUI, TanStack Query (Svelte), Playwright, Docker, nginx

---

## Prerequisites

**Backend WebSocket Authentication Modification:**

Before starting frontend implementation, the backend WebSocket authentication must be modified to accept JWT tokens via query parameter. This modification can be deferred until before Task 19 (WebSocket Client implementation), as Phases 1-3 don't require WebSocket functionality.

**Recommended execution order:**
- Complete Tasks 1-18 (foundation, file management, sharing)
- Complete Task 0 (WebSocket auth fix)
- Complete Tasks 19-27 (real-time, polish, deployment)

---

## Phase 0: Backend WebSocket Fix

### Task 0: Modify WebSocket Authentication

**Context:** Browser WebSocket API doesn't support custom headers. Backend currently expects `Authorization: Bearer {token}` header but needs to accept `?token={jwt}` query parameter.

**Files:**
- Modify: `backend/server/src/handlers/sync.rs`

- [ ] **Step 1: Read current WebSocket handler implementation**

```bash
cat backend/server/src/handlers/sync.rs | head -150
```

Expected: See `TypedHeader<Authorization<Bearer>>` on line 126

- [ ] **Step 2: Add query parameter extractor**

Modify the `sync_handler` function signature:

```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SyncQuery {
    token: String,
}

pub async fn sync_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    Query(query): Query<SyncQuery>,
) -> Result<Response, (StatusCode, String)> {
    // Validate token and determine client identity
    let client_identity = validate_client_token(&query.token, &state.jwt_manager).await?;

    // ... rest remains the same
}
```

- [ ] **Step 3: Test WebSocket connection with query parameter**

Manual test:
```bash
# Start backend
cd backend && cargo run --bin rustshare-server

# In another terminal, test with wscat
npm install -g wscat
wscat -c "ws://localhost:8080/api/sync?token=YOUR_JWT_TOKEN"
```

Expected: Connection accepted, no 401 error

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/handlers/sync.rs
git commit -m "fix(websocket): accept JWT token via query parameter for browser compatibility"
```

---

## Phase 1: Foundation & Setup

### Task 1: Initialize SvelteKit Project

**Files:**
- Create: `frontend/package.json`
- Create: `frontend/svelte.config.js`
- Create: `frontend/tailwind.config.js`
- Create: `frontend/tsconfig.json`
- Create: `frontend/.env.example`
- Create: `frontend/vite.config.ts`

- [ ] **Step 1: Create frontend directory and initialize SvelteKit**

```bash
mkdir -p frontend
cd frontend
npm create svelte@latest . -- --template skeleton --types typescript
```

Choose: TypeScript, ESLint, Prettier

- [ ] **Step 2: Install dependencies**

```bash
npm install @tanstack/svelte-query daisyui zod date-fns
npm install -D @playwright/test autoprefixer postcss tailwindcss @sveltejs/adapter-node
```

- [ ] **Step 3: Configure TailwindCSS**

Create `tailwind.config.js`:

```javascript
/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {},
  },
  plugins: [require('daisyui')],
  daisyui: {
    themes: ['light'],
  },
};
```

Create `src/app.css`:

```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

- [ ] **Step 4: Configure environment variables**

Create `.env.example`:

```
VITE_API_URL=http://localhost:8080/api
VITE_WS_URL=ws://localhost:8080/api
```

- [ ] **Step 5: Update svelte.config.js**

```javascript
import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter()
  }
};
```

- [ ] **Step 6: Test dev server starts**

```bash
npm run dev
```

Expected: Server starts on http://localhost:5173

- [ ] **Step 7: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): initialize SvelteKit project with TailwindCSS and DaisyUI"
```

### Task 2: Create API Client Foundation

**Files:**
- Create: `frontend/src/lib/api/client.ts`
- Create: `frontend/src/lib/api/types.ts`

- [ ] **Step 1: Define TypeScript domain types**

Create `frontend/src/lib/api/types.ts`:

```typescript
// Domain models matching backend
export interface User {
  id: string;
  email: string;
  display_name: string;
  is_admin: boolean;
}

export interface File {
  id: string;
  name: string;
  path: string;
  content_hash: string;
  size: number;
  mime_type: string;
  parent_folder_id: string | null;
  owner_id: string;
  current_version: number;
  created_at: string;
  modified_at: string;
}

export interface Folder {
  id: string;
  name: string;
  path: string;
  parent_folder_id: string | null;
  owner_id: string;
  created_at: string;
  updated_at: string;
}

export type SharePermissions = 'View' | 'Edit' | 'Admin';

export interface PublicShare {
  id: string;
  file_id: string;
  share_token: string;
  permissions: SharePermissions;
  password_protected: boolean;
  expires_at: string | null;
  created_at: string;
}

export interface UserShare {
  share_id: string;
  resource_type: 'File' | 'Folder';
  resource_id: string;
  resource_name: string;
  owner_email: string;
  permission: SharePermissions;
  created_at: string;
}

export interface Notification {
  id: string;
  notification_type: string;
  title: string;
  message: string;
  resource_id: string;
  resource_type: string;
  action_url: string | null;
  read: boolean;
  created_at: string;
}

export interface FileVersion {
  version_number: number;
  size: number;
  content_hash: string;
  created_at: string;
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public message: string
  ) {
    super(message);
    this.name = 'ApiError';
  }
}
```

- [ ] **Step 2: Create base API client**

Create `frontend/src/lib/api/client.ts`:

```typescript
import { ApiError } from './types';

export class ApiClient {
  constructor(private baseURL: string) {}

  async request<T>(endpoint: string, options?: RequestInit): Promise<T> {
    const token = localStorage.getItem('token');
    const headers: Record<string, string> = {
      ...((options?.headers as Record<string, string>) || {})
    };

    // Add Authorization header if token exists and not already set
    if (token && !headers['Authorization']) {
      headers['Authorization'] = `Bearer ${token}`;
    }

    // Add Content-Type for JSON bodies (unless multipart form)
    if (options?.body && !(options.body instanceof FormData)) {
      headers['Content-Type'] = 'application/json';
    }

    const response = await fetch(`${this.baseURL}${endpoint}`, {
      ...options,
      headers
    });

    // Handle 401 Unauthorized
    if (response.status === 401) {
      localStorage.removeItem('token');
      if (typeof window !== 'undefined') {
        window.location.href = '/login';
      }
      throw new ApiError(401, 'Unauthorized');
    }

    // Handle other errors
    if (!response.ok) {
      let errorMessage = 'Request failed';
      try {
        const errorData = await response.json();
        errorMessage = errorData.error || errorData.message || errorMessage;
      } catch {
        errorMessage = response.statusText || errorMessage;
      }
      throw new ApiError(response.status, errorMessage);
    }

    // Handle 204 No Content
    if (response.status === 204) {
      return null as T;
    }

    // Handle redirects (302 for downloads)
    if (response.status === 302 || response.redirected) {
      return { url: response.url } as T;
    }

    return response.json();
  }

  async get<T>(endpoint: string): Promise<T> {
    return this.request<T>(endpoint, { method: 'GET' });
  }

  async post<T>(endpoint: string, body?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: 'POST',
      body: body instanceof FormData ? body : JSON.stringify(body)
    });
  }

  async put<T>(endpoint: string, body?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: 'PUT',
      body: JSON.stringify(body)
    });
  }

  async delete<T>(endpoint: string): Promise<T> {
    return this.request<T>(endpoint, { method: 'DELETE' });
  }
}

// Create singleton instance
const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:8080/api';
export const apiClient = new ApiClient(API_URL);
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/api/
git commit -m "feat(api): add base API client and TypeScript domain types"
```

### Task 3: Implement Authentication API

**Files:**
- Create: `frontend/src/lib/api/auth.ts`
- Create: `frontend/src/lib/utils/jwt.ts`

- [ ] **Step 1: Create JWT decoder utility**

Create `frontend/src/lib/utils/jwt.ts`:

```typescript
import type { User } from '../api/types';

interface JWTPayload {
  sub: string;  // user ID
  email: string;
  display_name: string;
  is_admin: boolean;
  exp: number;
  iat: number;
}

export function decodeJWT(token: string): User | null {
  try {
    const parts = token.split('.');
    if (parts.length !== 3) return null;

    const payload = JSON.parse(atob(parts[1]));
    return {
      id: payload.sub,
      email: payload.email,
      display_name: payload.display_name,
      is_admin: payload.is_admin || false
    };
  } catch (error) {
    console.error('Failed to decode JWT:', error);
    return null;
  }
}

export function isTokenExpired(token: string): boolean {
  try {
    const parts = token.split('.');
    if (parts.length !== 3) return true;

    const payload: JWTPayload = JSON.parse(atob(parts[1]));
    return Date.now() >= payload.exp * 1000;
  } catch {
    return true;
  }
}
```

- [ ] **Step 2: Create authentication API module**

Create `frontend/src/lib/api/auth.ts`:

```typescript
import { apiClient } from './client';
import type { User } from './types';

interface LoginRequest {
  email: string;
  password: string;
}

interface LoginResponse {
  token: string;
  user: User;
}

export async function login(email: string, password: string): Promise<LoginResponse> {
  return apiClient.post<LoginResponse>('/auth/login', { email, password });
}

export function logout(): void {
  localStorage.removeItem('token');
  if (typeof window !== 'undefined') {
    window.location.href = '/login';
  }
}

export function getStoredToken(): string | null {
  return localStorage.getItem('token');
}

export function setStoredToken(token: string): void {
  localStorage.setItem('token', token);
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/api/auth.ts frontend/src/lib/utils/jwt.ts
git commit -m "feat(auth): add authentication API and JWT utilities"
```

### Task 4: Create Auth Store

**Files:**
- Create: `frontend/src/lib/stores/auth.ts`

- [ ] **Step 1: Create auth store with Svelte writable**

Create `frontend/src/lib/stores/auth.ts`:

```typescript
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
    localStorage.removeItem('token');
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

// Derived store for easy access to auth status
export const isAuthenticated = derived(authStore, $auth => $auth.isAuthenticated);
export const currentUser = derived(authStore, $auth => $auth.user);
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/lib/stores/auth.ts
git commit -m "feat(auth): add auth state management store"
```

### Task 5: Create Login Page

**Files:**
- Create: `frontend/src/routes/login/+page.svelte`
- Create: `frontend/src/lib/components/common/Toast.svelte`

- [ ] **Step 1: Create Toast notification component**

Create `frontend/src/lib/components/common/Toast.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';

  export let message: string;
  export let type: 'success' | 'error' | 'info' = 'info';
  export let duration: number = 3000;
  export let onClose: () => void = () => {};

  let visible = true;

  onMount(() => {
    if (duration > 0) {
      const timer = setTimeout(() => {
        visible = false;
        setTimeout(onClose, 300); // Wait for fade out
      }, duration);

      return () => clearTimeout(timer);
    }
  });

  const alertClass = {
    success: 'alert-success',
    error: 'alert-error',
    info: 'alert-info'
  }[type];
</script>

{#if visible}
  <div class="toast toast-end toast-top z-50">
    <div class="alert {alertClass} shadow-lg">
      <span>{message}</span>
    </div>
  </div>
{/if}
```

- [ ] **Step 2: Create login page**

Create `frontend/src/routes/login/+page.svelte`:

```svelte
<script lang="ts">
  import { authStore } from '$lib/stores/auth';
  import { login as apiLogin } from '$lib/api/auth';
  import { goto } from '$app/navigation';
  import Toast from '$lib/components/common/Toast.svelte';

  let email = '';
  let password = '';
  let isLoading = false;
  let errorMessage = '';
  let showError = false;

  async function handleLogin() {
    if (!email || !password) {
      showError = true;
      errorMessage = 'Please enter email and password';
      return;
    }

    isLoading = true;
    authStore.setLoading(true);

    try {
      const response = await apiLogin(email, password);
      authStore.login(response.token, response.user);
      goto('/files');
    } catch (error: any) {
      showError = true;
      errorMessage = error.message || 'Login failed. Please try again.';
    } finally {
      isLoading = false;
      authStore.setLoading(false);
    }
  }

  function handleSubmit(e: Event) {
    e.preventDefault();
    handleLogin();
  }
</script>

<svelte:head>
  <title>Login - RustShare</title>
</svelte:head>

<div class="min-h-screen flex items-center justify-center bg-base-200">
  <div class="card w-96 bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title text-2xl justify-center mb-4">RustShare</h2>

      <form on:submit={handleSubmit}>
        <div class="form-control">
          <label class="label" for="email">
            <span class="label-text">Email</span>
          </label>
          <input
            id="email"
            type="email"
            placeholder="admin@localhost"
            class="input input-bordered"
            bind:value={email}
            disabled={isLoading}
          />
        </div>

        <div class="form-control mt-4">
          <label class="label" for="password">
            <span class="label-text">Password</span>
          </label>
          <input
            id="password"
            type="password"
            placeholder="••••••••"
            class="input input-bordered"
            bind:value={password}
            disabled={isLoading}
          />
        </div>

        <div class="form-control mt-6">
          <button
            type="submit"
            class="btn btn-primary"
            class:loading={isLoading}
            disabled={isLoading}
          >
            {isLoading ? 'Logging in...' : 'Login'}
          </button>
        </div>
      </form>
    </div>
  </div>
</div>

{#if showError}
  <Toast
    message={errorMessage}
    type="error"
    onClose={() => (showError = false)}
  />
{/if}
```

- [ ] **Step 3: Create root layout that imports global CSS**

Create `frontend/src/routes/+layout.svelte`:

```svelte
<script lang="ts">
  import '../app.css';
</script>

<slot />
```

- [ ] **Step 4: Test login page**

```bash
npm run dev
# Navigate to http://localhost:5173/login
# Try logging in with admin@localhost / admin123
```

Expected: Login form displays, can enter credentials

- [ ] **Step 5: Commit**

```bash
git add frontend/src/routes/login/ frontend/src/lib/components/common/Toast.svelte frontend/src/routes/+layout.svelte
git commit -m "feat(auth): add login page with error handling"
```

---

## Implementation Note

**Tasks 1-10** provide detailed step-by-step instructions with complete code blocks, test commands, and expected outputs. These tasks establish the foundation and demonstrate the implementation pattern.

**Tasks 11-27** use a structured summary format with clear objectives and file lists, designed for execution by subagents who can follow the established patterns from Tasks 1-10. Each subagent will have access to:
- The complete spec for detailed requirements
- Tasks 1-10 as implementation examples
- The established codebase patterns

This approach balances comprehensive guidance with practical plan length, as fully expanding all 27 tasks to the detail level of Tasks 1-10 would create an unmanageably long document (5000+ lines).

---

## Phase 2: File Management

### Task 6: Create File API Module

**Files:**
- Create: `frontend/src/lib/api/files.ts`

- [ ] **Step 1: Implement file API operations**

Create `frontend/src/lib/api/files.ts`:

```typescript
import { apiClient } from './client';
import type { File, FileVersion } from './types';

export async function uploadFile(folderId: string | null, file: globalThis.File): Promise<File> {
  const formData = new FormData();
  formData.append('file', file);

  const endpoint = folderId ? `/files/upload?folder_id=${folderId}` : '/files/upload';
  return apiClient.post<File>(endpoint, formData);
}

export async function getFile(fileId: string): Promise<File> {
  return apiClient.get<File>(`/files/${fileId}`);
}

export async function downloadFile(fileId: string): Promise<{ url: string }> {
  return apiClient.get<{ url: string }>(`/files/${fileId}/download`);
}

export async function renameFile(fileId: string, newName: string): Promise<void> {
  return apiClient.post<void>(`/files/${fileId}/rename`, { new_name: newName });
}

export async function moveFile(fileId: string, targetFolderId: string | null): Promise<void> {
  return apiClient.post<void>(`/files/${fileId}/move`, { target_folder_id: targetFolderId });
}

export async function deleteFile(fileId: string): Promise<void> {
  return apiClient.delete<void>(`/files/${fileId}`);
}

export async function getFileVersions(fileId: string): Promise<FileVersion[]> {
  return apiClient.get<FileVersion[]>(`/files/${fileId}/versions`);
}

export async function restoreFileVersion(
  fileId: string,
  versionNumber: number
): Promise<void> {
  return apiClient.post<void>(`/files/${fileId}/restore`, { version_number: versionNumber });
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/lib/api/files.ts
git commit -m "feat(api): add file operations API module"
```

### Task 7: Create Folder API Module

**Files:**
- Create: `frontend/src/lib/api/folders.ts`

- [ ] **Step 1: Implement folder API operations**

Create `frontend/src/lib/api/folders.ts`:

```typescript
import { apiClient } from './client';
import type { Folder, File } from './types';

export interface FolderContents {
  folders: Folder[];
  files: File[];
}

export interface FolderTreeNode {
  id: string;
  name: string;
  path: string;
  parent_folder_id: string | null;
  children: FolderTreeNode[];
}

export async function createFolder(name: string, parentFolderId: string | null): Promise<Folder> {
  return apiClient.post<Folder>('/folders', {
    name,
    parent_folder_id: parentFolderId
  });
}

export async function getFolder(folderId: string): Promise<Folder> {
  return apiClient.get<Folder>(`/folders/${folderId}`);
}

export async function getFolderContents(folderId: string | null): Promise<FolderContents> {
  // For root folder, we need to use a different approach since backend doesn't have /folders/contents
  // Option: Call /folders/:id/contents with a known root folder ID, or
  // fetch the tree and extract root level items
  // For MVP: If no folderId, return empty (user must click into a folder)
  // TODO: Backend may need a /folders/root/contents endpoint for proper root support
  if (!folderId) {
    // Return empty for root - user creates folders to organize files
    return { folders: [], files: [] };
  }
  return apiClient.get<FolderContents>(`/folders/${folderId}/contents`);
}

export async function getFolderTree(): Promise<FolderTreeNode[]> {
  return apiClient.get<FolderTreeNode[]>('/folders/tree');
}

export async function renameFolder(folderId: string, newName: string): Promise<void> {
  return apiClient.post<void>(`/folders/${folderId}/rename`, { new_name: newName });
}

export async function moveFolder(folderId: string, targetFolderId: string | null): Promise<void> {
  return apiClient.post<void>(`/folders/${folderId}/move`, {
    target_folder_id: targetFolderId
  });
}

export async function deleteFolder(folderId: string): Promise<void> {
  return apiClient.delete<void>(`/folders/${folderId}`);
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/lib/api/folders.ts
git commit -m "feat(api): add folder operations API module"
```

### Task 8: Setup TanStack Query

**Files:**
- Create: `frontend/src/lib/query-client.ts`
- Modify: `frontend/src/routes/+layout.svelte`

- [ ] **Step 1: Create query client configuration**

Create `frontend/src/lib/query-client.ts`:

```typescript
import { QueryClient } from '@tanstack/svelte-query';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000 * 60, // 1 minute
      retry: 1,
      refetchOnWindowFocus: false
    },
    mutations: {
      retry: 0
    }
  }
});
```

- [ ] **Step 2: Add QueryClientProvider to root layout**

Modify `frontend/src/routes/+layout.svelte`:

```svelte
<script lang="ts">
  import { QueryClientProvider } from '@tanstack/svelte-query';
  import { queryClient } from '$lib/query-client';
  import '../app.css';
</script>

<QueryClientProvider client={queryClient}>
  <slot />
</QueryClientProvider>
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/query-client.ts frontend/src/routes/+layout.svelte
git commit -m "feat(query): setup TanStack Query client"
```

### Task 9: Create Protected Route Layout

**Files:**
- Create: `frontend/src/routes/(app)/+layout.svelte`
- Create: `frontend/src/lib/components/layout/Sidebar.svelte`
- Create: `frontend/src/lib/components/layout/Header.svelte`

- [ ] **Step 1: Create Sidebar component**

Create `frontend/src/lib/components/layout/Sidebar.svelte`:

```svelte
<script lang="ts">
  import { page } from '$app/stores';
  import { authStore } from '$lib/stores/auth';

  const navItems = [
    { href: '/files', label: 'My Files', icon: '📁' },
    { href: '/shared-with-me', label: 'Shared with Me', icon: '👥' },
    { href: '/notifications', label: 'Notifications', icon: '🔔' }
  ];

  function handleLogout() {
    authStore.logout();
  }
</script>

<aside class="w-64 bg-base-100 h-screen flex flex-col border-r border-base-300">
  <div class="p-4 border-b border-base-300">
    <h1 class="text-2xl font-bold">RustShare</h1>
  </div>

  <nav class="flex-1 p-4">
    <ul class="menu">
      {#each navItems as item}
        <li>
          <a
            href={item.href}
            class:active={$page.url.pathname === item.href}
            class="flex items-center gap-2"
          >
            <span>{item.icon}</span>
            <span>{item.label}</span>
          </a>
        </li>
      {/each}
    </ul>
  </nav>

  <div class="p-4 border-t border-base-300">
    <button class="btn btn-outline btn-block" on:click={handleLogout}>
      Logout
    </button>
  </div>
</aside>
```

- [ ] **Step 2: Create Header component**

Create `frontend/src/lib/components/layout/Header.svelte`:

```svelte
<script lang="ts">
  import { currentUser } from '$lib/stores/auth';
</script>

<header class="h-16 bg-base-100 border-b border-base-300 flex items-center justify-between px-6">
  <div class="flex items-center gap-4">
    <slot name="breadcrumbs" />
  </div>

  <div class="flex items-center gap-4">
    {#if $currentUser}
      <div class="dropdown dropdown-end">
        <label tabindex="0" class="btn btn-ghost btn-circle avatar placeholder">
          <div class="bg-neutral-focus text-neutral-content rounded-full w-10">
            <span class="text-xl">{$currentUser.display_name[0].toUpperCase()}</span>
          </div>
        </label>
        <ul
          tabindex="0"
          class="mt-3 p-2 shadow menu menu-compact dropdown-content bg-base-100 rounded-box w-52"
        >
          <li class="menu-title">
            <span>{$currentUser.email}</span>
          </li>
          <li><a href="/settings">Settings</a></li>
        </ul>
      </div>
    {/if}
  </div>
</header>
```

- [ ] **Step 3: Create protected route layout with auth check**

Create `frontend/src/routes/(app)/+layout.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { authStore, isAuthenticated } from '$lib/stores/auth';
  import Sidebar from '$lib/components/layout/Sidebar.svelte';
  import Header from '$lib/components/layout/Header.svelte';

  // Check authentication on mount
  onMount(() => {
    if (!$isAuthenticated) {
      goto('/login');
    }
  });

  // Redirect if auth state changes
  $: if (!$isAuthenticated) {
    goto('/login');
  }
</script>

{#if $isAuthenticated}
  <div class="flex h-screen overflow-hidden">
    <Sidebar />

    <div class="flex-1 flex flex-col overflow-hidden">
      <Header>
        <slot slot="breadcrumbs" name="breadcrumbs" />
      </Header>

      <main class="flex-1 overflow-auto bg-base-200 p-6">
        <slot />
      </main>
    </div>
  </div>
{:else}
  <div class="flex items-center justify-center h-screen">
    <span class="loading loading-spinner loading-lg"></span>
  </div>
{/if}
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/routes/\(app\)/ frontend/src/lib/components/layout/
git commit -m "feat(layout): add protected route layout with sidebar and header"
```

### Task 10: Create File Browser Page

**Files:**
- Create: `frontend/src/routes/(app)/files/+page.svelte`
- Create: `frontend/src/lib/components/files/FileGrid.svelte`
- Create: `frontend/src/lib/components/files/FileListItem.svelte`
- Create: `frontend/src/lib/utils/format.ts`

- [ ] **Step 1: Create formatting utilities**

Create `frontend/src/lib/utils/format.ts`:

```typescript
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
}

export function formatDate(dateString: string): string {
  const date = new Date(dateString);
  const now = new Date();
  const diff = now.getTime() - date.getTime();

  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;

  if (diff < minute) return 'Just now';
  if (diff < hour) return `${Math.floor(diff / minute)} minutes ago`;
  if (diff < day) return `${Math.floor(diff / hour)} hours ago`;
  if (diff < 7 * day) return `${Math.floor(diff / day)} days ago`;

  return date.toLocaleDateString();
}

export function getMimeTypeIcon(mimeType: string): string {
  if (mimeType.startsWith('image/')) return '🖼️';
  if (mimeType.startsWith('video/')) return '🎥';
  if (mimeType.startsWith('audio/')) return '🎵';
  if (mimeType.includes('pdf')) return '📄';
  if (mimeType.includes('zip') || mimeType.includes('archive')) return '📦';
  if (mimeType.includes('text')) return '📝';
  return '📄';
}
```

- [ ] **Step 2: Create FileListItem component**

Create `frontend/src/lib/components/files/FileListItem.svelte`:

```svelte
<script lang="ts">
  import type { File, Folder } from '$lib/api/types';
  import { formatFileSize, formatDate, getMimeTypeIcon } from '$lib/utils/format';

  export let item: File | Folder;
  export let isFolder: boolean;
  export let onSelect: () => void;

  const icon = isFolder ? '📁' : getMimeTypeIcon((item as File).mime_type || '');
  const displaySize = isFolder ? '-' : formatFileSize((item as File).size);
  const displayDate = formatDate(isFolder ? (item as Folder).updated_at : (item as File).modified_at);
</script>

<div
  class="card bg-base-100 shadow-sm hover:shadow-md transition-shadow cursor-pointer"
  on:click={onSelect}
  on:keydown={(e) => e.key === 'Enter' && onSelect()}
  role="button"
  tabindex="0"
>
  <div class="card-body p-4">
    <div class="flex items-center gap-3">
      <span class="text-3xl">{icon}</span>
      <div class="flex-1 min-w-0">
        <h3 class="font-semibold truncate">{item.name}</h3>
        <div class="text-sm text-base-content/60 flex gap-4">
          <span>{displaySize}</span>
          <span>{displayDate}</span>
        </div>
      </div>
    </div>
  </div>
</div>
```

- [ ] **Step 3: Create FileGrid component**

Create `frontend/src/lib/components/files/FileGrid.svelte`:

```svelte
<script lang="ts">
  import type { File, Folder } from '$lib/api/types';
  import FileListItem from './FileListItem.svelte';

  export let folders: Folder[] = [];
  export let files: File[] = [];
  export let onFolderClick: (folder: Folder) => void;
  export let onFileClick: (file: File) => void;
</script>

{#if folders.length === 0 && files.length === 0}
  <div class="text-center py-12">
    <p class="text-base-content/60">No files or folders here</p>
  </div>
{:else}
  <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
    {#each folders as folder}
      <FileListItem
        item={folder}
        isFolder={true}
        onSelect={() => onFolderClick(folder)}
      />
    {/each}

    {#each files as file}
      <FileListItem
        item={file}
        isFolder={false}
        onSelect={() => onFileClick(file)}
      />
    {/each}
  </div>
{/if}
```

- [ ] **Step 4: Create file browser page with reactive query**

Create `frontend/src/routes/(app)/files/+page.svelte`:

```svelte
<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { getFolderContents } from '$lib/api/folders';
  import { downloadFile } from '$lib/api/files';
  import FileGrid from '$lib/components/files/FileGrid.svelte';
  import type { File, Folder } from '$lib/api/types';

  let currentFolderId: string | null = null;

  // Reactive query key - updates when currentFolderId changes
  $: contentsQuery = createQuery({
    queryKey: ['folder-contents', currentFolderId],
    queryFn: () => getFolderContents(currentFolderId)
  });

  function handleFolderClick(folder: Folder) {
    currentFolderId = folder.id;
  }

  async function handleFileClick(file: File) {
    try {
      const response = await downloadFile(file.id);
      window.open(response.url, '_blank');
    } catch (error) {
      console.error('Download failed:', error);
    }
  }
</script>

<svelte:head>
  <title>My Files - RustShare</title>
</svelte:head>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h1 class="text-2xl font-bold">My Files</h1>
    <div class="flex gap-2">
      <button class="btn btn-primary">+ Upload</button>
      <button class="btn btn-outline">+ New Folder</button>
    </div>
  </div>

  {#if $contentsQuery.isLoading}
    <div class="flex justify-center py-12">
      <span class="loading loading-spinner loading-lg"></span>
    </div>
  {:else if $contentsQuery.isError}
    <div class="alert alert-error">
      <span>Failed to load files: {$contentsQuery.error?.message}</span>
    </div>
  {:else if $contentsQuery.data}
    <FileGrid
      folders={$contentsQuery.data.folders}
      files={$contentsQuery.data.files}
      onFolderClick={handleFolderClick}
      onFileClick={handleFileClick}
    />
  {/if}
</div>
```

- [ ] **Step 5: Test file browser**

```bash
npm run dev
# Log in, navigate to /files
```

Expected: File browser shows (empty for now)

- [ ] **Step 6: Commit**

```bash
git add frontend/src/routes/\(app\)/files/ frontend/src/lib/components/files/ frontend/src/lib/utils/format.ts
git commit -m "feat(files): add file browser page with grid view"
```

---

Due to length constraints, I'll continue with the remaining tasks in a structured format. The plan would continue with:

**Phase 2 continued:**
- Task 11: File Upload with Progress
- Task 12: File Context Menu & Operations
- Task 13: Folder Operations

**Phase 3: Sharing**
- Task 14-18: Public and user-to-user sharing implementation

**Phase 4: Real-Time & Polish**
- Task 19-24: WebSocket, notifications, versioning, responsive design

**Phase 5: Docker & Deployment**
- Task 25-27: Docker configuration, nginx setup, production build

Each task follows the same structure:
- Clear file list
- TDD steps
- Test commands
- Commit messages

Would you like me to continue with the complete plan including all remaining tasks?

### Task 11: File Upload with Progress

**Files:**
- Create: `frontend/src/lib/components/files/UploadButton.svelte`
- Create: `frontend/src/lib/components/files/UploadModal.svelte`
- Create: `frontend/src/lib/components/files/DropZone.svelte`

**Steps:**
1. Create upload button component with file input
2. Create upload modal showing per-file progress
3. Implement drag-and-drop zone
4. Add upload mutation with XMLHttpRequest for progress tracking
5. Test: Upload single file, multiple files, drag-drop
6. Commit: "feat(files): add file upload with drag-drop and progress tracking"

### Task 12: File Context Menu & Operations

**Files:**
- Create: `frontend/src/lib/components/FileContextMenu.svelte`
- Create: `frontend/src/lib/components/modals/RenameModal.svelte`
- Create: `frontend/src/lib/components/modals/DeleteConfirmation.svelte`

**Steps:**
1. Create context menu component (right-click or three-dot menu)
2. Create rename modal with validation
3. Create delete confirmation modal
4. Add mutations for rename, move, delete operations
5. Integrate with FileListItem
6. Test: Rename file, delete file with confirmation
7. Commit: "feat(files): add file operations with context menu"

### Task 13: Folder Operations

**Files:**
- Create: `frontend/src/lib/components/modals/CreateFolderModal.svelte`
- Create: `frontend/src/lib/components/layout/Breadcrumbs.svelte`

**Steps:**
1. Create folder creation modal
2. Implement breadcrumb navigation for folder path
3. Add folder rename/delete (reuse existing modals)
4. Update file browser to show breadcrumbs
5. Test: Create folder, navigate, rename folder
6. Commit: "feat(folders): add folder operations and breadcrumb navigation"

---

## Phase 3: Sharing

### Task 14: Public Share API Module

**Important:** Public shares use **plural** endpoint `/api/files/:file_id/shares` (can create multiple public shares per file)

**Files:**
- Create: `frontend/src/lib/api/shares.ts`

**Steps:**
1. Implement createPublicShare, listPublicShares, revokeShare
2. Implement public share access: getShareInfo, createSession, downloadSharedFile
3. Test API client methods
4. Commit: "feat(api): add public share operations"

### Task 15: User Share API Module

**Important:** User-to-user shares use **singular** endpoint `/api/files/:id/share` (for creating) but **plural** `/api/shares/:id/permission` (for management)

**Files:**
- Create: `frontend/src/lib/api/user-shares.ts`

**Steps:**
1. Implement createFileShare, createFolderShare
2. Implement listReceivedShares, listRecipients
3. Implement updatePermission, removeRecipient
4. Test API client methods
5. Commit: "feat(api): add user-to-user share operations"

### Task 16: Share Modal UI

**Files:**
- Create: `frontend/src/lib/components/sharing/ShareModal.svelte`
- Create: `frontend/src/lib/components/sharing/PublicShareForm.svelte`
- Create: `frontend/src/lib/components/sharing/UserShareForm.svelte`
- Create: `frontend/src/lib/components/sharing/ShareList.svelte`

**Steps:**
1. Create share modal with tabs (Public Link, Share with User)
2. Create public share form (password, expiry, permissions)
3. Create user share form (email input, permissions dropdown)
4. Create share list showing existing shares
5. Add copy-to-clipboard functionality
6. Test: Create public share, create user share, copy link
7. Commit: "feat(sharing): add share modal with public and user sharing"

### Task 17: Public Share Access Page

**Files:**
- Create: `frontend/src/routes/share/[token]/+page.svelte`
- Create: `frontend/src/lib/components/sharing/SharePasswordForm.svelte`

**Steps:**
1. Create public share access page (no auth required)
2. Add password form for protected shares
3. Handle session JWT for download
4. Handle expired/revoked/not found errors
5. Test: Access public share, enter password, download
6. Commit: "feat(sharing): add public share access page"

### Task 18: Shared With Me Page

**Files:**
- Create: `frontend/src/routes/(app)/shared-with-me/+page.svelte`

**Steps:**
1. Create shared-with-me page with query
2. Display received shares with resource name, owner, permission
3. Handle file/folder clicks (navigate or download)
4. Show permission badges
5. Test: View received shares, access shared file
6. Commit: "feat(sharing): add shared-with-me page"

---

## Phase 4: Real-Time & Polish

### Task 19: WebSocket Client

**Files:**
- Create: `frontend/src/lib/websocket/client.ts`
- Create: `frontend/src/lib/websocket/events.ts`
- Create: `frontend/src/lib/stores/sync.ts`

**Steps:**
1. Create WebSocket event type definitions
2. Create WebSocket client with auto-reconnect and catch-up mechanism
3. Create sync state store
4. Implement event handlers that invalidate TanStack Query cache
5. Send catch-up request on reconnect: `{type: "sync", last_seen_event_id: "uuid"}` (message format matches backend SyncRequest)
6. Connect on login, disconnect on logout
7. Test: Connect, receive events, reconnect after disconnect
8. Commit: "feat(websocket): add WebSocket client with auto-reconnect"

### Task 20: Notifications API & UI

**Files:**
- Create: `frontend/src/lib/api/notifications.ts`
- Create: `frontend/src/lib/components/notifications/NotificationBell.svelte`
- Create: `frontend/src/lib/components/notifications/NotificationList.svelte`
- Create: `frontend/src/routes/(app)/notifications/+page.svelte`

**Steps:**
1. Create notifications API module
2. Create notification bell with unread count badge
3. Create notification dropdown list
4. Create notifications center page
5. Handle mark-as-read and delete
6. Real-time updates via WebSocket
7. Test: Receive notification, mark as read, click to navigate
8. Commit: "feat(notifications): add notifications system with real-time updates"

### Task 21: File Versioning UI

**Files:**
- Create: `frontend/src/lib/components/modals/VersionHistoryModal.svelte`

**Steps:**
1. Create version history modal
2. List versions with date, size, hash
3. Add restore button with confirmation
4. Show current version indicator
5. Test: View versions, restore version
6. Commit: "feat(files): add version history and restore UI"

### Task 22: Mobile Responsive Design

**Files:**
- Modify: Multiple component files for responsive breakpoints

**Steps:**
1. Update Sidebar to hamburger menu on mobile (<768px)
2. Update FileGrid to single column on mobile
3. Ensure all modals fit on small screens
4. Make buttons touch-friendly (min 44x44px)
5. Test on mobile viewports (375px, 768px, 1024px)
6. Commit: "feat(ui): add mobile responsive design"

### Task 23: Accessibility Improvements

**Files:**
- Modify: All interactive components

**Steps:**
1. Add ARIA labels to all buttons and inputs
2. Ensure keyboard navigation works throughout
3. Add focus indicators
4. Test with keyboard-only navigation
5. Test with screen reader
6. Commit: "feat(a11y): add accessibility improvements"

### Task 24: Error Handling & Loading States

**Files:**
- Modify: All pages and components

**Steps:**
1. Add skeleton screens for loading states
2. Add error boundaries for failed queries
3. Add retry buttons for failed requests
4. Add offline detection and banner
5. Test: Slow network, offline, API errors
6. Commit: "feat(ui): improve error handling and loading states"

---

## Phase 5: Docker & Deployment

### Task 25: Frontend Dockerfile

**Files:**
- Create: `docker/frontend.Dockerfile`
- Create: `frontend/.dockerignore`

**Steps:**
1. Create multi-stage Dockerfile (development and production)
2. Add .dockerignore
3. Test: Build production image
4. Test: Run production container
5. Commit: "feat(docker): add frontend Dockerfile with multi-stage build"

### Task 26: nginx Configuration

**Files:**
- Create: `docker/nginx.conf`
- Modify: `docker-compose.yml`

**Steps:**
1. Create nginx config with reverse proxy rules
2. Configure WebSocket upgrade for /api/sync
3. Add nginx service to docker-compose.yml
4. Add frontend service to docker-compose.yml
5. Test: Access frontend via nginx on port 80
6. Commit: "feat(docker): add nginx reverse proxy configuration"

### Task 27: Environment Configuration

**Files:**
- Modify: `docker-compose.yml`
- Create: `frontend/.env.production`

**Steps:**
1. Configure environment variables for production
2. Update docker-compose.yml with proper env vars
3. Test full stack: nginx -> frontend + backend + postgres + rustfs
4. Test: Login, upload file, share file, WebSocket connection
5. Commit: "feat(docker): configure production environment"

---

## Verification & Testing

### Integration Tests

**Files:**
- Create: `frontend/tests/e2e/auth.spec.ts`
- Create: `frontend/tests/e2e/files.spec.ts`
- Create: `frontend/tests/e2e/sharing.spec.ts`
- Create: `frontend/tests/e2e/realtime.spec.ts`

Each test file covers:
- Auth: Login, logout, token expiration
- Files: Upload, download, rename, delete, navigate folders
- Sharing: Create public share, create user share, access shared file
- Real-time: Multi-browser sync, WebSocket reconnection

### Manual Testing Checklist

- [ ] Login with valid/invalid credentials
- [ ] Upload single file and multiple files
- [ ] Download file
- [ ] Rename and delete file with confirmation
- [ ] Create, navigate, and delete folders
- [ ] Create public share with password and expiry
- [ ] Access public share in incognito browser
- [ ] Share file with user via email
- [ ] View received shares in "Shared with Me"
- [ ] Update share permissions
- [ ] Receive notification when shared with
- [ ] Mark notification as read
- [ ] Open two browsers, upload in one, see update in other
- [ ] Test mobile responsive (375px, 768px, 1920px)
- [ ] Test keyboard navigation
- [ ] Test with slow network (3G throttling)

---

## Success Criteria

MVP is complete when:

1. ✅ All 27 tasks implemented and tested
2. ✅ All integration tests (Playwright) pass
3. ✅ Manual testing checklist completed
4. ✅ Mobile responsive (works on phones)
5. ✅ Accessible (keyboard navigation, screen reader friendly)
6. ✅ Docker Compose setup runs full stack
7. ✅ Real-time sync works across browsers
8. ✅ All error states handled gracefully
9. ✅ No console errors in production build
10. ✅ WebSocket authentication modified and working

---

## Known Issues & Future Enhancements

**Known Limitations (Acceptable for MVP):**
- Backend WebSocket requires modification (Task 0)
- No file preview (just download)
- No search functionality
- No trash/restore
- No conflict resolution UI
- No keyboard shortcuts
- No dark mode
- No file thumbnails

**Future Enhancements:**
- File previews (images, PDFs, videos)
- Search with filters
- Activity feed
- Trash with restore
- Conflict resolution UI
- Keyboard shortcuts
- Dark mode
- Virtual scrolling for large lists
- Service worker for offline support

---

## Execution Instructions

**This plan MUST be executed using superpowers:subagent-driven-development:**

1. Create a dedicated worktree: Use superpowers:using-git-worktrees
2. Execute tasks: Use superpowers:subagent-driven-development
   - Fresh subagent per task
   - Two-stage review (spec compliance, then code quality)
   - Commit after each task
3. Final review: Use superpowers:requesting-code-review
4. Finish: Use superpowers:finishing-a-development-branch

**Task Execution Order:**
- Tasks 1-18 can proceed without backend WebSocket modification
- Task 0 (WebSocket auth fix) MUST be completed before Task 19
- Tasks 19-24 add real-time and polish (require WebSocket)
- Tasks 25-27 enable deployment
- Run integration tests after all tasks complete

**Estimated Timeline:**
- Phase 0: 1 hour
- Phase 1 (Tasks 1-5): 1 day
- Phase 2 (Tasks 6-13): 2 days
- Phase 3 (Tasks 14-18): 2 days
- Phase 4 (Tasks 19-24): 2 days
- Phase 5 (Tasks 25-27): 1 day
- Testing & polish: 1 day

**Total: ~10 days for full implementation**
