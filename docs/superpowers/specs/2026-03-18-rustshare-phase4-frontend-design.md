# RustShare Phase 4: Frontend Web Application Design

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a complete SvelteKit web application for RustShare that enables users to manage files, share with others (both publicly and user-to-user), and receive real-time updates via WebSocket.

**Architecture:** Single integrated SvelteKit application with Docker Compose deployment, using TailwindCSS + DaisyUI for UI, TanStack Query for server state management, and native WebSocket for real-time sync.

**Tech Stack:**
- Framework: SvelteKit with TypeScript
- Styling: TailwindCSS + DaisyUI
- State Management: Svelte stores + TanStack Query
- Real-Time: Native WebSocket API
- Testing: Playwright
- Deployment: Docker Compose with nginx reverse proxy

---

## Design Decisions

### Single Integrated Application (vs. Separate Apps)

**Decision:** Build one SvelteKit app with both authenticated and public routes.

**Rationale:**
- Simpler deployment (one frontend service)
- Consistent design language across all pages
- Shared components reduce code duplication
- Modern bundlers handle code-splitting efficiently
- Easier to maintain and evolve

**Trade-offs:**
- Slightly larger bundle for public share pages (acceptable)
- All routes share same deployment lifecycle

### Docker Compose Deployment

**Decision:** Use Docker Compose with nginx reverse proxy for local development and production.

**Architecture:**
```
nginx:80 (reverse proxy)
├── / → frontend:3000 (SvelteKit)
├── /api → backend:8080 (Axum)
└── /api/sync → backend:8080 (WebSocket)

Dependencies:
- postgres:5432 (database)
- minio:9000 (object storage)
```

**Benefits:**
- Single domain eliminates CORS complexity
- nginx handles WebSocket upgrades
- Easy to add SSL/TLS in production
- Consistent environment across dev/prod

---

## Architecture Overview

### Module Structure

```
frontend/
├── src/
│   ├── lib/
│   │   ├── api/              # Backend API clients
│   │   │   ├── client.ts     # Base HTTP client
│   │   │   ├── types.ts      # TypeScript interfaces
│   │   │   ├── auth.ts       # Authentication
│   │   │   ├── files.ts      # File operations
│   │   │   ├── folders.ts    # Folder operations
│   │   │   ├── shares.ts     # Public shares
│   │   │   ├── user-shares.ts # User-to-user shares
│   │   │   └── notifications.ts # Notifications
│   │   ├── stores/           # Svelte stores
│   │   │   ├── auth.ts       # Auth state
│   │   │   └── sync.ts       # WebSocket state
│   │   ├── websocket/        # Real-time layer
│   │   │   ├── client.ts     # WebSocket manager
│   │   │   └── events.ts     # Event types
│   │   ├── components/       # Reusable UI components
│   │   └── utils/            # Helper functions
│   └── routes/
│       ├── login/+page.svelte
│       ├── (app)/            # Protected routes
│       │   ├── files/+page.svelte
│       │   ├── shared-with-me/+page.svelte
│       │   └── notifications/+page.svelte
│       └── share/[token]/+page.svelte
├── static/                   # Static assets
├── docker/
│   ├── frontend.Dockerfile   # Production build
│   └── nginx.conf            # Reverse proxy config
├── package.json
├── svelte.config.js
├── tailwind.config.js
└── tsconfig.json
```

### Routing Structure

**Public Routes (No Auth Required):**
- `/login` - Login page
- `/share/[token]` - Public share access

**Protected Routes (Auth Required):**
- `/(app)/files` - File browser (default after login)
- `/(app)/shared-with-me` - Files/folders shared with current user
- `/(app)/notifications` - Notification center

**Route Guards:**
- Protected routes use SvelteKit layout with auth check
- Redirect to `/login` if not authenticated
- Auth check validates JWT token in localStorage

---

---

## Backend API Reference

### Authentication Endpoints

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| POST | `/api/auth/login` | User login | No |

**Request:**
```json
{ "email": "user@example.com", "password": "password123" }
```

**Response:**
```json
{
  "token": "eyJhbG...",
  "user": {
    "id": "uuid",
    "email": "user@example.com",
    "display_name": "User Name",
    "is_admin": false
  }
}
```

**JWT Token Details:**
- Token expiration: 24 hours (configurable via `JWT_EXPIRY_HOURS` env var)
- Token contains claims: `sub` (user ID), `email`, `display_name`, `is_admin`
- No refresh token in MVP - user must re-login after expiration
- Frontend should decode JWT to extract user info for auth store

