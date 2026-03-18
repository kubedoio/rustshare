# RustShare Phase 3B: Public Share Links Design Specification

**Date:** 2026-03-18
**Status:** Draft - Pending Review
**Phase:** 3B - Public Share Links (Extends Phase 3A Real-time Sync)

## Overview

Phase 3B adds public share link functionality to RustShare, enabling file owners to create shareable URLs that anonymous users can access to view and optionally upload new versions of files. Share viewers receive real-time WebSocket notifications when shared files change, providing a collaborative experience without requiring user accounts.

## Goals

1. Enable file owners to create public share links with configurable permissions
2. Support password-protected and time-limited shares for security
3. Provide real-time notifications to share viewers when files change
4. Allow ReadWrite share links to accept file version uploads
5. Track share access for analytics and security monitoring
6. Build on Phase 3A's WebSocket infrastructure with minimal new code

## Non-Goals (Phase 3B)

- User-to-user sharing between authenticated RustShare users (deferred to Phase 3C)
- Folder sharing (files only in Phase 3B)
- Share link analytics dashboard (basic access tracking only)
- Public file previews/thumbnails (out of scope)
- Share link customization (vanity URLs, branding)
- Batch operations on shared files

## Architecture

### High-Level Design

Phase 3B extends Phase 3A's WebSocket notification system to support anonymous share viewers. Instead of creating a separate WebSocket endpoint, share viewers receive temporary session tokens that grant access to the existing `/api/sync` endpoint.

**Data Flow:**
```
Owner creates share → ShareService generates token → Store in database
                                                            ↓
Anonymous user visits /public/share/:token → Validate password/expiry
                                                            ↓
Issue session token (JWT with share_id) → Return to viewer
                                                            ↓
Viewer uses session token for:
  - GET /public/share/:token/download (fetch file content)
  - POST /public/share/:token/upload (upload new version if ReadWrite)
  - WebSocket /api/sync (subscribe to file updates)
                                                            ↓
File modified → EventStore::append → EventBroadcaster
                                                            ↓
WebSocket handler filters events:
  - Send to owner (user_id match)
  - Send to share viewers (share_id match + file_id match)
```

### Key Design Decisions

1. **Session Token Pattern:** Share viewers receive short-lived JWT-like tokens (1 hour expiry) after validating share access. This reuses existing JWT infrastructure and forces periodic re-validation of share status.

2. **Single WebSocket Endpoint:** Extend `/api/sync` to accept both user JWTs and session tokens. The handler determines client identity and filters events accordingly. This avoids duplicate WebSocket infrastructure.

3. **Files Only:** Phase 3B supports sharing individual files, not folders. This limits scope and avoids complexity around recursive permissions and folder navigation UI.

4. **ReadWrite = Version Upload Only:** ReadWrite permission allows uploading new file versions but not renaming, moving, or deleting the file. These are owner-only operations.

5. **Soft Delete Revocation:** Revoked shares set `revoked_at` timestamp instead of deleting the row. This preserves audit trails while immediately invalidating the token.

6. **Best-Effort Notification Delivery:** Share viewers use the same catch-up mechanism as Phase 3A. If they lag behind EventBroadcaster capacity, they receive a "lagged" message and must re-sync.

## Components

### 1. ShareService

**Location:** `backend/crates/core/src/services/share_service.rs`

**Responsibilities:**
- Create/revoke/update share links
- Validate share access (token + password + expiration)
- Issue session tokens for valid share access
- Track access counts and last accessed timestamps

**Interface:**
```rust
pub struct ShareService<E, M>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
{
    event_store: Arc<E>,
    metadata_store: Arc<M>,
    jwt_manager: Arc<JwtManager>,
}

impl<E, M> ShareService<E, M>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
{
    /// Create new share link for a file
    pub async fn create_share(
        &self,
        file_id: FileId,
        created_by: UserId,
        permissions: SharePermissions,
        password: Option<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Share>;

    /// Validate share access and return session token
    pub async fn validate_and_create_session(
        &self,
        share_token: String,
        password: Option<String>,
    ) -> Result<ShareSession>;

    /// Revoke share link (soft delete)
    pub async fn revoke_share(
        &self,
        share_id: ShareId,
        user_id: UserId,
    ) -> Result<()>;

    /// Update share settings (password, expiry)
    pub async fn update_share(
        &self,
        share_id: ShareId,
        user_id: UserId,
        password: Option<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Share>;

    /// List all shares for a file (owner only)
    pub async fn list_file_shares(
        &self,
        file_id: FileId,
        user_id: UserId,
    ) -> Result<Vec<Share>>;

    /// Get share by token (for validation)
    async fn get_share_by_token(
        &self,
        share_token: &str,
    ) -> Result<Share>;
}
```

