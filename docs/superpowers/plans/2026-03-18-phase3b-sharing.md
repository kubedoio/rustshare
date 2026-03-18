# Phase 3B: Public Share Links Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement public share links for files with password protection, expiration, and real-time WebSocket notifications for share viewers.

**Architecture:** Extend Phase 3A's event sourcing and real-time sync infrastructure to support anonymous share access. ShareService manages share link creation/validation, JwtManager issues session tokens, and WebSocket sync_handler extends to serve share viewers.

**Tech Stack:** PostgreSQL (migrations), ShareService, JwtManager extensions, rate limiting middleware (in-memory LRU), background cleanup job (tokio interval).

---

## Task 1: Database Migrations

**Files:**
- Create: `backend/migrations/20260318000004_add_share_revocation.sql`
- Create: `backend/migrations/20260318000005_create_share_access_log.sql`

- [ ] **Step 1: Write migration for revoked_at and last_accessed_at**

Create 20260318000004_add_share_revocation.sql:
```sql
-- Add soft delete and access tracking to shares table
ALTER TABLE shares ADD COLUMN revoked_at TIMESTAMP WITH TIME ZONE;
ALTER TABLE shares ADD COLUMN last_accessed_at TIMESTAMP WITH TIME ZONE;

-- Index for active shares lookup (excludes revoked)
CREATE INDEX idx_shares_active ON shares(share_token) WHERE revoked_at IS NULL;

-- Comment on columns
COMMENT ON COLUMN shares.revoked_at IS 'Soft delete timestamp - share is revoked when not NULL';
COMMENT ON COLUMN shares.last_accessed_at IS 'Last time share was accessed via validate_and_create_session';
```

- [ ] **Step 2: Write migration for share_access_log table**

Create 20260318000005_create_share_access_log.sql:
```sql
-- Audit log for share access attempts
CREATE TABLE share_access_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    share_id UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    accessed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    ip_address INET,
    user_agent TEXT,
    action VARCHAR(50) NOT NULL, -- 'access', 'download', 'upload'
    success BOOLEAN NOT NULL DEFAULT true
);

-- Index for cleanup queries
CREATE INDEX idx_share_access_log_accessed_at ON share_access_log(accessed_at);

-- Index for share-specific queries
CREATE INDEX idx_share_access_log_share_id ON share_access_log(share_id);

COMMENT ON TABLE share_access_log IS 'Audit log for share link access attempts';
COMMENT ON COLUMN share_access_log.action IS 'Type of access: access (session), download, upload';
```

- [ ] **Step 3: Run migrations**

Run: `cd backend && sqlx migrate run`
Expected: Migrations applied successfully

- [ ] **Step 4: Verify schema**

Run: `psql -d rustshare_dev -c "\d shares"` and `\d share_access_log`
Expected: New columns and table present

- [ ] **Step 5: Commit**

```bash
git add backend/migrations/20260318000004_add_share_revocation.sql backend/migrations/20260318000005_create_share_access_log.sql
git commit -m "feat(migrations): add share revocation and access logging

Add Phase 3B database schema:
- shares.revoked_at for soft delete
- shares.last_accessed_at for usage tracking
- share_access_log table for audit trail
- Indexes for efficient queries"
```

---

## Task 2: Add Share Event Types

**Files:**
- Modify: `backend/crates/core/src/events/types.rs`

- [ ] **Step 1: Write test for new event types**

Add to types.rs tests:
```rust
#[test]
fn test_share_event_type_serialization() {
    let event_type = EventType::ShareCreated;
    let json = serde_json::to_string(&event_type).unwrap();
    assert_eq!(json, r#"{"type":"ShareCreated"}"#);

    let event_type = EventType::ShareRevoked;
    let json = serde_json::to_string(&event_type).unwrap();
    assert_eq!(json, r#"{"type":"ShareRevoked"}"#);

    let event_type = EventType::ShareUpdated;
    let json = serde_json::to_string(&event_type).unwrap();
    assert_eq!(json, r#"{"type":"ShareUpdated"}"#);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend/crates/core && cargo test test_share_event_type_serialization`
Expected: FAIL with "no variant `ShareCreated` found"

- [ ] **Step 3: Add ShareCreated, ShareRevoked, ShareUpdated variants**

Add to EventType enum:
```rust
// Share events
ShareCreated,
ShareRevoked,
ShareUpdated,
```

- [ ] **Step 4: Add type_name() matches**

Add to type_name() method:
```rust
EventType::ShareCreated => "ShareCreated",
EventType::ShareRevoked => "ShareRevoked",
EventType::ShareUpdated => "ShareUpdated",
```

- [ ] **Step 5: Add event payloads**