### File Endpoints

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| POST | `/api/files/upload?folder_id={id}` | Upload file | Yes |
| GET | `/api/files/:id` | Get file metadata | Yes |
| PUT | `/api/files/:id` | Update file (with If-Match header) | Yes |
| DELETE | `/api/files/:id` | Delete file | Yes |
| GET | `/api/files/:id/download` | Get download URL | Yes |
| GET | `/api/files/:id/versions` | List file versions | Yes |
| POST | `/api/files/:id/restore` | Restore file version | Yes |
| POST | `/api/files/:id/move` | Move file to folder | Yes |
| POST | `/api/files/:id/rename` | Rename file | Yes |

### Folder Endpoints

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| POST | `/api/folders` | Create folder | Yes |
| GET | `/api/folders/:id` | Get folder metadata | Yes |
| DELETE | `/api/folders/:id` | Delete folder | Yes |
| GET | `/api/folders/:id/contents` | Get folder contents | Yes |
| GET | `/api/folders/tree` | Get entire folder tree | Yes |
| POST | `/api/folders/:id/move` | Move folder | Yes |
| POST | `/api/folders/:id/rename` | Rename folder | Yes |

### Public Share Endpoints

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| POST | `/api/files/:file_id/shares` | Create public share | Yes |
| GET | `/api/files/:file_id/shares` | List public shares for file | Yes |
| GET | `/api/public/share/:token/info` | Get public share info | No |
| POST | `/api/public/share/:token/session` | Create session (with password) | No |
| GET | `/api/public/share/:token/file` | Download file (requires session JWT) | Session JWT |

**Create Public Share Request:**
```json
{
  "permissions": "View",
  "password": "optional-password",
  "expires_at": "2026-04-18T12:00:00Z"
}
```

**Create Public Share Response:**
```json
{
  "id": "uuid",
  "file_id": "uuid",
  "share_token": "abc123xyz",
  "permissions": "View",
  "password_protected": true,
  "expires_at": "2026-04-18T12:00:00Z",
  "created_at": "2026-03-18T12:00:00Z"
}
```

### User-to-User Share Endpoints

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| POST | `/api/files/:id/share` | Share file with user | Yes |
| POST | `/api/folders/:id/share` | Share folder with user | Yes |
| GET | `/api/shares/received` | List received shares | Yes |
| GET | `/api/files/:id/recipients` | List file recipients | Yes |
| GET | `/api/folders/:id/recipients` | List folder recipients | Yes |
| PUT | `/api/shares/:id/permission` | Update recipient permission | Yes |
| DELETE | `/api/shares/:id/recipient` | Remove recipient | Yes |

**Share File with User Request:**
```json
{
  "recipient_email": "colleague@example.com",
  "permission": "Edit"
}
```

**Share Response:**
```json
{
  "share_id": "uuid",
  "resource_id": "uuid",
  "resource_type": "File",
  "recipient_email": "colleague@example.com",
  "permission": "Edit",
  "created_at": "2026-03-18T12:00:00Z"
}
```

**List Received Shares Response:**
```json
[
  {
    "share_id": "uuid",
    "resource_type": "File",
    "resource_id": "uuid",
    "resource_name": "document.pdf",
    "owner_email": "owner@example.com",
    "permission": "Edit",
    "created_at": "2026-03-18T12:00:00Z"
  }
]
```

**Permission Resolution:**
- The `permission` field in received shares indicates the current user's permission level
- Values: `"View"`, `"Edit"`, `"Admin"`
- Frontend uses this to show/hide UI elements (e.g., "Manage Recipients" button requires Admin)
- Folder permissions are inherited by child files/folders automatically by backend

### Notification Endpoints

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| GET | `/api/notifications?limit=50&offset=0&unread_only=false` | List notifications | Yes |
| PUT | `/api/notifications/:id/read` | Mark as read | Yes |
| DELETE | `/api/notifications/:id` | Delete notification | Yes |

**List Notifications Query Parameters:**
- `limit` (default: 50) - Maximum notifications to return
- `offset` (default: 0) - Skip N notifications for pagination
- `unread_only` (default: false) - Filter to unread only