**Token Generation:**
```rust
use rand::Rng;

fn generate_share_token() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    const TOKEN_LENGTH: usize = 32;  // 32 chars fits in VARCHAR(64) with room for future expansion

    let mut rng = rand::thread_rng();
    (0..TOKEN_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
```

**Session Token Structure:**

JWT claims for share session tokens include custom fields beyond standard claims:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct ShareSessionClaims {
    pub sub: String,  // Format: "share:{share_id}"
    pub share_id: ShareId,
    pub file_id: FileId,
    pub permissions: SharePermissions,
    pub iat: i64,
    pub exp: i64,
}

impl ShareSessionClaims {
    pub fn new(
        share_id: ShareId,
        file_id: FileId,
        permissions: SharePermissions,
        ttl_seconds: i64,
    ) -> Self {
        let now = Utc::now().timestamp();
        Self {
            sub: format!("share:{}", share_id),
            share_id,
            file_id,
            permissions,
            iat: now,
            exp: now + ttl_seconds,
        }
    }
}
```

**JWT Token Examples:**

User JWT (Phase 3A):
```json
{
  "sub": "user:550e8400-e29b-41d4-a716-446655440000",
  "email": "user@example.com",
  "iat": 1710753600,
  "exp": 1710757200
}
```

Share Session Token (Phase 3B):
```json
{
  "sub": "share:660e8400-e29b-41d4-a716-446655440001",
  "share_id": "660e8400-e29b-41d4-a716-446655440001",
  "file_id": "770e8400-e29b-41d4-a716-446655440002",
  "permissions": "Read",
  "iat": 1710753600,
  "exp": 1710757200
}
```

**Token Encoding/Decoding:**

JwtManager should be extended to support custom claims:
```rust
// Encode share session
let claims = ShareSessionClaims::new(share_id, file_id, permissions, 3600);
let token = jwt_manager.encode_custom_claims(&claims)?;

// Decode and validate
let decoded = jwt_manager.decode_token(&token)?;
if decoded.sub.starts_with("share:") {
    let session: ShareSessionClaims = serde_json::from_value(
        serde_json::to_value(&decoded)?
    )?;
    // Use session.share_id, session.file_id, session.permissions
}
```

### 2. Database Schema Changes

**Existing `shares` table:**
```sql
CREATE TABLE shares (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    share_token VARCHAR(64) NOT NULL UNIQUE,
    permissions VARCHAR(20) NOT NULL,  -- 'Read' or 'ReadWrite'
    password_hash TEXT,
    expires_at TIMESTAMP WITH TIME ZONE,
    access_count INTEGER DEFAULT 0,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_shares_file_id ON shares(file_id);
CREATE INDEX idx_shares_token ON shares(share_token);
```

**New columns needed:**
```sql
ALTER TABLE shares ADD COLUMN revoked_at TIMESTAMP WITH TIME ZONE;
ALTER TABLE shares ADD COLUMN last_accessed_at TIMESTAMP WITH TIME ZONE;

CREATE INDEX idx_shares_active ON shares(share_token)
WHERE revoked_at IS NULL;
```

**New `share_access_log` table (optional, for analytics):**
```sql
CREATE TABLE share_access_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    share_id UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    accessed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    ip_address INET,
    user_agent TEXT,
    action VARCHAR(50) NOT NULL,  -- 'view', 'download', 'upload'
    success BOOLEAN NOT NULL DEFAULT true
);

CREATE INDEX idx_share_access_log_share_id ON share_access_log(share_id);
CREATE INDEX idx_share_access_log_accessed_at ON share_access_log(accessed_at);
```

### 3. MetadataStore Extensions

**New Methods:**
```rust
pub trait MetadataStoreOps: Send + Sync {
    // Existing methods...