Add after existing payloads:
```rust
/// Share created event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareCreatedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub share_token: String,
    pub permissions: SharePermissions,
    pub password_protected: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: UserId,
}

/// Share revoked event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRevokedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub revoked_by: UserId,
}

/// Share updated event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareUpdatedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub password_changed: bool,
    pub expires_at_changed: bool,
    pub new_expires_at: Option<DateTime<Utc>>,
    pub updated_by: UserId,
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cd backend/crates/core && cargo test test_share_event_type_serialization`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add backend/crates/core/src/events/types.rs
git commit -m "feat(events): add share event types

Add ShareCreated, ShareRevoked, ShareUpdated events:
- Event variants in EventType enum
- type_name() mapping for WebSocket notifications
- Event payload structs with all required fields

Includes serialization test."
```

---

## Task 3: Add ShareError Types

**Files:**
- Create: `backend/crates/core/src/services/share_errors.rs`
- Modify: `backend/crates/core/src/services/mod.rs`

- [ ] **Step 1: Write test for ShareError Display**

Create share_errors.rs:
```rust
use std::fmt;
use uuid::Uuid;

#[derive(Debug)]
pub enum ShareError {
    NotFound,
    NotFoundById(Uuid),
    FileNotFound(Uuid),
    PermissionDenied { file_id: Uuid, user_id: Uuid },
    Revoked,
    Expired,
    PasswordRequired,
    InvalidPassword,
    Database(sqlx::Error),
    PasswordHash(String),
    Jwt(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_error_display() {
        let err = ShareError::NotFound;
        assert_eq!(err.to_string(), "Share not found");

        let err = ShareError::Revoked;
        assert_eq!(err.to_string(), "Share has been revoked");

        let err = ShareError::PasswordRequired;
        assert_eq!(err.to_string(), "Password required for this share");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend/crates/core && cargo test test_share_error_display`
Expected: FAIL with "no method named `to_string`"

- [ ] **Step 3: Implement Display trait**

Add to share_errors.rs:
```rust
impl fmt::Display for ShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShareError::NotFound => write!(f, "Share not found"),
            ShareError::NotFoundById(id) => write!(f, "Share {} not found", id),
            ShareError::FileNotFound(id) => write!(f, "File {} not found", id),
            ShareError::PermissionDenied { file_id, user_id } => {
                write!(f, "User {} does not have permission to manage shares for file {}", user_id, file_id)
            }
            ShareError::Revoked => write!(f, "Share has been revoked"),
            ShareError::Expired => write!(f, "Share has expired"),
            ShareError::PasswordRequired => write!(f, "Password required for this share"),
            ShareError::InvalidPassword => write!(f, "Invalid password"),
            ShareError::Database(err) => write!(f, "Database error: {}", err),
            ShareError::PasswordHash(msg) => write!(f, "Password hashing error: {}", msg),
            ShareError::Jwt(msg) => write!(f, "JWT error: {}", msg),
        }
    }
}

impl std::error::Error for ShareError {}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd backend/crates/core && cargo test test_share_error_display`
Expected: PASS

- [ ] **Step 5: Export from services/mod.rs**

Add to services/mod.rs:
```rust
mod share_errors;
pub use share_errors::*;
```

- [ ] **Step 6: Commit**

```bash
git add backend/crates/core/src/services/share_errors.rs backend/crates/core/src/services/mod.rs
git commit -m "feat(services): add ShareError types

Define ShareError enum for share operations:
- NotFound, Revoked, Expired variants
- PasswordRequired, InvalidPassword variants
- Permission and database error variants
- Display trait implementation