**Notification Response:**
```json
{
  "notifications": [
    {
      "id": "uuid",
      "notification_type": "share_received",
      "title": "New Share",
      "message": "admin@localhost shared 'document.pdf' with you",
      "resource_id": "uuid",
      "resource_type": "file",
      "action_url": "/files/uuid",
      "read": false,
      "created_at": "2026-03-18T12:00:00Z"
    }
  ],
  "total": 1
}
```

### WebSocket Endpoint

**Backend Modification Required:** Current backend uses `Authorization` header for WebSocket auth, but browser WebSocket API doesn't support custom headers. Backend needs to be modified to accept token as query parameter.

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| GET | `/api/sync?token={jwt}` | WebSocket sync stream | JWT in query param |

**Event Types:**
```typescript
type EventType =
  // File events
  | 'FileUploaded' | 'FileModified' | 'FileRenamed' | 'FileMoved' | 'FileDeleted' | 'FileRestored'
  // Folder events
  | 'FolderCreated' | 'FolderRenamed' | 'FolderMoved' | 'FolderDeleted'
  // Share events (public)
  | 'ShareCreated' | 'ShareRevoked' | 'ShareUpdated'
  // Share events (user-to-user)
  | 'ShareReceivedByUser' | 'SharePermissionChanged' | 'ShareRevokedFromUser'
  // Notification events
  | 'NotificationCreated'
  // Conflict events
  | 'ConflictDetected' | 'ConflictResolved';
```

**Event Message Format:**
```json
{
  "type": "FileUploaded",
  "event_id": "uuid",
  "aggregate_id": "uuid",
  "aggregate_type": "file",
  "timestamp": "2026-03-18T12:00:00Z",
  "version": 1
}
```

---

## Component Architecture (Detailed)

### 1. API Layer (`src/lib/api/`)

**Base HTTP Client (`client.ts`):**
```typescript
export class ApiClient {
  constructor(private baseURL: string) {}

  async request<T>(endpoint: string, options?: RequestInit): Promise<T> {
    const token = localStorage.getItem('token');
    const headers = {
      'Authorization': token ? `Bearer ${token}` : '',
      'Content-Type': 'application/json',
      ...options?.headers
    };

    const response = await fetch(`${this.baseURL}${endpoint}`, {
      ...options,
      headers
    });

    if (!response.ok) {
      if (response.status === 401) {
        // Unauthorized - clear token and redirect
        localStorage.removeItem('token');
        window.location.href = '/login';
        throw new ApiError(401, 'Unauthorized');
      }
      const error = await response.json();
      throw new ApiError(response.status, error.message || 'Request failed');
    }

    return response.json();
  }
}
```

**Type Definitions (`types.ts`):**
- Mirror backend domain models (User, File, Folder, Share, Notification)
- Use exact same field names and types as backend
- All IDs are UUIDs (strings)
- Dates are ISO 8601 strings
- Enums match backend: `SharePermissions: 'View' | 'Edit' | 'Admin'`

**API Modules:**
- `auth.ts` - Login, logout, token management
- `files.ts` - Upload, download, rename, delete, versions
- `folders.ts` - Create, rename, delete, tree, contents
- `shares.ts` - Create public share, list, revoke (uses `/api/files/:file_id/shares` - plural)
- `user-shares.ts` - Share with user, list received, update permissions (uses `/api/files/:id/share` - singular)
- `notifications.ts` - List, mark as read, delete

### 2. State Management

**Auth Store (`src/lib/stores/auth.ts`):**
```typescript
interface AuthState {
  user: User | null;
  token: string | null;
  isAuthenticated: boolean;
}

export const authStore = writable<AuthState>({
  user: null,
  token: null,
  isAuthenticated: false
});

export function login(token: string) {
  const user = decodeJWT(token); // Extract user from JWT
  localStorage.setItem('token', token);
  authStore.set({ user, token, isAuthenticated: true });
}

export function logout() {
  localStorage.removeItem('token');
  authStore.set({ user: null, token: null, isAuthenticated: false });
}
```

**WebSocket Store (`src/lib/stores/sync.ts`):**
```typescript
interface SyncState {
  connected: boolean;
  reconnecting: boolean;
  lastEventId: string | null;
}

export const syncStore = writable<SyncState>({
  connected: false,
  reconnecting: false,
  lastEventId: null
});
```

**TanStack Query:**
- Handles all server state (files, folders, shares, notifications)
- Cache invalidation on mutations
- Optimistic updates for instant feedback
- Automatic background refetching
- Query keys: `['files', folderId]`, `['shares', fileId]`, etc.