    /// Create new share link
    async fn create_share(&self, share: &Share) -> Result<()>;

    /// Get share by token
    async fn get_share_by_token(&self, token: &str) -> Result<Option<Share>>;

    /// Get share by ID
    async fn get_share(&self, share_id: ShareId) -> Result<Option<Share>>;

    /// List all active shares for a file
    async fn get_file_shares(&self, file_id: FileId) -> Result<Vec<Share>>;

    /// Update share settings
    async fn update_share(&self, share: &Share) -> Result<()>;

    /// Soft delete share (set revoked_at)
    async fn revoke_share(&self, share_id: ShareId) -> Result<()>;

    /// Increment access count and update last_accessed_at
    async fn increment_share_access(&self, share_id: ShareId) -> Result<()>;

    /// Log share access event (optional)
    async fn log_share_access(
        &self,
        share_id: ShareId,
        ip_address: Option<String>,
        user_agent: Option<String>,
        action: &str,
        success: bool,
    ) -> Result<()>;
}
```

### 4. WebSocket Handler Extensions

**Extended `sync_handler` Logic:**
```rust
#[derive(Debug)]
enum ClientIdentity {
    User { user_id: UserId },
    ShareViewer {
        share_id: ShareId,
        file_id: FileId,
        permissions: SharePermissions
    },
}

pub async fn sync_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Response, (StatusCode, String)> {
    let token = auth.token();

    // Validate token and determine client identity
    let identity = match validate_token(&state.jwt_manager, token) {
        Ok(claims) if claims.sub.starts_with("user:") => {
            let user_id = Uuid::parse_str(&claims.sub[5..])
                .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid user ID".to_string()))?;
            ClientIdentity::User { user_id }
        }
        Ok(claims) if claims.sub.starts_with("share:") => {
            // Parse share session claims
            let share_claims: ShareSessionClaims = serde_json::from_value(claims.into())
                .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid session".to_string()))?;

            ClientIdentity::ShareViewer {
                share_id: share_claims.share_id,
                file_id: share_claims.file_id,
                permissions: share_claims.permissions,
            }
        }
        _ => return Err((StatusCode::UNAUTHORIZED, "Invalid token".to_string())),
    };

    info!("WebSocket connection established: {:?}", identity);

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, identity, state)))
}

async fn handle_socket(socket: WebSocket, identity: ClientIdentity, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_rx = state.broadcaster.subscribe();

    // Event filtering based on client identity
    let should_send_event = |event: &Event| -> bool {
        match &identity {
            ClientIdentity::User { user_id } => {
                // Phase 3A behavior: send all user's events
                event.user_id == *user_id
            }
            ClientIdentity::ShareViewer { file_id, .. } => {
                // Phase 3B behavior: send only events for shared file
                event.aggregate_id == *file_id
                    && event.aggregate_type == AggregateType::File
            }
        }
    };

    // Rest of handler logic remains same as Phase 3A...
    // (catch-up mechanism, live event streaming, lagged handling)
}
```

## API Endpoints

### Share Management (Authenticated Owners)

#### Create Share Link
```
POST /api/files/:file_id/shares
Authorization: Bearer <user_jwt>
Content-Type: application/json

Request:
{
  "permissions": "Read" | "ReadWrite",
  "password": "optional_password",  // null for no password
  "expires_at": "2026-04-01T00:00:00Z"  // null for no expiration
}

Response: 201 Created
{
  "share_id": "uuid",
  "share_token": "abc123def456",
  "share_url": "https://rustshare.example.com/public/share/abc123def456",
  "permissions": "Read",
  "password_protected": true,
  "expires_at": "2026-04-01T00:00:00Z",
  "access_count": 0,
  "created_at": "2026-03-18T10:00:00Z"
}

Errors:
- 404 Not Found: File does not exist
- 403 Forbidden: User does not own file
- 400 Bad Request: Invalid request body
```

#### List Shares for File
```
GET /api/files/:file_id/shares
Authorization: Bearer <user_jwt>

