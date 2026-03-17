# RustShare: Personal/Team File Sync & Share Platform
**Design Specification**
**Date:** 2026-03-17
**Status:** Draft - Pending Review

---

## Executive Summary

RustShare is a personal/team file synchronization and sharing platform built with Rust, designed as an open-source alternative to Nextcloud and Seafile. The system provides a web-first user experience with multi-protocol support (WebDAV, S3-compatible), real-time synchronization, and intelligent conflict detection.

**Key Features:**
- Web-based file management with mobile-responsive UI
- Real-time sync with conflict detection and user-guided resolution
- File versioning with configurable retention policies
- Protected share links (password, expiry)
- Preview and thumbnail generation for common file types
- Multi-protocol support (HTTP API, WebDAV, S3-compatible)
- Built on RustFS (S3-compatible object storage) for scalability

**Architecture Philosophy:** Start simple, design for growth
- Modular monolith architecture (can split into microservices later)
- Event-sourced design for auditability and real-time features
- Docker Compose deployment for ease of development

---

## 1. System Architecture

### 1.1 High-Level Overview

```
┌─────────────────────────────────────────────────────────────┐
│                         Clients                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ Web UI   │  │ WebDAV   │  │ S3 Tools │  │  Mobile  │   │
│  │(Browser) │  │ Clients  │  │ (rclone) │  │ (Future) │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
└───────┼─────────────┼─────────────┼─────────────┼──────────┘
        │             │             │             │
        └─────────────┴─────────────┴─────────────┘
                      │
        ┌─────────────▼──────────────────────────────────┐
        │        RustShare Server (Rust Monolith)        │
        │  ┌──────────────────────────────────────────┐  │
        │  │         Protocol Adapters                │  │
        │  │  HTTP API │ WebDAV │ S3-Compatible │ WS │  │
        │  └─────────────────┬────────────────────────┘  │
        │  ┌─────────────────▼────────────────────────┐  │
        │  │          Core Business Logic             │  │
        │  │   File Service │ Sync Engine │ Auth     │  │
        │  └─────────────────┬────────────────────────┘  │
        │  ┌─────────────────▼────────────────────────┐  │
        │  │           Event Store                    │  │
        │  │     (Append-only event log)              │  │
        │  └──────────────────────────────────────────┘  │
        └─────────────┬──────────────┬───────────────────┘
                      │              │
        ┌─────────────▼──────┐  ┌───▼──────────────────┐
        │    PostgreSQL      │  │   RustFS (S3 Storage)│
        │  Events + Metadata │  │   File Content Blobs │
        └────────────────────┘  └──────────────────────┘
```

### 1.2 Component Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| **Backend Server** | Rust (Axum framework) | HTTP/WebSocket server, business logic |
| **Database** | PostgreSQL 16 | Event store + materialized views (metadata) |
| **Object Storage** | RustFS | S3-compatible storage for file content |
| **Frontend** | SvelteKit or React + Vite | Web UI with real-time updates |
| **Deployment** | Docker Compose | Container orchestration for dev/prod |

### 1.3 Modular Monolith Structure

The Rust backend is organized as a Cargo workspace with clear module boundaries:

```
backend/
├── crates/
│   ├── core/              # Business logic (no I/O)
│   │   ├── domain/        # User, File, Folder, Share types
│   │   ├── events/        # Event definitions
│   │   └── services/      # FileService, ShareService, etc.
│   ├── storage/           # Data persistence
│   │   ├── event_store/   # Append-only event log
│   │   ├── metadata/      # Queries on materialized views
│   │   └── object_store/  # RustFS S3 client abstraction
│   ├── protocols/         # Protocol adapters
│   │   ├── http_api/      # REST/JSON API
│   │   ├── websocket/     # Real-time sync protocol
│   │   ├── webdav/        # WebDAV (RFC 4918)
│   │   └── s3_compat/     # S3-compatible API
│   ├── sync/              # Synchronization engine
│   │   ├── conflict_detection/
│   │   ├── version_manager/
│   │   └── change_notifier/
│   └── auth/              # Authentication & authorization
│       ├── session/       # JWT tokens, sessions
│       └── permissions/   # ACL evaluation
└── server/                # Main binary
    └── main.rs            # Wires everything together
```

**Key Design Principles:**
- Core business logic has no knowledge of HTTP, WebDAV, or protocols
- Protocol adapters translate requests → domain commands → events
- Events are the single source of truth
- Any module can subscribe to events to add functionality

---

## 2. Data Model

### 2.1 Core Domain Types