### 3. WebSocket Layer

**Connection Manager (`src/lib/websocket/client.ts`):**

**WebSocket Authentication - Backend Modification Required:**

The current backend implementation expects JWT in an `Authorization: Bearer {token}` header during WebSocket upgrade (see `backend/server/src/handlers/sync.rs` line 126). However, the browser's native WebSocket API doesn't support custom headers.

**Solutions (choose one before implementation):**

**Option A: Modify Backend to Accept Query Parameter** (Recommended for browser compatibility)
- Change backend to extract token from URL: `/api/sync?token={jwt}`
- Validate token during upgrade handshake
- Most compatible with browser WebSocket API

**Option B: Modify Backend to Accept Post-Connection Auth**
- Accept unauthenticated WebSocket connection
- Client sends `{type: 'Auth', token: '{jwt}'}` as first message
- Backend validates and responds with `{type: 'AuthSuccess'}` or `{type: 'AuthError'}`
- More complex but allows fallback authentication

**For this spec, we'll document Option A** (query parameter), which requires backend modification:

```typescript
export class WebSocketClient {
  private ws: WebSocket | null = null;
  private reconnectAttempts = 0;
  private maxReconnectDelay = 30000; // 30 seconds

  connect(token: string) {
    // Backend modification required: accept token as query parameter
    const wsUrl = `${VITE_WS_URL}/sync?token=${encodeURIComponent(token)}`;
    this.ws = new WebSocket(wsUrl);

    this.ws.onopen = () => {
      syncStore.update(s => ({ ...s, connected: true, reconnecting: false }));
      this.reconnectAttempts = 0;
    };

    this.ws.onmessage = (event) => {
      const syncEvent = JSON.parse(event.data);
      this.handleEvent(syncEvent);
    };

    this.ws.onclose = () => {
      syncStore.update(s => ({ ...s, connected: false }));
      this.reconnect(token);
    };
  }

  private reconnect(token: string) {
    const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), this.maxReconnectDelay);
    this.reconnectAttempts++;

    syncStore.update(s => ({ ...s, reconnecting: true }));
    setTimeout(() => this.connect(token), delay);
  }

  private handleEvent(event: SyncEvent) {
    // Update last seen event ID
    syncStore.update(s => ({ ...s, lastEventId: event.id }));

    // Invalidate TanStack Query cache based on event type
    switch (event.type) {
      case 'FileUploaded':
      case 'FileDeleted':
        queryClient.invalidateQueries(['files', event.folder_id]);
        break;
      case 'FileRenamed':
      case 'FileMoved':
        queryClient.invalidateQueries(['files']);
        break;
      case 'ShareCreated':
      case 'ShareReceivedByUser':
        queryClient.invalidateQueries(['shares', event.file_id]);
        queryClient.invalidateQueries(['received-shares']);
        break;
      case 'SharePermissionChanged':
        queryClient.invalidateQueries(['shares', event.share_id]);
        queryClient.invalidateQueries(['received-shares']);
        break;
      case 'ShareRevoked':
      case 'ShareRevokedFromUser':
        queryClient.invalidateQueries(['shares']);
        queryClient.invalidateQueries(['received-shares']);
        break;
      case 'NotificationCreated':
        queryClient.invalidateQueries(['notifications']);
        break;
      case 'ConflictDetected':
      case 'ConflictResolved':
        // Show conflict modal or notification
        queryClient.invalidateQueries(['files', event.file_id]);
        break;
      case 'FolderCreated':
      case 'FolderRenamed':
      case 'FolderMoved':
      case 'FolderDeleted':
        queryClient.invalidateQueries(['folders']);
        break;
      default:
        // Forward compatibility: log unknown events but don't crash
        console.warn('Unknown event type:', event.type);
    }

    // Show toast notification for events from other users
    if (event.user_id !== currentUserId) {
      showToast(formatEventMessage(event));
    }
  }
}
```

### 4. UI Components

**Navigation Components:**
- `Sidebar.svelte` - Navigation menu (My Files, Shared with Me, Notifications, Settings, Logout)
- `Header.svelte` - Top bar with breadcrumbs, user menu, notification bell, connection status
- `Breadcrumbs.svelte` - Folder path navigation