Response: 200 OK
{
  "shares": [
    {
      "share_id": "uuid",
      "share_token": "abc123",
      "permissions": "Read",
      "password_protected": true,
      "expires_at": "2026-04-01T00:00:00Z",
      "access_count": 42,
      "last_accessed_at": "2026-03-18T09:30:00Z",
      "created_at": "2026-03-01T10:00:00Z",
      "revoked_at": null
    }
  ]
}

Errors:
- 404 Not Found: File does not exist
- 403 Forbidden: User does not own file
```

#### Update Share Settings
```
PATCH /api/shares/:share_id
Authorization: Bearer <user_jwt>
Content-Type: application/json

Request:
{
  "password": "new_password",  // null to remove password
  "expires_at": "2026-05-01T00:00:00Z"  // null to remove expiration
}

Response: 200 OK
{
  "share_id": "uuid",
  "share_token": "abc123",
  "password_protected": true,
  "expires_at": "2026-05-01T00:00:00Z"
}

Errors:
- 404 Not Found: Share does not exist
- 403 Forbidden: User does not own file
- 400 Bad Request: Invalid request body
```

#### Revoke Share Link
```
DELETE /api/shares/:share_id
Authorization: Bearer <user_jwt>

Response: 204 No Content

Errors:
- 404 Not Found: Share does not exist
- 403 Forbidden: User does not own file
```

### Public Share Access (Anonymous)

#### Access Share Link (Get Info)
```
GET /public/share/:token

Response: 200 OK (if no password required)
{
  "file_id": "uuid",
  "file_name": "document.pdf",
  "file_size": 1048576,
  "mime_type": "application/pdf",
  "version": 3,
  "permissions": "Read",
  "requires_password": false
}

Response: 401 Unauthorized (if password required)
{
  "error": "Password required",
  "requires_password": true
}

Response: 404 Not Found (if token invalid/expired/revoked)
{
  "error": "Share not found or expired"
}
```

#### Validate Password & Get Session Token
```
POST /public/share/:token/access
Content-Type: application/json

Request:
{
  "password": "user_provided_password"  // optional if not password-protected
}

Response: 200 OK
{
  "session_token": "jwt_encoded_session",
  "expires_at": "2026-03-18T11:00:00Z",  // 1 hour from now
  "file": {
    "file_id": "uuid",
    "file_name": "document.pdf",
    "file_size": 1048576,
    "mime_type": "application/pdf",
    "version": 3
  },
  "permissions": "Read"
}

Response: 401 Unauthorized
{
  "error": "Invalid password"
}

Response: 404 Not Found
{
  "error": "Share not found or expired"
}
```

#### Download Shared File
```
GET /public/share/:token/download
Authorization: Bearer <session_token>

Response: 200 OK
Content-Type: application/pdf
Content-Disposition: attachment; filename="document.pdf"

[file content]

Errors:
- 401 Unauthorized: Invalid/expired session token
- 404 Not Found: File not found
```

#### Upload New Version (ReadWrite only)
```
POST /public/share/:token/upload
Authorization: Bearer <session_token>
Content-Type: multipart/form-data

Request:
file: <binary data>

Response: 201 Created
{
  "file_id": "uuid",
  "version": 4,
  "content_hash": "sha256:...",
  "size": 1048576,
  "uploaded_at": "2026-03-18T10:15:00Z"
}

Errors:
- 403 Forbidden: Share link is read-only
- 401 Unauthorized: Invalid/expired session token
- 413 Payload Too Large: File exceeds quota
```

**Note:** The `:token` in the URL path is for API clarity and logging, but authorization is entirely based on the session token's embedded permissions. The handler validates that the session token's `file_id` matches the file being uploaded.

### WebSocket Real-Time Notifications

**Endpoint:** `ws://host/api/sync` (same as Phase 3A)

**Authentication:** Session token in `Authorization: Bearer <session_token>` header

**Protocol:**
```
// Client connects with session token
→ WebSocket Upgrade with Authorization header

// Optional: Client requests catch-up
→ {"type": "sync", "last_seen_event_id": "uuid"}
← [array of missed FileModified/FileDeleted events]

// Server streams live events
← {"event_id": "uuid", "event_type": "FileModified", "aggregate_id": "file_uuid", ...}
← {"event_id": "uuid", "event_type": "FileDeleted", "aggregate_id": "file_uuid", ...}

// If client lags behind
← {"type": "lagged", "message": "Too many events, please sync"}
```