**User:**
```rust
struct User {
    id: UserId,              // UUID
    email: String,
    password_hash: String,   // Argon2id
    display_name: String,
    storage_quota: i64,      // bytes
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

**File:**
```rust
struct File {
    id: FileId,              // UUID
    name: String,
    path: String,            // logical path: "/Documents/report.pdf"
    size: i64,               // bytes
    mime_type: String,
    content_hash: String,    // SHA-256
    storage_key: String,     // RustFS object key
    owner_id: UserId,
    parent_folder_id: Option<FolderId>,
    current_version: i32,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
}
```

**FileVersion:**
```rust
struct FileVersion {
    id: VersionId,           // UUID
    file_id: FileId,
    version_number: i32,
    content_hash: String,
    storage_key: String,     // RustFS key for this version
    size: i64,
    created_by: UserId,
    created_at: DateTime<Utc>,
    change_description: Option<String>,
}
```

**Folder:**
```rust
struct Folder {
    id: FolderId,            // UUID
    name: String,
    path: String,            // logical path: "/Documents"
    owner_id: UserId,
    parent_folder_id: Option<FolderId>,  // null for root folders
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

**Share:**
```rust
struct Share {
    id: ShareId,             // UUID
    file_id: FileId,
    share_token: String,     // cryptographically random
    created_by: UserId,
    permissions: SharePermissions,  // Read | ReadWrite
    password_hash: Option<String>,  // bcrypt
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    access_count: i32,
}
```

### 2.2 Event Store Schema

Events are stored in an append-only table:

```sql
CREATE TABLE events (
    id BIGSERIAL PRIMARY KEY,
    event_id UUID UNIQUE NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    aggregate_id UUID NOT NULL,      -- file_id, user_id, etc.
    aggregate_type VARCHAR(50) NOT NULL,  -- "file", "user", "share"
    payload JSONB NOT NULL,
    user_id UUID NOT NULL,           -- who triggered the event
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    version INTEGER NOT NULL,        -- optimistic locking

    INDEX idx_aggregate (aggregate_id, aggregate_type),
    INDEX idx_timestamp (timestamp),
    INDEX idx_event_type (event_type)
);
```

**Event Types:**

**File Events:**
- `FileUploaded` - new file created
- `FileModified` - file content changed
- `FileRenamed` - filename changed
- `FileMoved` - moved to different folder
- `FileDeleted` - soft delete

**Folder Events:**
- `FolderCreated` - new folder created
- `FolderRenamed` - folder name changed
- `FolderMoved` - folder moved to different parent
- `FolderDeleted` - folder deleted (cascades to contents)

**Share Events:**
- `ShareCreated` - share link generated
- `ShareRevoked` - share link invalidated

**Sync Events:**
- `ConflictDetected` - sync conflict occurred
- `ConflictResolved` - user resolved conflict

**Example Event Payloads:**

**File Event:**
```json
{
  "event_type": "FileModified",
  "aggregate_id": "550e8400-e29b-41d4-a716-446655440000",
  "payload": {
    "file_id": "550e8400-e29b-41d4-a716-446655440000",
    "new_version": 5,
    "content_hash": "sha256:abc123def456...",
    "storage_key": "users/user123/files/file456/v5",
    "size": 2048576,
    "modified_by": "user-uuid-789"
  }
}
```

**Folder Event:**
```json
{
  "event_type": "FolderCreated",
  "aggregate_id": "650e8400-e29b-41d4-a716-446655440001",
  "payload": {
    "folder_id": "650e8400-e29b-41d4-a716-446655440001",
    "name": "Projects",
    "path": "/Documents/Projects",
    "parent_folder_id": "parent-folder-uuid",
    "owner_id": "user-uuid-123"
  }
}
```

### 2.3 Materialized Views

For query performance, maintain standard SQL tables updated by event handlers:

- `users` - current user state
- `files` - current file state
- `file_versions` - version history
- `shares` - active shares
- `folders` - folder hierarchy
- `user_storage_usage` - aggregate storage per user

These tables are updated synchronously after events are persisted, within the same database transaction.

---

## 3. Real-Time Synchronization

### 3.1 WebSocket Protocol

**Connection & Authentication:**
```
Client → Server: { "type": "auth", "token": "jwt-token-here" }
Server → Client: { "type": "auth_success", "user_id": "uuid" }
```

**Subscription:**
```
Client → Server: { "type": "subscribe", "path": "/Documents" }
Server → Client: { "type": "subscribed", "path": "/Documents", "version": 42 }
```

**Change Notification:**
```
Server → Client: {
  "type": "file_changed",
  "file_id": "uuid",
  "path": "/Documents/report.pdf",
  "change_type": "modified",
  "version": 43,
  "content_hash": "sha256:...",
  "modified_by": "user@example.com",
  "timestamp": "2026-03-17T10:30:00Z"
}
```

### 3.2 Conflict Detection Flow

**1. Upload Intent:**
```
Client → Server: {
  "type": "upload_intent",
  "file_id": "uuid",
  "last_known_version": 5,
  "content_hash": "sha256:..."
}
```

**2. Server Checks:**
- Query current version from database
- If `current_version == last_known_version`: **No conflict**, proceed with upload
- If `current_version > last_known_version`: **Conflict detected!**

**3. Conflict Response:**
```
Server → Client: {
  "type": "conflict_detected",
  "file_id": "uuid",
  "current_version": 7,
  "your_version": 5,
  "current_download_url": "https://...",
  "current_modified_by": "user@example.com",
  "current_modified_at": "2026-03-17T09:15:00Z"
}
```

**4. Client Shows Conflict UI:**
- Download current version from server
- Display both versions side-by-side
- Options: "Keep mine", "Keep theirs", "Keep both" (renames), "Manual merge" (future)

**5. Resolution:**
```
Client → Server: {
  "type": "resolve_conflict",
  "file_id": "uuid",
  "resolution": "keep_mine",
  "resolved_content_hash": "sha256:..."
}
```

**6. Server Processes:**
- Create new version with resolved content
- Emit `ConflictResolved` event
- Notify other connected clients

### 3.3 Version Tracking

- Every file modification increments version counter atomically
- Optimistic locking using database constraints prevents race conditions
- Event store maintains complete history
- Server time (UTC) is source of truth for conflict detection

---

## 4. Multi-Protocol Support

### 4.1 Protocol Adapter Architecture

All protocols follow the same translation flow:

```
Protocol Request → Authenticate → Authorize → Translate to Domain Command
→ Execute Core Service → Emit Event → Respond in Protocol Format
```

### 4.2 HTTP REST API

**Primary interface for web UI.**

**Key Endpoints:**

**File Operations:**
- `POST /api/files/upload` - Upload file with metadata
- `GET /api/files/{id}` - Get file metadata
- `GET /api/files/{id}/download` - Download file content (presigned URL)
- `PUT /api/files/{id}` - Update file (triggers conflict detection)
- `DELETE /api/files/{id}` - Soft delete
- `GET /api/files/tree?path=/` - Get folder structure
- `POST /api/files/{id}/move` - Move file to different folder
- `POST /api/files/{id}/rename` - Rename file

**Folder Operations:**
- `POST /api/folders` - Create new folder
- `GET /api/folders/{id}` - Get folder metadata
- `PUT /api/folders/{id}` - Update folder (rename)
- `DELETE /api/folders/{id}` - Delete folder (cascade to contents)
- `POST /api/folders/{id}/move` - Move folder to different parent
- `GET /api/folders/{id}/contents` - List folder contents

**Versioning:**
- `GET /api/files/{id}/versions` - List version history
- `GET /api/files/{id}/versions/{version}` - Get specific version metadata
- `POST /api/files/{id}/restore` - Restore previous version

**Sharing:**
- `POST /api/shares` - Create share link
- `GET /api/shares/{token}` - Get share details (public endpoint)
- `DELETE /api/shares/{id}` - Revoke share
- `PUT /api/shares/{id}` - Update share (password, expiry)

**User:**
- `POST /api/auth/login` - Authenticate
- `POST /api/auth/logout` - Invalidate token
- `GET /api/user/profile` - Get current user
- `GET /api/user/storage` - Storage usage

**WebSocket:**
- `WS /api/ws` - Real-time sync connection

### 4.3 WebDAV Protocol

**Standard WebDAV implementation (RFC 4918)** - allows mounting as network drive.

**Endpoint:** `https://rustshare.example.com/dav/{username}/`

**Supported Methods:**
- `PROPFIND` - List files/folders → queries metadata store
- `GET` - Download file → proxies to RustFS (or presigned URL)
- `PUT` - Upload/update file → conflict detection + event emission
- `DELETE` - Delete file → emits `FileDeleted` event
- `MKCOL` - Create folder
- `MOVE` - Rename/move file
- `COPY` - Copy file (future)
- `LOCK/UNLOCK` - File locking (optional for v1)

**Authentication:** HTTP Basic Auth or Bearer token (JWT)

**Compatible Clients:**
- macOS Finder (Connect to Server)
- Windows Explorer (Map Network Drive)
- Linux file managers (Nautilus, Dolphin, etc.)
- Mobile apps (WebDAV Navigator, etc.)

### 4.4 S3-Compatible API

**Subset of S3 API** for compatibility with existing tools (rclone, Cyberduck, Mountain Duck, etc.).

**Endpoint:** `https://rustshare.example.com/s3/`

**Supported Operations:**
- `ListBuckets` - List user's top-level folders as "buckets"
- `ListObjects` / `ListObjectsV2` - List files in folder/bucket
- `GetObject` - Download file
- `PutObject` - Upload file (goes through conflict detection)
- `DeleteObject` - Delete file
- `HeadObject` - Get file metadata
- `CreateMultipartUpload` - Large file uploads (future)

**Authentication:** AWS Signature V4 (compatible with S3 tools)
- Each user gets API credentials (access key ID + secret access key)
- Stored hashed in database

**Bucket Mapping:**
- Buckets map to top-level user folders
- Example: user's `/Documents` folder appears as `documents` bucket

### 4.5 Storage Abstraction

The `storage/object_store/` module abstracts RustFS access:

```rust
#[async_trait]
trait ObjectStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<String>;
    async fn get(&self, key: &str) -> Result<Bytes>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn generate_presigned_url(
        &self,
        key: &str,
        expiry: Duration
    ) -> Result<String>;
}
```

**Implementation uses Rust S3 client** (e.g., `aws-sdk-s3` or `rusoto_s3`) configured for RustFS endpoint.

**Optimization:** For large files, generate presigned URLs so clients upload/download directly to/from RustFS, reducing bandwidth through backend server.

---

## 5. File Versioning & Storage

### 5.1 Content-Addressed Storage

**On file upload/modification:**

1. Calculate SHA-256 hash of file content
2. Check if hash already exists in storage (deduplication)
3. If new: upload to RustFS with key `users/{user_id}/files/{file_id}/v{version}`
4. If duplicate: reuse existing storage key, create new version record
5. Increment version counter in database (atomic operation)
6. Create `FileVersion` entry linking to storage key
7. Emit `FileModified` event

### 5.2 Storage Layout in RustFS

```
Bucket: rustshare-data
├── users/
│   ├── {user_id}/
│   │   ├── files/
│   │   │   ├── {file_id}/
│   │   │   │   ├── v1         # original upload
│   │   │   │   ├── v2         # first modification
│   │   │   │   ├── v3         # second modification
│   │   │   │   └── current    # symlink/metadata (optional)
│   │   └── thumbnails/
│   │       ├── {file_id}/
│   │       │   ├── small.jpg  # 200x200
│   │       │   └── large.jpg  # 800x800
```

### 5.3 Version Retention Policy

**Configurable per-user or globally:**

- **0-30 days:** Keep all versions
- **30-90 days:** Keep major versions only (every 5th version, or tagged versions)
- **90+ days:** Keep only current + initial version

**Background Job:**
- Runs daily
- Checks version age against retention policy
- Deletes old versions from RustFS
- Updates database to mark versions as pruned

### 5.4 Version Restoration

**User can restore any previous version:**

- Web UI: Browse version history → "Restore this version"
- API: `POST /api/files/{id}/restore` with `version_number`

**Process:**
- Creates new version (doesn't overwrite current)
- Copies content from old version's storage key
- Emits `FileRestored` event
- New version becomes current

### 5.5 Deduplication Benefits

- If user uploads same file twice, only one copy stored in RustFS
- If two users upload identical files, shared storage (with separate metadata)
- Saves storage space and bandwidth
- Version records track different logical uses of same content

---

## 6. Sharing & Access Control

### 6.1 Share Link Creation

**User Flow:**
1. Select file/folder in web UI, click "Share"
2. Configure share options:
   - Permission level: Read-only or Read-Write
   - Password protection (optional)
   - Expiration date/time (optional)
3. Server generates:
   - Unique share token (128-bit cryptographic random)
   - Share record in database
   - Shareable URL: `https://rustshare.example.com/s/{token}`

### 6.2 Share Access

**Recipient Flow:**
1. Visit share link
2. If password-protected: prompt for password
3. If expired or revoked: show error
4. If valid: show file preview/download page or folder listing
5. Increment access counter, log access event

### 6.3 Permission Model (v1 - Simple)

**User Permissions:**
- **Owner:** Full control (read, write, delete, share, manage versions)
- **No access:** Default for all other users

**Share Permissions:**
- **Anonymous (via token):**
  - Read-only: view/download
  - Read-write: view/download/upload to folder (future enhancement)

### 6.4 Architecture for Future Growth

**Extensible permission system:**

```rust
// V1 - Simple
enum Permission {
    Owner,
    SharedReadOnly,
    SharedReadWrite,
}

// Future - expandable without refactoring
struct PermissionSet {
    user_id: UserId,
    file_id: FileId,
    permissions: Vec<PermissionType>,  // [Read, Write, Delete, Share, Admin]
    granted_by: UserId,
    granted_at: DateTime<Utc>,
}
```

**Future enhancements can add:**
- Team/group permissions
- Role-based access control
- Fine-grained ACLs per file/folder
- Inherited permissions from parent folders

### 6.5 Share Management

**Owner can:**
- View all active shares created by them
- Revoke share (token becomes invalid immediately)
- Update share (change password, expiry, permissions)
- View analytics (access count, last accessed time)

**Security:**
- Share tokens are cryptographically random (128+ bit entropy)
- Passwords hashed with bcrypt
- Expiration checked on every access
- Access logging for audit trail

---

## 7. Preview & Thumbnail Generation

### 7.1 Supported File Types (v1)

| Category | Formats | Preview Method |
|----------|---------|----------------|
| **Images** | JPEG, PNG, GIF, WebP, SVG | Full preview in browser |
| **PDFs** | PDF | Browser's built-in PDF viewer |
| **Videos** | MP4, WebM | HTML5 video player with streaming |
| **Text** | TXT, MD, source code | Syntax-highlighted preview |
| **Office** | DOCX, XLSX, PPTX | Thumbnail only (future: viewer) |

### 7.2 Thumbnail Generation

**Workflow:**

1. File uploaded to RustFS, `FileUploaded` event emitted
2. Background worker (async task) picks up event
3. Worker determines file type from MIME type
4. If previewable:
   - Download file from RustFS (or stream)
   - Generate two thumbnails: **small (200x200)** and **large (800x800)**
   - Maintain aspect ratio, add padding if needed
   - Upload thumbnails to RustFS at `thumbnails/{file_id}/small.jpg` and `large.jpg`
   - Update file metadata with thumbnail status

**Libraries:**
- **Images:** `image` crate (JPEG/PNG/GIF/WebP)
- **PDFs:** `pdfium-render` or `pdf-render` crate
- **Videos:** `ffmpeg` via `tokio-process` for frame extraction
- **SVG:** Serve directly, no thumbnail needed

### 7.3 Preview Delivery

**Web UI:**
- Grid/list view shows small thumbnails
- Click file → preview modal with large preview
- Preview modal features:
  - Images: full-size with zoom/pan
  - PDFs: embedded viewer or thumbnail + download
  - Videos: HTML5 player with controls
  - Text: syntax-highlighted code block
  - Unsupported: file icon + download button

**API Endpoints:**
- `GET /api/files/{id}/thumbnail?size=small|large` - serve thumbnail
- `GET /api/files/{id}/preview` - serve preview (embeddable content)
- `GET /api/files/{id}/download` - original file download

### 7.4 Performance & Error Handling

**Performance:**
- Thumbnails stored in RustFS, served via presigned URLs
- Lazy generation: generate on first preview request if not cached
- Background job processes thumbnail queue asynchronously

**Error Handling:**
- If generation fails (corrupted file, unsupported format), store placeholder icon
- Don't retry indefinitely, mark as `preview_unavailable` after 3 attempts
- Log errors for debugging

---

## 8. Authentication & Security

### 8.1 Authentication Methods

**Web UI (Primary):**
- Email/password login
- Passwords hashed with **Argon2id** (memory-hard, GPU-resistant)
- Minimum password strength: 8+ characters (configurable)
- Returns JWT token with 24-hour expiry
- Refresh token for extended sessions (httpOnly cookie)

**Protocol Authentication:**
- **WebDAV:** HTTP Basic Auth (username/password) or Bearer token (JWT)
- **S3 API:** AWS Signature V4 (access key ID + secret key)
  - Each user gets API credentials (generated, stored hashed)

### 8.2 JWT Structure

**Claims:**
```json
{
  "sub": "user-uuid",
  "email": "user@example.com",
  "exp": 1234567890,
  "iat": 1234567890,
  "iss": "rustshare"
}
```

**Storage:**
- JWT in httpOnly cookie (CSRF protection via SameSite=Strict)
- Optional: session table in database for revocation capability
- WebSocket authentication uses same JWT

### 8.3 Security Measures

**Transport Security:**
- TLS/HTTPS required for all connections
- Let's Encrypt integration for automatic certificates
- HTTP Strict Transport Security (HSTS) headers

**Rate Limiting:**
- Per-IP rate limits on auth endpoints (prevent brute force)
- Per-user rate limits on API endpoints (prevent abuse)
- Implemented via middleware (tower-governor or similar)

**File Upload Security:**
- Maximum file size limit (configurable, default: 5GB)
- MIME type validation (reject executables if configured)
- Virus scanning hook (optional ClamAV integration)
- Content-Security-Policy headers prevent XSS

**Share Link Security:**
- Cryptographically random tokens (128-bit entropy minimum)
- Password protection (bcrypt hashed)
- Expiration enforcement (checked on every access)
- Access logging for audit trail

**SQL Injection Prevention:**
- Use prepared statements/parameterized queries (sqlx query macros)
- Never construct SQL from user input

**Data at Rest:**
- RustFS handles storage encryption (if enabled)
- Sensitive database fields (password hashes, API keys) stored securely
- Future: optional client-side encryption

**Privacy:**
- No telemetry or analytics by default
- User data stays in deployment environment
- GDPR compliance: data export, account deletion with cascade delete

---

## 9. Frontend Web UI

### 9.1 Technology Stack

**Recommended: SvelteKit** or **React + Vite**

**Core Stack:**
- **TypeScript** - type safety for API interactions
- **TailwindCSS** - rapid UI development, mobile-responsive
- **Shadcn/ui or DaisyUI** - component libraries
- **Tanstack Query** - data fetching, caching, optimistic updates

### 9.2 UI Structure

**Authentication Pages:**
- Login page
- Registration page (if self-registration enabled)
- Password reset flow

**Main Application:**

**1. File Browser (Primary View):**
- Left sidebar: folder tree navigation
- Main area: file grid/list with thumbnails
- Top bar: breadcrumb navigation, search, upload button
- Right panel: file details (selected file)
- Context menu: right-click for file operations

**2. File Operations:**
- **Upload modal:** drag-drop zone, progress bars, batch uploads
- **Preview modal:** full-screen preview with version history sidebar
- **Share dialog:** create share with options (password, expiry, permissions)
- **Conflict resolution modal:** side-by-side comparison, resolution buttons

**3. User Settings:**
- Profile settings
- Storage usage visualization (pie chart, breakdown by folder)
- API credentials management
- Active sessions list

**4. Share Access Page (Public):**
- Standalone page for share links (no login required)
- Password entry if protected
- File preview/download interface

### 9.3 Real-Time Features

**WebSocket Integration:**
- Establish connection on login
- Subscribe to user's root folder changes
- Live updates:
  - New files appear automatically
  - Modified files update in place (show "updated by X" notification)
  - Deleted files fade out with animation
  - Conflict notifications trigger modal

### 9.4 Responsive Design

**Desktop (1024px+):**
- Three-column layout: folder tree | file grid | details panel
- Keyboard shortcuts (arrow keys, delete, cmd+A, etc.)
- Drag-and-drop for uploads and moving files

**Tablet (768px-1023px):**
- Two-column layout: collapsible sidebar + main content
- Touch-friendly buttons and spacing

**Mobile (< 768px):**
- Single column, hamburger menu for navigation
- Bottom nav bar for primary actions (upload, search, menu)
- Optimized for touch gestures (swipe to delete, long-press menu)

**Progressive Web App (PWA):**
- Service worker for offline detection
- Install prompt for "add to home screen"
- Future: offline file caching

### 9.5 Performance Considerations

- **Virtual scrolling** for large file lists (thousands of files)
- **Lazy loading** thumbnails as user scrolls
- **Optimistic updates** - show upload immediately, sync in background
- **Code splitting** - lazy load preview components by file type
- **Image optimization** - serve WebP with JPEG fallback

### 9.6 Accessibility

- Keyboard navigation throughout
- Screen reader support (ARIA labels)
- Focus management for modals
- High contrast mode support
- Color-blind friendly palette

---

## 10. Error Handling & Edge Cases

### 10.1 Storage Errors (RustFS)

**Network failures:**
- Retry with exponential backoff (3 attempts: 1s, 2s, 4s)
- If all retries fail, queue operation in database for later processing

**Storage full:**
- Return clear error message to user
- Suggest cleanup or quota upgrade
- Background job to notify admins

**Corrupted upload:**
- Validate content hash after upload
- If mismatch, reject and ask client to retry
- Log incident for investigation

**RustFS unavailable:**
- Queue write operations in database
- Process when RustFS becomes available
- Read operations return cached data or error

### 10.2 Conflict Scenarios

**Simple conflict:**
- Two users edit same file
- Second user gets conflict detection
- User prompted with resolution UI

**Three-way conflict:**
- Multiple users editing during network partition
- Last detector gets prompted with all versions
- Event ordering determines canonical sequence

**Rapid-fire updates:**
- Use event sequence numbers to determine order
- Database constraints prevent out-of-order updates

**Client crash during upload:**
- Orphaned partial uploads cleaned by background job (after 24h)
- If client reconnects, can resume upload

### 10.3 Network/Connection Issues

**WebSocket disconnect:**
- Client auto-reconnects with exponential backoff
- Resume subscription after reconnection
- Catch up on missed events

**Upload interruption:**
- Support resumable uploads (tus protocol or custom chunked upload)
- Store upload progress in database

**Download interruption:**
- HTTP range requests for resume capability
- Standard browser behavior

**Long-running operations:**
- Return 202 Accepted with operation ID
- Client polls for status at `/api/operations/{id}`

### 10.4 Authentication/Authorization Errors

**Expired JWT:**
- Return 401 Unauthorized
- Frontend automatically refreshes token (if refresh token valid)
- Seamless to user

**Invalid share token:**
- Return 404 Not Found (not 403, avoid leaking existence)
- Show error page to user

**Insufficient permissions:**
- Return 403 Forbidden with clear message
- Frontend shows appropriate error

**Rate limit exceeded:**
- Return 429 Too Many Requests with Retry-After header
- Frontend shows temporary lockout message

### 10.5 Data Integrity

**Hash mismatch:**
- File corrupted in transit
- Reject upload, ask client to retry
- Log incident

**Version race condition:**
- Database constraint violation (unique constraint on version)
- Return conflict error, trigger conflict resolution flow

**Event ordering issues:**
- Events have sequence numbers
- Detect gaps, request replay from event store

**Database connection loss:**
- Use connection pool with health checks
- Retry queries with exponential backoff
- Circuit breaker pattern for repeated failures

### 10.6 Common Edge Cases

**Filename Conflicts:**
- Two files uploaded with same name to same folder
- Auto-rename second file: "filename (2).ext"
- User can rename afterwards

**Massive File Operations:**
- Deleting folder with thousands of files
- Process asynchronously with progress tracking
- Soft delete first, purge storage in background job

**Quota Enforcement:**
- Check quota before accepting upload
- Real-time quota tracking via `user_storage_usage` table
- Background job reconciles if out of sync

**Share Link Edge Cases:**
- Share created → file deleted → share returns 410 Gone
- Password-protected share with wrong password → rate limit attempts
- Expired share → clear message with expiration date

**Time Synchronization:**
- All timestamps in UTC
- Conflict detection uses server time as source of truth
- Client clock skew tolerance: ±5 minutes for version checks

**Concurrent Operations:**
- Multiple tabs open: events broadcast to all WebSocket connections
- Prevent double-upload: deduplicate by content hash
- Database transactions for critical operations

### 10.7 Graceful Degradation

**If WebSockets fail:**
- Fall back to polling (every 30 seconds)
- Show warning banner to user

**If thumbnails fail:**
- Show file type icon instead
- User can still download/preview file

**If preview unavailable:**
- Offer download only
- Show error message explaining why

### 10.8 Logging & Monitoring

**Error Tracking:**
- Log all errors with context (user_id, operation, stacktrace)
- Structured logging (JSON format)
- Integration with error tracking service (Sentry, etc.)

**Performance Metrics:**
- Track upload/download speeds
- API endpoint latency
- WebSocket connection count
- Database query performance

**Audit Log:**
- Security events (login attempts, failed auth, share creation)
- File access via share links
- Admin actions

**Health Checks:**
- `/health` endpoint for load balancer
- Check database connectivity
- Check RustFS connectivity
- Return 200 OK if healthy, 503 Service Unavailable if not

---

## 11. Development Workflow

### 11.1 Repository Structure

```
rustshare/
├── backend/                    # Rust server (Cargo workspace)
│   ├── Cargo.toml             # workspace definition
│   ├── crates/
│   │   ├── core/              # business logic
│   │   ├── storage/           # data persistence
│   │   ├── protocols/         # HTTP, WebDAV, S3
│   │   ├── sync/              # real-time sync engine
│   │   └── auth/              # authentication
│   ├── server/                # main binary
│   │   ├── main.rs
│   │   └── Cargo.toml
│   └── migrations/            # SQL migrations (sqlx)
├── frontend/                  # Web UI (SvelteKit or React)
│   ├── src/
│   ├── package.json
│   └── vite.config.ts
├── docker/
│   ├── backend.Dockerfile
│   ├── frontend.Dockerfile
│   └── nginx.conf             # production reverse proxy
├── docker-compose.yml         # base services
├── docker-compose.dev.yml     # development overrides
├── docs/
│   └── superpowers/
│       └── specs/
│           └── 2026-03-17-rustshare-design.md
└── README.md
```

### 11.2 Docker Compose Configuration

**docker-compose.yml (Base):**

```yaml
version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: rustshare
      POSTGRES_USER: rustshare
      POSTGRES_PASSWORD: changeme
    volumes:
      - postgres_data:/var/lib/postgresql/data
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U rustshare"]
      interval: 10s
      timeout: 5s
      retries: 5

  rustfs:
    image: rustfs/rustfs:latest
    environment:
      RUSTFS_ROOT_USER: rustfsadmin
      RUSTFS_ROOT_PASSWORD: rustfsadmin
    volumes:
      - rustfs_data:/data
    ports:
      - "9000:9000"  # S3 API
      - "9001:9001"  # Console UI
    command: server /data
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:9000/minio/health/live"]
      interval: 10s
      timeout: 5s
      retries: 5

  backend:
    build:
      context: .
      dockerfile: docker/backend.Dockerfile
    environment:
      DATABASE_URL: postgres://rustshare:changeme@postgres/rustshare
      RUSTFS_ENDPOINT: http://rustfs:9000
      RUSTFS_ACCESS_KEY: rustfsadmin
      RUSTFS_SECRET_KEY: rustfsadmin
      RUSTFS_BUCKET: rustshare-data
      JWT_SECRET: change-me-in-production
      RUST_LOG: info
    depends_on:
      postgres:
        condition: service_healthy
      rustfs:
        condition: service_healthy
    ports:
      - "8080:8080"
    volumes:
      - ./backend:/app
      - cargo_cache:/usr/local/cargo

  frontend:
    build:
      context: .
      dockerfile: docker/frontend.Dockerfile
    environment:
      VITE_API_URL: http://localhost:8080
      VITE_WS_URL: ws://localhost:8080
    depends_on:
      - backend
    ports:
      - "3000:3000"
    volumes:
      - ./frontend:/app
      - node_modules:/app/node_modules

volumes:
  postgres_data:
  rustfs_data:
  cargo_cache:
  node_modules:
```

**docker-compose.dev.yml (Development Overrides):**

```yaml
version: '3.8'

services:
  backend:
    command: cargo watch -x run
    volumes:
      - ./backend:/app:delegated
      - cargo_cache:/usr/local/cargo
    environment:
      RUST_LOG: debug
      RUST_BACKTRACE: 1

  frontend:
    command: npm run dev -- --host 0.0.0.0
    volumes:
      - ./frontend:/app:delegated
      - node_modules:/app/node_modules
```

### 11.3 Development Commands

**Start all services with hot-reload:**
```bash
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up
```

**Run database migrations:**
```bash
docker-compose exec backend sqlx migrate run
```

**Access services:**
- Frontend: http://localhost:3000
- Backend API: http://localhost:8080
- RustFS Console: http://localhost:9001 (rustfsadmin / rustfsadmin)
- PostgreSQL: localhost:5432

**View logs:**
```bash
docker-compose logs -f backend
docker-compose logs -f frontend
```

**Rebuild after dependency changes:**
```bash
docker-compose build backend
docker-compose build frontend
```

### 11.4 Backend Development (without Docker)

**Setup:**
```bash
cd backend
cp .env.example .env
sqlx database create
sqlx migrate run
```

**Run with hot-reload:**
```bash
cargo install cargo-watch
cargo watch -x run
```

**Run tests:**
```bash
cargo test
cargo test --workspace  # all crates
```

**Check code:**
```bash
cargo clippy
cargo fmt --check
```

### 11.5 Frontend Development (without Docker)

**Setup:**
```bash
cd frontend
npm install
```

**Run dev server:**
```bash
npm run dev
```

**Type checking:**
```bash
npm run check
```

**Build for production:**
```bash
npm run build
npm run preview  # preview production build
```

### 11.6 Database Migrations

Using `sqlx-cli`:

**Create migration:**
```bash
sqlx migrate add create_users_table
```

**Run migrations:**
```bash
sqlx migrate run
```

**Revert last migration:**
```bash
sqlx migrate revert
```

### 11.7 Testing Strategy

**Unit Tests:**
- Test core business logic in `core/` crate
- No I/O dependencies, fast execution
- Run with `cargo test -p rustshare-core`

**Integration Tests:**
- Test API endpoints, protocol adapters
- Use test database (separate from dev database)
- Run with `cargo test --test integration_tests`

**End-to-End Tests:**
- Playwright or Cypress for frontend user flows
- Test critical paths: login, upload, share, conflict resolution
- Run with `npm run test:e2e`

**Load Tests:**
- Test WebSocket connections (thousands of concurrent clients)
- Test concurrent uploads (simulate multiple users)
- Use `k6` or `wrk` for HTTP load testing

---

## 12. Deployment & Scaling Considerations

### 12.1 Production Deployment

**Minimum Requirements:**
- 2 CPU cores
- 4GB RAM
- 50GB disk (plus storage for RustFS)
- TLS certificate (Let's Encrypt)

**Recommended Setup:**
- Docker Compose on single server (for small deployments)
- Kubernetes for larger deployments (future)

**Production docker-compose.yml additions:**
- Nginx reverse proxy with TLS termination
- Automated backups for PostgreSQL
- Health checks for all services
- Resource limits (CPU/memory)

### 12.2 Scaling Strategy

**Vertical Scaling (Initial):**
- Increase CPU/RAM as user base grows
- PostgreSQL can handle thousands of concurrent connections
- RustFS scales with storage capacity

**Horizontal Scaling (Future):**
- Multiple backend instances behind load balancer
- Shared PostgreSQL with read replicas
- Redis for WebSocket session management (Pub/Sub)
- RustFS distributed mode (multiple nodes)

**Bottlenecks to Monitor:**
- Database connections (connection pooling)
- WebSocket connections per instance
- RustFS throughput (network bandwidth)
- Thumbnail generation queue

### 12.3 Backup & Disaster Recovery

**PostgreSQL Backups:**
- Daily automated backups (pg_dump)
- Point-in-time recovery (WAL archiving)
- Store backups in separate location (S3 or equivalent)

**RustFS Backups:**
- RustFS bucket replication to separate region (if supported)
- Or periodic sync to backup storage

**Recovery Testing:**
- Test restore procedure monthly
- Document recovery steps
- Measure RTO (Recovery Time Objective) and RPO (Recovery Point Objective)

---

## 13. Future Enhancements

**Phase 2 (Post-MVP):**
- Native mobile apps (iOS, Android) with background sync
- Desktop clients (macOS, Windows, Linux) with local sync folder
- Real-time collaboration indicators (who's viewing/editing)
- Full-text search across file contents
- Team/group permissions
- Activity feed and notifications
- Two-factor authentication (TOTP, WebAuthn)

**Phase 3 (Advanced):**
- End-to-end encryption (client-side)
- Advanced conflict resolution (three-way merge for text files)
- Integration with external storage (Google Drive, Dropbox sync)
- Advanced admin dashboard with analytics
- API webhooks for integrations
- Collaborative document editing (integrate Yrs/Automerge)

---

## 14. Open Questions & Decisions Needed

**Before Implementation:**

1. **Frontend Framework Choice:** SvelteKit vs React?
   - Recommendation: SvelteKit (leaner, better real-time support)

2. **Database:** PostgreSQL sufficient, or add Redis for caching?
   - Recommendation: Start with PostgreSQL only, add Redis if WebSocket scaling needed

3. **Thumbnail Generation:** Synchronous (blocking) vs async (background job)?
   - Recommendation: Async background job for better UX

4. **Self-Registration:** Allow public sign-ups or invite-only?
   - Recommendation: Start with **invite-only** (admin creates users) for v1. Public self-registration can be added as optional config in Phase 2. This keeps initial deployment secure and simple, suitable for personal/team use cases without requiring email verification, captcha, or anti-abuse measures.

5. **Storage Limits:** Default quota per user?
   - Recommendation: 10GB initially, configurable

6. **License:** Apache 2.0, MIT, or GPL?
   - Recommendation: Apache 2.0 (matches RustFS)

---

## 15. Success Criteria

**MVP is complete when:**

✅ User can register, login, and manage profile
✅ User can upload files via web UI (with progress indicators)
✅ User can browse files in folder structure
✅ User can download files
✅ File versioning works (view history, restore versions)
✅ Conflict detection triggers on simultaneous edits
✅ Conflict resolution UI allows user to choose version
✅ Share links work (with password and expiry options)
✅ Previews and thumbnails generate for supported file types
✅ WebDAV clients can connect and sync files
✅ S3-compatible tools (rclone) can access storage
✅ Mobile-responsive web UI works on phones/tablets
✅ Real-time sync updates all connected clients
✅ Docker Compose deployment works out-of-the-box

**Performance Targets:**
- File upload: < 2s for 10MB file (excluding network transfer time)
- WebSocket latency: < 100ms for change notifications
- Thumbnail generation: < 5s for typical image file
- API response time: < 200ms for metadata queries

---

## 16. Conclusion

RustShare provides a modern, performant, and open-source alternative to existing file sync and share solutions. By leveraging Rust's performance and safety guarantees, event-sourced architecture for real-time features, and RustFS for scalable storage, the platform is positioned to start simple while supporting future growth.

The modular monolith architecture allows rapid initial development while maintaining clean boundaries for future microservices extraction if needed. Multi-protocol support ensures compatibility with existing tools and workflows, lowering barriers to adoption.

**Next Steps:**
1. Review and approve this design specification
2. Create implementation plan (using writing-plans skill)
3. Set up development environment (Docker Compose)
4. Begin implementation with core modules (auth, file operations)
5. Iterate with user feedback

---

**End of Design Specification**