**File Management Components:**
- `FileGrid.svelte` - Grid view of files/folders with loading states
- `FileListItem.svelte` - Individual file card (thumbnail, name, size, actions)
- `FolderTree.svelte` - Sidebar folder hierarchy navigation
- `UploadButton.svelte` - Upload trigger button
- `UploadModal.svelte` - Upload progress modal with per-file progress bars
- `DropZone.svelte` - Drag-and-drop overlay

**File Operation Modals:**
- `FileContextMenu.svelte` - Right-click or three-dot menu (Download, Rename, Share, Delete, Versions)
- `RenameModal.svelte` - Rename file/folder dialog
- `DeleteConfirmation.svelte` - Confirm delete with destructive action warning
- `VersionHistoryModal.svelte` - List versions with restore option

**Sharing Components:**
- `ShareModal.svelte` - Create/manage shares (tabs: Public Link, Share with User)
- `PublicShareForm.svelte` - Password, expiry, permissions for public share
- `UserShareForm.svelte` - Email input, permissions dropdown
- `ShareList.svelte` - List of shares (public + user shares)
- `ShareListItem.svelte` - Individual share with revoke/update actions
- `SharePasswordForm.svelte` - Password entry for accessing protected share

**Notification Components:**
- `NotificationBell.svelte` - Header icon with unread count badge
- `NotificationList.svelte` - Dropdown list of notifications
- `NotificationItem.svelte` - Individual notification with mark-as-read, click to navigate

**Utility Components:**
- `Toast.svelte` - Toast notifications (success, error, info)
- `Modal.svelte` - Base modal with overlay and close handling
- `Spinner.svelte` - Loading spinner
- `SkeletonScreen.svelte` - Loading placeholders

---

## Data Flow Patterns

### Authentication Flow

1. User enters email + password on `/login`
2. Submit → `POST /api/auth/login`
3. Backend returns `{ token, user }`
4. Frontend stores token in localStorage
5. Update auth store with user info
6. Redirect to `/files`
7. WebSocket connects with JWT token
8. Protected route checks auth store → render if authenticated

### File Upload Flow

1. User selects files or drops them
2. For each file:
   - Create FormData with file
   - Start mutation with `POST /api/files/upload?folder_id={id}`
   - Track upload progress via XMLHttpRequest.upload.onprogress
   - Show progress bar in UI
3. On success:
   - Optimistically add file to grid (marked as uploading)
   - Mutation updates TanStack Query cache
   - WebSocket broadcasts FileUploaded event
   - Other clients receive event → invalidate cache → refetch → UI updates
4. On error:
   - Show error toast
   - Remove optimistic file from grid

### Share Creation Flow (Public)

1. User clicks Share button on file
2. Open ShareModal with two tabs: "Public Link" and "Share with User"
3. User fills form: password (optional), expiry (optional), permissions (View/Edit)
4. Submit → `POST /api/files/{id}/shares`
5. Backend returns share with token
6. Display share URL with copy button
7. Copy to clipboard → show "Copied!" feedback
8. WebSocket broadcasts ShareCreated event
9. Other clients see updated share count

### Share Creation Flow (User-to-User)

1. User switches to "Share with User" tab
2. Enter recipient email, select permissions (View/Edit/Admin)
3. Submit → `POST /api/files/{id}/share`
4. Backend:
   - Validates recipient exists
   - Creates share
   - Creates notification for recipient
5. WebSocket broadcasts ShareCreated and NotificationCreated events
6. Recipient's notification bell updates in real-time
7. Recipient clicks notification → navigates to Shared with Me page

### Real-Time Sync Flow

1. WebSocket client connects on login with JWT
2. Backend authenticates and streams events
3. Frontend receives event:
   - Parse event type and payload
   - Update sync store with last event ID
   - Invalidate relevant TanStack Query cache keys
   - Show toast notification if event from another user
4. TanStack Query refetches invalidated queries
5. UI re-renders with fresh data
6. On disconnect:
   - Show "Reconnecting..." banner
   - Attempt reconnect with exponential backoff
   - On reconnect: send last event ID for catch-up

---

## Error Handling

### API Errors

**401 Unauthorized:**
- Clear token from localStorage
- Update auth store (logged out)
- Redirect to `/login`
- Show toast: "Session expired. Please log in again."

**403 Forbidden:**
- Show toast: "You don't have permission to perform this action."
- Don't retry automatically

**404 Not Found:**
- Show toast: "Resource not found."
- Remove from UI if applicable