**Event Filtering:**
- Share viewers only receive events where `event.aggregate_id == session.file_id`
- Event types: `FileModified`, `FileDeleted`
- Share viewers do NOT receive: `FileRenamed`, `FileMoved` (owner-only operations)

## Security Model

### Password Protection

**Hashing:**
- Use Argon2 via existing `PasswordHasher` from `rustshare_auth`
- Store only `password_hash` in database
- Hash at service layer before creating share

**Validation:**
```rust
if let Some(password_hash) = &share.password_hash {
    let is_valid = PasswordHasher::verify(&provided_password, password_hash)?;
    if !is_valid {
        return Err(ShareError::InvalidPassword);
    }
}
```

**Brute Force Protection:**
- Rate limit password attempts per share token (5 attempts per 15 minutes)
- Track failed attempts in-memory cache
- Return generic "Invalid password" error (no timing attacks)

### Token Security

**Share Tokens:**
- Cryptographically secure random generation (32 alphanumeric characters)
- URL-safe format (no special encoding needed)
- Stored in database as unique index
- Not guessable (62^32 = ~2^190 possible values)

**Session Tokens:**
- JWT format with HMAC-SHA256 signature
- Same secret as user JWTs (`JWT_SECRET` env var)
- Claims structure:
  ```json
  {
    "sub": "share:uuid",
    "share_id": "uuid",
    "file_id": "uuid",
    "permissions": "Read",
    "iat": 1710753600,
    "exp": 1710757200
  }
  ```
- Short expiration (1 hour default, configurable via `SHARE_SESSION_TTL`)
- No refresh tokens (must re-validate share access)

### Expiration Enforcement

**Share Link Expiration:**
```rust
pub fn is_expired(&self) -> bool {
    if let Some(expires_at) = self.expires_at {
        Utc::now() > expires_at
    } else {
        false
    }
}
```

**Session Token Expiration:**
- JWT `exp` claim validated by JwtManager
- Expired sessions return 401 Unauthorized
- Forces re-validation of share status every hour

**Revocation:**
- Soft delete: set `revoked_at` timestamp
- Check at validation time: `share.revoked_at.is_some()`
- Existing sessions remain valid until expiry (eventual consistency)

**Known Limitation:** Revoked shares with active session tokens can continue accessing files for up to 1 hour (session TTL). This tradeoff favors simplicity over immediate revocation. Mitigation: keep session TTL short (1 hour default) and implement in-memory revoked share cache if immediate revocation is required in the future.

### Access Control

**Read Permission Grants:**
- ✅ View share info
- ✅ Download file content
- ✅ WebSocket notifications
- ✅ View version history
- ❌ Upload new versions
- ❌ Rename, move, delete file

**ReadWrite Permission Grants:**
- ✅ All Read permissions
- ✅ Upload new file versions
- ❌ Rename, move, delete file (owner-only)

**Enforcement:**
```rust
// In upload handler
let session = validate_session_token(&token)?;

if session.permissions != SharePermissions::ReadWrite {
    return Err((StatusCode::FORBIDDEN, "Read-only share"));
}

// Proceed with upload...
```

### Access Logging

**What to Log:**
- Timestamp of access
- Share ID
- Action type (view, download, upload)
- IP address (optional, configurable)
- User agent (optional)
- Success/failure status

**Privacy:**
- IP logging optional via `SHARE_LOG_IP_ADDRESS` env var
- 30-day log retention policy (configurable)
- No personally identifiable information beyond IP
- Support data export/deletion for GDPR compliance

## Event Sourcing

### New Event Types

**ShareCreated:**
```rust
pub struct ShareCreatedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub share_token: String,
    pub permissions: SharePermissions,
    pub password_protected: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: UserId,
}
```

**ShareRevoked:**
```rust
pub struct ShareRevokedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub revoked_by: UserId,
}
```

**ShareUpdated:**
```rust
pub struct ShareUpdatedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub password_changed: bool,
    pub expires_at_changed: bool,
    pub new_expires_at: Option<DateTime<Utc>>,
    pub updated_by: UserId,
}
```

### EventType Extensions

```rust
pub enum EventType {
    // ... existing events

    // Share events
    ShareCreated,
    ShareRevoked,
    ShareUpdated,
}
```