Includes Display test."
```

---

## Task 4: Add JwtManager Extensions for ShareSessionClaims

**Files:**
- Create: `backend/crates/auth/src/session.rs`
- Modify: `backend/crates/auth/src/jwt_manager.rs`
- Modify: `backend/crates/auth/src/lib.rs`

- [ ] **Step 1: Write test for ShareSessionClaims**

Create session.rs:
```rust
use chrono::{DateTime, Duration, Utc};
use rustshare_core::domain::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Share session claims for JWT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareSessionClaims {
    pub sub: String, // Format: "share:{share_id}"
    pub share_id: ShareId,
    pub file_id: FileId,
    pub permissions: SharePermissions,
    pub iat: i64,
    pub exp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_session_claims_creation() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let claims = ShareSessionClaims::new(share_id, file_id, SharePermissions::Read, 3600);

        assert_eq!(claims.sub, format!("share:{}", share_id));
        assert_eq!(claims.share_id, share_id);
        assert_eq!(claims.file_id, file_id);
        assert_eq!(claims.permissions, SharePermissions::Read);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_share_session_claims_expiration() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let claims = ShareSessionClaims::new(share_id, file_id, SharePermissions::Read, -1);

        assert!(claims.is_expired());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend/crates/auth && cargo test test_share_session_claims_creation`
Expected: FAIL with "no method named `new`"

- [ ] **Step 3: Implement ShareSessionClaims**

Add to session.rs:
```rust
impl ShareSessionClaims {
    /// Create new share session claims
    pub fn new(
        share_id: ShareId,
        file_id: FileId,
        permissions: SharePermissions,
        ttl_seconds: i64,
    ) -> Self {
        let now = Utc::now();
        let exp = now + Duration::seconds(ttl_seconds);

        Self {
            sub: format!("share:{}", share_id),
            share_id,
            file_id,
            permissions,
            iat: now.timestamp(),
            exp: exp.timestamp(),
        }
    }

    /// Check if claims are expired
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }
}
```

- [ ] **Step 4: Write test for JwtManager encode_custom_claims**

Add to jwt_manager.rs tests:
```rust
#[test]
fn test_encode_decode_custom_claims() {
    use crate::session::ShareSessionClaims;

    let manager = JwtManager::new("test_secret".to_string());
    let share_id = uuid::Uuid::new_v4();
    let file_id = uuid::Uuid::new_v4();

    let claims = ShareSessionClaims::new(
        share_id,
        file_id,
        rustshare_core::domain::SharePermissions::Read,
        3600,
    );

    let token = manager.encode_custom_claims(&claims).unwrap();
    let decoded: ShareSessionClaims = manager.decode_custom(&token).unwrap();

    assert_eq!(decoded.share_id, share_id);
    assert_eq!(decoded.file_id, file_id);
}
```

- [ ] **Step 5: Run test to verify it fails**

Run: `cd backend/crates/auth && cargo test test_encode_decode_custom_claims`
Expected: FAIL with "no method named `encode_custom_claims`"

- [ ] **Step 6: Implement JwtManager extensions**

Add to jwt_manager.rs:
```rust
use serde::de::DeserializeOwned;

impl JwtManager {
    /// Encode custom claims to JWT
    pub fn encode_custom_claims<T: Serialize>(&self, claims: &T) -> Result<String> {
        let token = encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| anyhow::anyhow!("Failed to encode token: {}", e))?;

        Ok(token)
    }

    /// Decode custom claims from JWT
    pub fn decode_custom<T: DeserializeOwned>(&self, token: &str) -> Result<T> {
        let token_data = decode::<T>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to decode token: {}", e))?;

        Ok(token_data.claims)
    }
}
```

- [ ] **Step 7: Export session module from lib.rs**

Add to auth/src/lib.rs:
```rust
pub mod session;
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cd backend/crates/auth && cargo test`
Expected: All tests PASS

- [ ] **Step 9: Commit**

```bash
git add backend/crates/auth/src/session.rs backend/crates/auth/src/jwt_manager.rs backend/crates/auth/src/lib.rs
git commit -m "feat(auth): add ShareSessionClaims and JwtManager extensions

Implement session tokens for share viewers:
- ShareSessionClaims struct with sub format 'share:{id}'
- new() constructor with TTL support
- is_expired() validation method
- JwtManager.encode_custom_claims() for generic JWT encoding
- JwtManager.decode_custom() for generic JWT decoding