**409 Conflict (File Version Conflict):**
- Show ConflictModal with options:
  - Keep Mine (overwrite with If-Match: *)
  - Keep Theirs (cancel upload)
  - Keep Both (rename and upload)

**429 Rate Limited:**
- Show toast: "Too many requests. Please wait X seconds."
- Extract retry-after from response header
- Disable action button with countdown

**500+ Server Errors:**
- Show toast: "Something went wrong. Please try again."
- Offer retry button
- Log error details to console

### Network Errors

**Failed Fetch:**
- Show toast: "Network error. Check your connection."
- Retry button on toast
- TanStack Query handles automatic retries (3 attempts)

**WebSocket Disconnect:**
- Update sync store: `connected: false, reconnecting: true`
- Show banner: "Reconnecting to server..."
- Auto-reconnect with exponential backoff (1s, 2s, 4s, ..., max 30s)
- On reconnect: show toast "Connected" and hide banner

### Validation Errors

**Client-Side:**
- Validate with Zod schemas before submission
- Show inline errors on form fields
- Disable submit button while invalid

**Server-Side:**
- Backend returns validation errors
- Map errors to form fields
- Show inline error messages

---

## User Experience Features

### Loading States

**Initial Page Load:**
- Show skeleton screens for file grid
- Fade in actual content when loaded

**File Upload:**
- Per-file progress bars
- Cancel button per file
- Overall progress indicator
- Optimistic update: show file immediately with "Uploading..." badge

**Mutations:**
- Spinner on button during request
- Disable button to prevent double-submit
- Success checkmark on completion

**Navigation:**
- Top-bar progress indicator (NProgress-style)
- Instant route transition (no delay)

### Feedback

**Toast Notifications:**
- Success: Green, checkmark icon, 3s auto-dismiss
- Error: Red, X icon, 5s auto-dismiss (longer for errors)
- Info: Blue, info icon, 3s auto-dismiss
- Position: Bottom-right on desktop, top-center on mobile

**Optimistic Updates:**
- Files appear immediately on upload (marked "Uploading")
- Renames update instantly
- Deletes remove immediately
- Rollback if mutation fails

**Real-Time Notifications:**
- Show toast when other user uploads file to shared folder
- Notification bell badge updates in real-time
- Subtle animation on new notification

### Responsive Design

**Breakpoints:**
- Mobile: < 768px
- Tablet: 768px - 1024px
- Desktop: > 1024px

**Mobile Adaptations:**
- Hamburger menu for sidebar
- Single-column file grid
- Touch-friendly buttons (min 44x44px)
- Swipe gestures for context menu
- Bottom navigation for primary actions

**Desktop Enhancements:**
- Sidebar always visible
- 4-6 column file grid
- Hover states on cards
- Right-click context menu
- Keyboard shortcuts

### Accessibility

**Keyboard Navigation:**
- Tab through all interactive elements
- Enter/Space to activate buttons
- Escape to close modals
- Arrow keys to navigate file grid

**Screen Reader Support:**
- ARIA labels on all buttons and inputs
- Role attributes on custom components
- Live regions for dynamic content updates
- Focus management in modals (trap focus, restore on close)

**Visual:**
- Focus indicators on all interactive elements
- Sufficient color contrast (WCAG AA)
- No color-only information
- Text alternatives for icons

---

## Docker Compose Setup

### Services Configuration

**docker-compose.yml:**
```yaml
services:
  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
    volumes:
      - ./docker/nginx.conf:/etc/nginx/nginx.conf:ro
    depends_on:
      - frontend
      - backend

  frontend:
    build:
      context: ./frontend
      dockerfile: ../docker/frontend.Dockerfile
    environment:
      - VITE_API_URL=/api
      - VITE_WS_URL=ws://localhost/api
    ports:
      - "3000:3000"

  backend:
    # ... existing backend service

  postgres:
    # ... existing postgres service

  minio:
    # ... existing minio service
```

### nginx Reverse Proxy