## Testing Strategy

### Unit Tests (15+ tests)

**ShareService Tests:**
- `test_create_share_generates_unique_token()`
- `test_create_share_with_password_hashes_correctly()`
- `test_validate_share_with_valid_password_succeeds()`
- `test_validate_share_with_invalid_password_fails()`
- `test_validate_expired_share_fails()`
- `test_validate_revoked_share_fails()`
- `test_validate_share_without_password_succeeds()`
- `test_session_token_contains_correct_claims()`
- `test_session_token_expires_after_ttl()`
- `test_revoke_share_sets_revoked_at()`
- `test_update_share_password()`
- `test_update_share_expiration()`
- `test_list_file_shares_filters_by_owner()`
- `test_list_file_shares_excludes_revoked()`
- `test_increment_access_count()`

**Domain Model Tests:**
- Extend existing `Share` tests
- `test_share_permissions_read()`
- `test_share_permissions_read_write()`

**WebSocket Handler Tests:**
- `test_session_token_connects_successfully()`
- `test_expired_session_token_rejected()`
- `test_share_viewer_receives_file_modified_event()`
- `test_share_viewer_filters_events_by_file_id()` - verify viewer only sees their shared file
- `test_share_viewer_does_not_receive_folder_events()` - verify non-file events are filtered
- `test_user_and_share_viewer_both_receive_event()`
- `test_catch_up_for_share_viewer()`

### Integration Tests (8+ tests)

**Share Creation and Access:**
- `test_create_and_access_share_link()`
- `test_access_password_protected_share()`
- `test_access_expired_share_fails()`
- `test_access_revoked_share_fails()`

**File Operations via Share:**
- `test_download_shared_file()`
- `test_upload_version_with_read_write_share()`
- `test_upload_version_with_read_only_share_fails()`

**Real-Time Notifications:**
- `test_share_viewer_receives_file_update_notification()`
- `test_owner_and_share_viewer_both_notified()`
- `test_share_viewer_notified_when_file_deleted()`

### Success Criteria

**Functional:**
- ✅ Users can create share links with Read/ReadWrite permissions
- ✅ Share links support password protection and expiration
- ✅ Anonymous users can access shared files via token
- ✅ Share viewers receive real-time WebSocket notifications
- ✅ ReadWrite shares allow version uploads
- ✅ Read-only shares block modifications
- ✅ Owners can revoke and update shares
- ✅ Access counts tracked accurately

**Security:**
- ✅ Passwords hashed with Argon2
- ✅ Expired/revoked shares rejected
- ✅ Session tokens expire after 1 hour
- ✅ Permission boundaries enforced
- ✅ Invalid tokens return 401

**Performance:**
- ✅ Share validation < 100ms
- ✅ Session token generation < 50ms
- ✅ WebSocket notifications < 500ms latency

**Testing:**
- ✅ 15+ unit tests passing
- ✅ 8+ integration tests passing
- ✅ Security tests covering all attack vectors

## Migration Path

### Database Migrations

**Migration 1: Add share columns**
```sql
-- 20260318000002_add_share_revocation.sql
ALTER TABLE shares ADD COLUMN revoked_at TIMESTAMP WITH TIME ZONE;
ALTER TABLE shares ADD COLUMN last_accessed_at TIMESTAMP WITH TIME ZONE;

CREATE INDEX idx_shares_active ON shares(share_token)
WHERE revoked_at IS NULL;
```

**Migration 2: Create access log table**
```sql
-- 20260318000003_create_share_access_log.sql
CREATE TABLE share_access_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    share_id UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    accessed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    ip_address INET,
    user_agent TEXT,
    action VARCHAR(50) NOT NULL,
    success BOOLEAN NOT NULL DEFAULT true
);

CREATE INDEX idx_share_access_log_share_id ON share_access_log(share_id);
CREATE INDEX idx_share_access_log_accessed_at ON share_access_log(accessed_at);
```

### Configuration

**New Environment Variables:**
```bash
# Share session token TTL in seconds (default: 3600 = 1 hour)
SHARE_SESSION_TTL=3600

# Enable IP address logging in share_access_log (default: false)
SHARE_LOG_IP_ADDRESS=false

# Share access log retention in days (default: 30)
SHARE_LOG_RETENTION_DAYS=30
```