Includes tests for claims creation and JWT encoding."
```

---

## Task 5: Add MetadataStore Share CRUD Methods

**Files:**
- Modify: `backend/crates/storage/src/metadata_store.rs`

- [ ] **Step 1: Write test for create_share and get_share_by_token**

Add to metadata_store.rs tests:
```rust
#[sqlx::test]
async fn test_create_and_get_share(pool: PgPool) {
    let store = MetadataStore::new(pool);

    // Setup: create user and file
    let user_id = Uuid::new_v4();
    let file = File::new(
        "test.txt".to_string(),
        "/test.txt".to_string(),
        1024,
        "hash".to_string(),
        "key".to_string(),
        "text/plain".to_string(),
        user_id,
        None,
    );
    store.create_file(&file).await.unwrap();

    // Create share
    let share = Share::new(
        file.id,
        "abcd1234efgh5678".to_string(),
        user_id,
        SharePermissions::Read,
        None,
        None,
    );
    store.create_share(&share).await.unwrap();

    // Retrieve by token
    let retrieved = store
        .get_share_by_token(&share.share_token)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(retrieved.id, share.id);
    assert_eq!(retrieved.share_token, share.share_token);
    assert_eq!(retrieved.permissions, SharePermissions::Read);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend/crates/storage && cargo test test_create_and_get_share -- --ignored`
Expected: FAIL with "no method named `create_share`"

- [ ] **Step 3: Implement create_share**

Add to MetadataStore impl:
```rust
/// Create new share link
pub async fn create_share(&self, share: &Share) -> Result<()> {
    sqlx::query(
        "INSERT INTO shares (id, file_id, share_token, created_by, permissions, password_hash, expires_at, access_count, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
    )
    .bind(share.id)
    .bind(share.file_id)
    .bind(&share.share_token)
    .bind(share.created_by)
    .bind(&share.permissions)
    .bind(&share.password_hash)
    .bind(share.expires_at)
    .bind(share.access_count)
    .bind(share.created_at)
    .bind(share.updated_at)
    .execute(&self.pool)
    .await?;

    Ok(())
}
```

- [ ] **Step 4: Implement get_share_by_token**

```rust
/// Get share by token
pub async fn get_share_by_token(&self, token: &str) -> Result<Option<Share>> {
    let share = sqlx::query_as::<_, Share>(
        "SELECT * FROM shares WHERE share_token = $1"
    )
    .bind(token)
    .fetch_optional(&self.pool)
    .await?;

    Ok(share)
}
```

- [ ] **Step 5: Implement remaining CRUD methods**

```rust
/// Get share by ID
pub async fn get_share(&self, share_id: ShareId) -> Result<Option<Share>> {
    let share = sqlx::query_as::<_, Share>(
        "SELECT * FROM shares WHERE id = $1"
    )
    .bind(share_id)
    .fetch_optional(&self.pool)
    .await?;

    Ok(share)
}

/// Get all shares for a file (active only)
pub async fn get_file_shares(&self, file_id: FileId) -> Result<Vec<Share>> {
    let shares = sqlx::query_as::<_, Share>(
        "SELECT * FROM shares WHERE file_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC"
    )
    .bind(file_id)
    .fetch_all(&self.pool)
    .await?;

    Ok(shares)
}

/// Update share settings
pub async fn update_share(&self, share: &Share) -> Result<()> {
    sqlx::query(
        "UPDATE shares
         SET password_hash = $1, expires_at = $2, updated_at = NOW()
         WHERE id = $3"
    )
    .bind(&share.password_hash)
    .bind(share.expires_at)
    .bind(share.id)
    .execute(&self.pool)
    .await?;

    Ok(())
}

/// Revoke share (soft delete)
pub async fn revoke_share(&self, share_id: ShareId) -> Result<()> {
    sqlx::query(
        "UPDATE shares SET revoked_at = NOW() WHERE id = $1"
    )
    .bind(share_id)
    .execute(&self.pool)
    .await?;

    Ok(())
}

/// Increment share access count
pub async fn increment_share_access(&self, share_id: ShareId) -> Result<()> {
    sqlx::query(
        "UPDATE shares
         SET access_count = access_count + 1, last_accessed_at = NOW()
         WHERE id = $1"
    )
    .bind(share_id)
    .execute(&self.pool)
    .await?;

    Ok(())
}

/// Log share access attempt
pub async fn log_share_access(
    &self,
    share_id: ShareId,
    ip_address: Option<String>,
    user_agent: Option<String>,
    action: String,
    success: bool,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO share_access_log (share_id, ip_address, user_agent, action, success)
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(share_id)
    .bind(ip_address.and_then(|ip| ip.parse::<std::net::IpAddr>().ok()))
    .bind(user_agent)
    .bind(action)
    .bind(success)
    .execute(&self.pool)
    .await?;

    Ok(())
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cd backend/crates/storage && cargo test test_create_and_get_share -- --ignored`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add backend/crates/storage/src/metadata_store.rs
git commit -m "feat(storage): add MetadataStore methods for share CRUD

Add share database operations:
- create_share: insert new share link
- get_share_by_token: retrieve by token for validation
- get_share: retrieve by ID
- get_file_shares: list all active shares for a file
- update_share: modify password/expiration
- revoke_share: soft delete
- increment_share_access: track usage
- log_share_access: audit trail

All methods follow existing MetadataStore patterns."
```

---

## Task 6: Create ShareService with generate_token and create_share

**Files:**
- Create: `backend/crates/core/src/services/share_service.rs`
- Modify: `backend/crates/core/src/services/mod.rs`

- [ ] **Step 1: Write test for generate_token uniqueness**

Create share_service.rs:
```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use rustshare_auth::{JwtManager, PasswordHasher};
use rustshare_core::domain::*;
use rustshare_core::events::*;
use rustshare_core::services::*;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

pub struct ShareService<E, M>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
{
    event_store: Arc<E>,
    metadata_store: Arc<M>,
    jwt_manager: Arc<JwtManager>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token_is_unique() {
        let mut tokens = HashSet::new();

        for _ in 0..1000 {
            let token = ShareService::<MockEventStore, MockMetadataStore>::generate_token();
            assert_eq!(token.len(), 32);
            assert!(token.chars().all(|c| c.is_alphanumeric()));
            assert!(tokens.insert(token), "Generated duplicate token");
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend/crates/core && cargo test test_generate_token_is_unique`
Expected: FAIL with "no function named `generate_token`"

- [ ] **Step 3: Implement generate_token**

Add before tests:
```rust
impl<E, M> ShareService<E, M>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
{
    pub fn new(
        event_store: Arc<E>,
        metadata_store: Arc<M>,
        jwt_manager: Arc<JwtManager>,
    ) -> Self {
        Self {
            event_store,
            metadata_store,
            jwt_manager,
        }
    }

    /// Generate cryptographically secure 32-character share token
    fn generate_token() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        const TOKEN_LENGTH: usize = 32;

        let mut rng = rand::thread_rng();
        (0..TOKEN_LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
}
```

- [ ] **Step 4: Write test for create_share**

Add to tests:
```rust
struct MockEventStore;
impl EventStoreOps for MockEventStore {
    async fn append(&self, _event: &Event, _broadcaster: &EventBroadcaster) -> Result<()> {
        Ok(())
    }
}

struct MockMetadataStore {
    files: std::sync::Mutex<Vec<File>>,
    shares: std::sync::Mutex<Vec<Share>>,
}

impl MetadataStoreOps for MockMetadataStore {
    async fn get_file(&self, file_id: FileId) -> Result<Option<File>> {
        Ok(self.files.lock().unwrap().iter().find(|f| f.id == file_id).cloned())
    }

    async fn create_share(&self, share: &Share) -> Result<()> {
        self.shares.lock().unwrap().push(share.clone());
        Ok(())
    }

    // Stub other required methods...
}

#[tokio::test]
async fn test_create_share_success() {
    let user_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    let file = File::new(
        "test.txt".to_string(),
        "/test.txt".to_string(),
        1024,
        "hash".to_string(),
        "key".to_string(),
        "text/plain".to_string(),
        user_id,
        None,
    );

    let event_store = Arc::new(MockEventStore);
    let metadata_store = Arc::new(MockMetadataStore {
        files: std::sync::Mutex::new(vec![file]),
        shares: std::sync::Mutex::new(Vec::new()),
    });
    let jwt_manager = Arc::new(JwtManager::new("test_secret".to_string()));

    let service = ShareService::new(event_store, metadata_store, jwt_manager);

    let share = service
        .create_share(
            file_id,
            user_id,
            SharePermissions::Read,
            None,
            None,
        )
        .await
        .unwrap();

    assert!(!share.share_token.is_empty());
    assert_eq!(share.file_id, file_id);
    assert_eq!(share.created_by, user_id);
    assert_eq!(share.permissions, SharePermissions::Read);
}

#[tokio::test]
async fn test_create_share_permission_denied() {
    let owner_id = Uuid::new_v4();
    let other_user_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    let file = File::new(
        "test.txt".to_string(),
        "/test.txt".to_string(),
        1024,
        "hash".to_string(),
        "key".to_string(),
        "text/plain".to_string(),
        owner_id,
        None,
    );

    let event_store = Arc::new(MockEventStore);
    let metadata_store = Arc::new(MockMetadataStore {
        files: std::sync::Mutex::new(vec![file]),
        shares: std::sync::Mutex::new(Vec::new()),
    });
    let jwt_manager = Arc::new(JwtManager::new("test_secret".to_string()));

    let service = ShareService::new(event_store, metadata_store, jwt_manager);

    let result = service
        .create_share(
            file_id,
            other_user_id,
            SharePermissions::Read,
            None,
            None,
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ShareError::PermissionDenied { .. }));
}
```

- [ ] **Step 5: Run tests to verify they fail**

Run: `cd backend/crates/core && cargo test test_create_share`
Expected: FAIL with "no method named `create_share`"

- [ ] **Step 6: Implement create_share**

Add to ShareService impl:
```rust
/// Create new share link for a file
pub async fn create_share(
    &self,
    file_id: FileId,
    created_by: UserId,
    permissions: SharePermissions,
    password: Option<String>,
    expires_at: Option<DateTime<Utc>>,
) -> Result<Share, ShareError> {
    // Verify file exists
    let file = self
        .metadata_store
        .get_file(file_id)
        .await
        .map_err(|e| ShareError::Database(e))?
        .ok_or(ShareError::FileNotFound(file_id))?;

    // Verify user owns file
    if file.owner_id != created_by {
        return Err(ShareError::PermissionDenied {
            file_id,
            user_id: created_by,
        });
    }

    // Hash password if provided
    let password_hash = if let Some(pwd) = password {
        Some(PasswordHasher::hash(&pwd).map_err(|e| ShareError::PasswordHash(e.to_string()))?)
    } else {
        None
    };

    // Generate unique token
    let share_token = Self::generate_token();

    // Create share
    let share = Share::new(
        file_id,
        share_token.clone(),
        created_by,
        permissions,
        password_hash.clone(),
        expires_at,
    );

    // Store share
    self.metadata_store
        .create_share(&share)
        .await
        .map_err(|e| ShareError::Database(e))?;

    // Emit ShareCreated event
    let event = Event::new(
        EventType::ShareCreated,
        share.id,
        AggregateType::Share,
        serde_json::to_value(&ShareCreatedPayload {
            share_id: share.id,
            file_id,
            share_token: share_token.clone(),
            permissions,
            password_protected: password_hash.is_some(),
            expires_at,
            created_by,
        })
        .unwrap(),
        created_by,
    );

    self.event_store
        .append(&event, &EventBroadcaster::new(100))
        .await
        .map_err(|e| ShareError::Database(sqlx::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))))?;

    Ok(share)
}
```

- [ ] **Step 7: Export ShareService in mod.rs**

Add to services/mod.rs:
```rust
mod share_service;
pub use share_service::*;
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cd backend/crates/core && cargo test`
Expected: All ShareService tests PASS

- [ ] **Step 9: Commit**

```bash
git add backend/crates/core/src/services/share_service.rs backend/crates/core/src/services/mod.rs
git commit -m "feat(services): add ShareService with create_share

Implement ShareService for share link management:
- create_share: generate token, validate ownership, store share
- generate_token: cryptographically secure 32-char alphanumeric
- Password hashing with PasswordHasher
- Emit ShareCreated event
- File ownership verification

Includes unit tests with mock stores."
```

---

## Task 7: Add validate_and_create_session Method

**Files:**
- Modify: `backend/crates/core/src/services/share_service.rs`

- [ ] **Step 1: Define ShareSession struct**

Add at top of share_service.rs after imports:
```rust
/// Share session response with token
#[derive(Debug, Clone)]
pub struct ShareSession {
    pub token: String,
    pub share_id: ShareId,
    pub file_id: FileId,
    pub permissions: SharePermissions,
    pub expires_at: DateTime<Utc>,
}
```

- [ ] **Step 2: Write test for validate_and_create_session**

Add to tests section:
```rust
#[tokio::test]
async fn test_validate_share_creates_session() {
    let user_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    let file = File::new(
        "test.txt".to_string(),
        "/test.txt".to_string(),
        1024,
        "hash".to_string(),
        "key".to_string(),
        "text/plain".to_string(),
        user_id,
        None,
    );

    let event_store = Arc::new(MockEventStore);
    let metadata_store = Arc::new(MockMetadataStore {
        files: std::sync::Mutex::new(vec![file]),
        shares: std::sync::Mutex::new(Vec::new()),
    });
    let jwt_manager = Arc::new(JwtManager::new("test_secret".to_string()));

    let service = ShareService::new(event_store, metadata_store.clone(), jwt_manager);

    // Create share first
    let share = service
        .create_share(file_id, user_id, SharePermissions::Read, None, None)
        .await
        .unwrap();

    // Validate and get session
    let session = service
        .validate_and_create_session(share.share_token.clone(), None)
        .await
        .unwrap();

    assert!(!session.token.is_empty());
    assert_eq!(session.file_id, file_id);
    assert_eq!(session.permissions, SharePermissions::Read);
}

#[tokio::test]
async fn test_validate_share_requires_password() {
    let user_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    let file = File::new(
        "test.txt".to_string(),
        "/test.txt".to_string(),
        1024,
        "hash".to_string(),
        "key".to_string(),
        "text/plain".to_string(),
        user_id,
        None,
    );

    let event_store = Arc::new(MockEventStore);
    let metadata_store = Arc::new(MockMetadataStore {
        files: std::sync::Mutex::new(vec![file]),
        shares: std::sync::Mutex::new(Vec::new()),
    });
    let jwt_manager = Arc::new(JwtManager::new("test_secret".to_string()));

    let service = ShareService::new(event_store, metadata_store.clone(), jwt_manager);

    // Create password-protected share
    let share = service
        .create_share(
            file_id,
            user_id,
            SharePermissions::Read,
            Some("password123".to_string()),
            None,
        )
        .await
        .unwrap();

    // Attempt without password should fail
    let result = service
        .validate_and_create_session(share.share_token.clone(), None)
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ShareError::PasswordRequired));

    // Attempt with wrong password should fail
    let result = service
        .validate_and_create_session(share.share_token.clone(), Some("wrong".to_string()))
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ShareError::InvalidPassword));

    // Attempt with correct password should succeed
    let session = service
        .validate_and_create_session(share.share_token.clone(), Some("password123".to_string()))
        .await
        .unwrap();

    assert!(!session.token.is_empty());
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd backend/crates/core && cargo test test_validate_share`
Expected: FAIL with "no method named `validate_and_create_session`"

- [ ] **Step 4: Implement validate_and_create_session**

Add to ShareService impl:
```rust
/// Validate share access and create session token
pub async fn validate_and_create_session(
    &self,
    share_token: String,
    password: Option<String>,
) -> Result<ShareSession, ShareError> {
    // Get share by token
    let share = self
        .metadata_store
        .get_share_by_token(&share_token)
        .await
        .map_err(|e| ShareError::Database(e))?
        .ok_or(ShareError::NotFound)?;

    // Check if revoked
    if share.revoked_at.is_some() {
        return Err(ShareError::Revoked);
    }

    // Check if expired
    if share.is_expired() {
        return Err(ShareError::Expired);
    }

    // Validate password if required
    if let Some(password_hash) = &share.password_hash {
        let provided_password = password.ok_or(ShareError::PasswordRequired)?;

        let is_valid = PasswordHasher::verify(&provided_password, password_hash)
            .map_err(|e| ShareError::PasswordHash(e.to_string()))?;

        if !is_valid {
            return Err(ShareError::InvalidPassword);
        }
    }

    // Update access count
    self.metadata_store
        .increment_share_access(share.id)
        .await
        .map_err(|e| ShareError::Database(e))?;

    // Create session token (1 hour TTL)
    let ttl_seconds = std::env::var("SHARE_SESSION_TTL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600);

    let claims = rustshare_auth::session::ShareSessionClaims::new(
        share.id,
        share.file_id,
        share.permissions,
        ttl_seconds,
    );

    let token = self
        .jwt_manager
        .encode_custom_claims(&claims)
        .map_err(|e| ShareError::Jwt(e.to_string()))?;

    Ok(ShareSession {
        token,
        share_id: share.id,
        file_id: share.file_id,
        permissions: share.permissions,
        expires_at: DateTime::from_timestamp(claims.exp, 0)
            .unwrap_or_else(|| Utc::now() + chrono::Duration::seconds(ttl_seconds)),
    })
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend/crates/core && cargo test test_validate_share`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add backend/crates/core/src/services/share_service.rs
git commit -m "feat(services): add validate_and_create_session method

Validate share access and issue session tokens:
- Check share exists and not revoked/expired
- Validate password if required (Argon2)
- Increment access count
- Generate JWT session token (1 hour TTL)
- Return ShareSession with token and metadata

Includes tests for password validation flow."
```

---

## Task 8: Add revoke_share, update_share, list_file_shares Methods

**Files:**
- Modify: `backend/crates/core/src/services/share_service.rs`

- [ ] **Step 1: Write tests**

Add to tests:
```rust
#[tokio::test]
async fn test_revoke_share() {
    let user_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    let file = File::new(
        "test.txt".to_string(),
        "/test.txt".to_string(),
        1024,
        "hash".to_string(),
        "key".to_string(),
        "text/plain".to_string(),
        user_id,
        None,
    );

    let event_store = Arc::new(MockEventStore);
    let metadata_store = Arc::new(MockMetadataStore {
        files: std::sync::Mutex::new(vec![file]),
        shares: std::sync::Mutex::new(Vec::new()),
    });
    let jwt_manager = Arc::new(JwtManager::new("test_secret".to_string()));

    let service = ShareService::new(event_store, metadata_store.clone(), jwt_manager);

    let share = service
        .create_share(file_id, user_id, SharePermissions::Read, None, None)
        .await
        .unwrap();

    // Revoke share
    service
        .revoke_share(share.id, user_id)
        .await
        .unwrap();

    // Should fail to validate revoked share
    let result = service
        .validate_and_create_session(share.share_token, None)
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ShareError::Revoked));
}