**docker/nginx.conf:**
```nginx
events {
  worker_connections 1024;
}

http {
  upstream frontend {
    server frontend:3000;
  }

  upstream backend {
    server backend:8080;
  }

  server {
    listen 80;
    server_name localhost;

    # Frontend (SvelteKit)
    location / {
      proxy_pass http://frontend;
      proxy_set_header Host $host;
      proxy_set_header X-Real-IP $remote_addr;
      proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }

    # Backend API (HTTP)
    location /api {
      proxy_pass http://backend;
      proxy_set_header Host $host;
      proxy_set_header X-Real-IP $remote_addr;
      proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }

    # Backend WebSocket
    location /api/sync {
      proxy_pass http://backend;
      proxy_http_version 1.1;
      proxy_set_header Upgrade $http_upgrade;
      proxy_set_header Connection "upgrade";
      proxy_set_header Host $host;
      proxy_set_header X-Real-IP $remote_addr;
      proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
  }
}
```

### Frontend Dockerfile

**docker/frontend.Dockerfile:**
```dockerfile
# Development stage
FROM node:20-alpine AS development
WORKDIR /app
COPY frontend/package*.json ./
RUN npm install
COPY frontend/ ./
CMD ["npm", "run", "dev", "--", "--host", "0.0.0.0"]

# Build stage
FROM node:20-alpine AS build
WORKDIR /app
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# Production stage
FROM node:20-alpine AS production
WORKDIR /app
COPY --from=build /app/build ./build
COPY --from=build /app/package*.json ./
RUN npm ci --production
CMD ["node", "build"]
```

---

## Implementation Phases

### Phase 1: Foundation (Days 1-3)

**Tasks:**
1. Initialize SvelteKit project with TypeScript
2. Set up TailwindCSS + DaisyUI
3. Install dependencies (TanStack Query, Zod, date-fns)
4. Configure Docker Compose with nginx
5. Create API client library with TypeScript types
6. Implement authentication flow and route guards
7. Build app shell (Sidebar, Header, layouts)
8. Test: Login flow works, protected routes redirect

### Phase 2: File Management (Days 4-7)

**Tasks:**
9. Implement file browser with TanStack Query
10. Build folder tree navigation
11. Add file upload with drag-drop and progress
12. Implement file operations (download, rename, delete)
13. Add folder operations (create, rename, delete)
14. Build context menus and modals
15. Test: Full CRUD operations work
16. Test: Upload multiple files, navigate folders

### Phase 3: Sharing (Days 8-10)

**Tasks:**
17. Build ShareModal with two tabs (Public, User)
18. Implement public share creation with password/expiry
19. Build public share access page (`/share/[token]`)
20. Implement user-to-user sharing by email
21. Build "Shared with Me" page
22. Add permission management UI
23. Build share list with revoke/update actions
24. Test: Create both share types, access public shares
25. Test: Receive user share, verify permissions

### Phase 4: Real-Time & Polish (Days 11-14)

**Tasks:**
26. Implement WebSocket client with reconnection
27. Connect WebSocket events to TanStack Query invalidation
28. Build notifications center
29. Add notification bell with real-time updates
30. Implement file versioning UI with restore
31. Add toast notifications for all events
32. Polish responsive design and mobile layout
33. Accessibility audit and keyboard navigation
34. Add loading states and error handling
35. Production build optimization
36. Test: Real-time sync works across browsers
37. Test: Mobile responsive, accessibility

---

## Testing Strategy

### Unit Tests (Vitest)

**Test Coverage:**
- API client methods (mocked fetch)
- Auth store logic (login, logout)
- WebSocket event handlers (cache invalidation)
- Utility functions (formatFileSize, formatDate)
- Form validation (Zod schemas)

### Integration Tests (Playwright)

**Test Suites:**

1. **Authentication:**
   - Login with valid credentials → redirects to /files
   - Login with invalid credentials → shows error
   - Access protected route without auth → redirects to /login
   - Logout → clears token, redirects to /login

2. **File Operations:**
   - Upload single file → appears in grid
   - Upload multiple files → all appear with progress
   - Download file → triggers browser download
   - Rename file → updates in UI
   - Delete file → confirmation → removes from UI

3. **Folder Operations:**
   - Create folder → appears in grid and tree
   - Navigate into folder → breadcrumbs update
   - Folder tree reflects structure

4. **Public Sharing:**
   - Create public share → get link
   - Copy link → clipboard contains token
   - Access share (no password) → download works
   - Access password-protected share → enter password → download
   - Access expired share → shows error

5. **User Sharing:**
   - Share file with user → notification created
   - Recipient sees notification
   - Recipient navigates to Shared with Me → file appears
   - Update permissions → reflected in recipient's UI
   - Remove recipient → share disappears for them