## Implementation Notes

### Rate Limiting

**Public Endpoints:**
- `/public/share/:token/access` - 10 requests per minute per IP
- `/public/share/:token/upload` - 5 uploads per hour per share token
- Implement using in-memory cache or Redis
- Return `429 Too Many Requests` with `Retry-After` header

**Password Attempts:**
- 5 failed attempts per share token per 15 minutes
- Track in-memory with automatic cleanup
- Reset counter on successful validation

### CORS Configuration

Public share endpoints require CORS headers for browser access:
```rust
.layer(CorsLayer::new()
    .allow_origin(Any)
    .allow_methods([Method::GET, Method::POST])
    .allow_headers([AUTHORIZATION, CONTENT_TYPE])
    .max_age(Duration::from_secs(3600)))
```

### File Size Limits

Upload endpoint file size limits:
- Default: 100MB per file (configurable via `MAX_FILE_SIZE` env var)
- Enforced at multipart parser level
- Return `413 Payload Too Large` if exceeded
- Check against owner's remaining quota before accepting upload

### Error Response Format

Standardized error format for all endpoints:
```json
{
  "error": "Human-readable message",
  "code": "ERROR_CODE",
  "details": "Optional additional context"
}
```

Error codes:
- `SHARE_NOT_FOUND` - Invalid or expired share token
- `SHARE_PASSWORD_REQUIRED` - Password required for access
- `SHARE_INVALID_PASSWORD` - Incorrect password
- `SHARE_REVOKED` - Share has been revoked
- `SHARE_EXPIRED` - Share expiration date passed
- `SESSION_EXPIRED` - Session token expired
- `PERMISSION_DENIED` - ReadWrite required for operation
- `QUOTA_EXCEEDED` - File upload would exceed owner's quota

### Background Jobs

**Access Log Cleanup:**
```rust
// Run daily at 3 AM
async fn cleanup_old_access_logs(metadata_store: &MetadataStore) {
    let retention_days = env::var("SHARE_LOG_RETENTION_DAYS")
        .unwrap_or("30".to_string())
        .parse()
        .unwrap_or(30);

    let cutoff = Utc::now() - Duration::days(retention_days);
    metadata_store.delete_access_logs_before(cutoff).await;
}
```

### TDD Approach

1. Write unit tests for ShareService methods
2. Implement ShareService to make tests pass
3. Write integration tests for share workflows
4. Implement HTTP handlers and WebSocket extensions
5. Write end-to-end tests for real-time notifications

### Code Organization

```
backend/
├── crates/
│   ├── core/
│   │   ├── domain/
│   │   │   └── share.rs (existing, minor updates)
│   │   ├── events/
│   │   │   └── types.rs (add ShareCreated/Revoked/Updated)
│   │   └── services/
│   │       └── share_service.rs (NEW)
│   ├── storage/
│   │   └── metadata_store.rs (add share methods)
│   └── auth/
│       └── jwt_manager.rs (extend for session tokens)
├── server/
│   └── src/
│       ├── handlers/
│       │   ├── shares.rs (NEW - share management)
│       │   ├── public.rs (NEW - public share access)
│       │   └── sync.rs (extend for session tokens)
│       └── main.rs (wire up new routes)
└── migrations/
    ├── 20260318000002_add_share_revocation.sql
    └── 20260318000003_create_share_access_log.sql
```

### Dependencies

No new dependencies needed. Uses existing:
- `axum` - HTTP/WebSocket server
- `tokio` - async runtime
- `sqlx` - database queries
- `serde` - JSON serialization
- `jsonwebtoken` - JWT handling
- `argon2` - password hashing
- `rand` - token generation

## Future Enhancements (Not in Phase 3B)

- User-to-user authenticated sharing (Phase 3C)
- Folder sharing with recursive permissions
- Share link analytics dashboard
- Custom share link domains/paths
- Share templates (default settings)
- Email notifications on share access
- Share link customization (branding, logos)
- Public file previews (thumbnails, PDF viewer)
- Download counts and bandwidth tracking
- Share link QR codes
- Time-limited download tokens
- Share link access control lists (IP whitelisting)