#[tokio::test]
async fn test_update_share_password() {
    let user_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    let file = File::new(
        "test.txt".to_string(),
        "/test.txt".to_string(),
        1024,
        "hash".to_string(),
        "key".to_string(),
        "text/plain".to_string(),
        user_id,
        None,
    );

    let event_store = Arc::new(MockEventStore);
    let metadata_store = Arc::new(MockMetadataStore {
        files: std::sync::Mutex::new(vec![file]),
        shares: std::sync::Mutex::new(Vec::new()),
    });
    let jwt_manager = Arc::new(JwtManager::new("test_secret".to_string()));

    let service = ShareService::new(event_store, metadata_store.clone(), jwt_manager);

    let share = service
        .create_share(file_id, user_id, SharePermissions::Read, None, None)
        .await
        .unwrap();

    // Update password
    let updated = service
        .update_share(share.id, user_id, Some("newpassword".to_string()), None)
        .await
        .unwrap();

    assert!(updated.is_password_protected());

    // Old access (no password) should fail
    let result = service
        .validate_and_create_session(share.share_token.clone(), None)
        .await;
    assert!(result.is_err());

    // New password should work
    let result = service
        .validate_and_create_session(share.share_token, Some("newpassword".to_string()))
        .await;
    assert!(result.is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend/crates/core && cargo test test_revoke_share test_update_share_password`
Expected: FAIL with "no method named `revoke_share`"

- [ ] **Step 3: Implement revoke_share**

Add to ShareService impl:
```rust
/// Revoke share link (soft delete)
pub async fn revoke_share(
    &self,
    share_id: ShareId,
    user_id: UserId,
) -> Result<(), ShareError> {
    // Get share to verify ownership
    let share = self
        .metadata_store
        .get_share(share_id)
        .await
        .map_err(|e| ShareError::Database(e))?
        .ok_or(ShareError::NotFoundById(share_id))?;

    // Get file to verify ownership
    let file = self
        .metadata_store
        .get_file(share.file_id)
        .await
        .map_err(|e| ShareError::Database(e))?
        .ok_or(ShareError::FileNotFound(share.file_id))?;

    if file.owner_id != user_id {
        return Err(ShareError::PermissionDenied {
            file_id: share.file_id,
            user_id,
        });
    }

    // Revoke share
    self.metadata_store
        .revoke_share(share_id)
        .await
        .map_err(|e| ShareError::Database(e))?;

    // Emit ShareRevoked event
    let event = Event::new(
        EventType::ShareRevoked,
        share_id,
        AggregateType::Share,
        serde_json::to_value(&ShareRevokedPayload {
            share_id,
            file_id: share.file_id,
            revoked_by: user_id,
        })
        .unwrap(),
        user_id,
    );

    self.event_store
        .append(&event, &EventBroadcaster::new(100))
        .await
        .map_err(|e| ShareError::Database(sqlx::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))))?;

    Ok(())
}
```

- [ ] **Step 4: Implement update_share**

```rust
/// Update share settings (password, expiry)
pub async fn update_share(
    &self,
    share_id: ShareId,
    user_id: UserId,
    password: Option<String>,
    expires_at: Option<DateTime<Utc>>,
) -> Result<Share, ShareError> {
    // Get share to verify ownership
    let mut share = self
        .metadata_store
        .get_share(share_id)
        .await
        .map_err(|e| ShareError::Database(e))?
        .ok_or(ShareError::NotFoundById(share_id))?;

    // Get file to verify ownership
    let file = self
        .metadata_store
        .get_file(share.file_id)
        .await
        .map_err(|e| ShareError::Database(e))?
        .ok_or(ShareError::FileNotFound(share.file_id))?;

    if file.owner_id != user_id {
        return Err(ShareError::PermissionDenied {
            file_id: share.file_id,
            user_id,
        });
    }

    // Update password if provided
    let password_changed = password.is_some();
    if let Some(pwd) = password {
        share.password_hash = Some(
            PasswordHasher::hash(&pwd).map_err(|e| ShareError::PasswordHash(e.to_string()))?,
        );
    }

    // Update expiration if provided
    let expires_at_changed = expires_at != share.expires_at;
    if expires_at_changed {
        share.expires_at = expires_at;
    }

    // Save changes
    self.metadata_store
        .update_share(&share)
        .await
        .map_err(|e| ShareError::Database(e))?;

    // Emit ShareUpdated event
    let event = Event::new(
        EventType::ShareUpdated,
        share_id,
        AggregateType::Share,
        serde_json::to_value(&ShareUpdatedPayload {
            share_id,
            file_id: share.file_id,
            password_changed,
            expires_at_changed,
            new_expires_at: share.expires_at,
            updated_by: user_id,
        })
        .unwrap(),
        user_id,
    );

    self.event_store
        .append(&event, &EventBroadcaster::new(100))
        .await
        .map_err(|e| ShareError::Database(sqlx::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))))?;

    Ok(share)
}
```

- [ ] **Step 5: Implement list_file_shares**

```rust
/// List all shares for a file (owner only)
pub async fn list_file_shares(
    &self,
    file_id: FileId,
    user_id: UserId,
) -> Result<Vec<Share>, ShareError> {
    // Verify user owns file
    let file = self
        .metadata_store
        .get_file(file_id)
        .await
        .map_err(|e| ShareError::Database(e))?
        .ok_or(ShareError::FileNotFound(file_id))?;

    if file.owner_id != user_id {
        return Err(ShareError::PermissionDenied { file_id, user_id });
    }

    // Get all shares for file
    self.metadata_store
        .get_file_shares(file_id)
        .await
        .map_err(|e| ShareError::Database(e))
}
```

- [ ] **Step 6: Run tests**

Run: `cd backend/crates/core && cargo test`
Expected: All ShareService tests PASS

- [ ] **Step 7: Commit**

```bash
git add backend/crates/core/src/services/share_service.rs
git commit -m "feat(services): add revoke/update/list share methods

Complete ShareService CRUD operations:
- revoke_share: soft delete with ownership check
- update_share: modify password/expiry with events
- list_file_shares: get all shares for file (owner only)

All methods:
- Verify file ownership
- Emit appropriate events
- Include comprehensive tests"
```

---

**Plan complete through Task 8. Tasks 9-25 continue with the same detailed structure covering handlers, WebSocket extensions, routes, rate limiting, background jobs, integration tests, and documentation as detailed earlier in this session.**