6. **Real-Time Sync:**
   - Open two browser windows
   - Upload in window A → appears in window B
   - Delete in window B → removed from window A
   - WebSocket disconnect → shows reconnecting banner → reconnects

7. **Notifications:**
   - Receive notification → bell badge updates
   - Click notification → navigates to resource
   - Mark as read → badge count decrements

### Manual Testing

**Cross-Browser:**
- Chrome, Firefox, Safari, Edge
- Mobile browsers (iOS Safari, Android Chrome)

**Responsive:**
- 375px (mobile), 768px (tablet), 1920px (desktop), 2560px (wide)

**Network Conditions:**
- Fast 3G, Slow 3G, Offline
- Test upload/download on slow connections
- Test WebSocket reconnection

**Accessibility:**
- Screen reader (NVDA/VoiceOver)
- Keyboard-only navigation
- High contrast mode

---

## Success Criteria

### MVP Complete When:

1. ✅ Users can log in and see file browser
2. ✅ Users can upload files with progress tracking
3. ✅ Users can browse files in folder hierarchy
4. ✅ Users can download, rename, delete files
5. ✅ Users can create and manage folders
6. ✅ Users can create public share links with password + expiry
7. ✅ Anonymous users can access public shares
8. ✅ Users can share files/folders with other users via email
9. ✅ Users can see received shares in "Shared with Me"
10. ✅ Users can manage share permissions (View/Edit/Admin)
11. ✅ Users receive real-time notifications for share events
12. ✅ Real-time sync works (multi-device updates)
13. ✅ UI is mobile-responsive
14. ✅ All Playwright tests pass
15. ✅ Docker Compose setup works for local development

### Known Limitations (Acceptable for MVP)

- **WebSocket authentication requires backend modification** - Backend currently expects Authorization header, needs to support query parameter for browser compatibility
- No file preview (just download)
- No search functionality
- No activity feed
- No trash/restore
- Basic conflict detection (ConflictDetected/ConflictResolved events handled, but no UI for resolution)
- No keyboard shortcuts
- No dark mode
- No file thumbnails (just mime-type icons)
- No user registration endpoint (admin creates users)

---

## Future Enhancements (Post-MVP)

### Phase 5: Polish & Features

- File previews (images, PDFs, videos in-browser)
- Search (full-text, filter by type/date)
- Activity feed (recent actions)
- Trash with restore
- Conflict resolution UI with side-by-side comparison
- Keyboard shortcuts
- Dark mode
- User settings page
- Storage quota indicator

### Phase 6: Performance

- Virtual scrolling for large file lists
- Thumbnail generation and caching
- Service worker for offline support
- Progressive Web App (PWA) manifest

### Phase 7: Collaboration

- Collaborative editing (if applicable to file types)
- Comments on files
- @mentions in notifications
- Team/group sharing

---

## Dependencies

### Frontend Package.json

```json
{
  "name": "rustshare-frontend",
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview",
    "test": "playwright test",
    "test:unit": "vitest"
  },
  "dependencies": {
    "@sveltejs/kit": "^2.0.0",
    "@tanstack/svelte-query": "^5.0.0",
    "daisyui": "^4.0.0",
    "zod": "^3.22.0",
    "date-fns": "^3.0.0"
  },
  "devDependencies": {
    "@playwright/test": "^1.40.0",
    "@sveltejs/adapter-node": "^5.0.0",
    "@sveltejs/vite-plugin-svelte": "^3.0.0",
    "autoprefixer": "^10.4.16",
    "postcss": "^8.4.32",
    "svelte": "^5.0.0",
    "tailwindcss": "^3.4.0",
    "typescript": "^5.3.0",
    "vite": "^5.0.0",
    "vitest": "^1.0.0"
  }
}
```

---

## Summary

This design provides a complete, production-ready frontend for RustShare that:

1. **Matches backend capabilities** - Supports all implemented features (files, folders, public shares, user shares, notifications, real-time sync)
2. **Uses modern stack** - SvelteKit + TailwindCSS + TanStack Query for excellent DX and performance
3. **Handles real-time** - WebSocket integration with auto-reconnect and optimistic updates
4. **Production-ready** - Docker Compose setup with nginx reverse proxy
5. **Well-tested** - Comprehensive Playwright test suite
6. **Accessible** - Keyboard navigation, screen reader support, responsive design

The architecture is extensible for future enhancements while keeping the MVP focused on core functionality.
