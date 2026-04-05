# Fix Compat Layer Inconsistencies Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all stub implementations and inconsistencies in the MetadataStoreCompat layer that break group shares and notifications.

**Architecture:** Implement proper delegation from compat layer to underlying SQL stores for group membership and user lookup. The compat layer wraps the new MetadataRepository trait but needs to fall back to SQL for operations not yet in the trait.

**Tech Stack:** Rust, sqlx, PostgreSQL, axum

---

## Background

The `MetadataStoreCompat` struct in `backend/crates/storage/src/metadata_v2/compat.rs` bridges the old `MetadataStore` (raw SQL) and new `MetadataRepository` trait. It has stub implementations that return hardcoded values:

1. `is_user_in_group()` always returns `Ok(false)` — **breaks all group shares**
2. `find_user_by_id()` always returns `Ok(None)` — **breaks notifications**
3. Token hash returned as token — **breaks share links**

---

## Task 1: Add SQL Pool Access to Compat Layer

**Files:**
- Modify: `backend/crates/storage/src/metadata_v2/compat.rs:1-50`
- Modify: `backend/crates/storage/src/lib.rs:260-275`

**Step 1: Add pool field to MetadataStoreCompat struct**

```rust
// In backend/crates/storage/src/metadata_v2/compat.rs
pub struct MetadataStoreCompat {
    repo: Arc<dyn MetadataRepository>,
    pool: sqlx::PgPool,  // Add this field
}
```

**Step 2: Update constructor to accept pool**

```rust
impl MetadataStoreCompat {
    pub fn new(repo: Arc<dyn MetadataRepository>, pool: sqlx::PgPool) -> Self {
        Self { repo, pool }
    }
}
```

**Step 3: Update lib.rs instantiation**

```rust
// In backend/crates/storage/src/lib.rs around line 265
async fn is_user_in_group(
    &self,
    user_id: UserId,
    group_id: Uuid,
) -> Result<bool> {
    // Delegate to the compat layer with pool access
    self.is_user_in_group(user_id, group_id).await
}
```

**Step 4: Check compilation**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo check --package rustshare-storage 2>&1 | head -50`

Expected: Compilation errors pointing to all places that construct MetadataStoreCompat

**Step 5: Fix all constructor calls**

Run: `grep -rn "MetadataStoreCompat::new" --include="*.rs"`

Update each call site to pass the pool.

**Step 6: Commit**

```bash
git add backend/crates/storage/src/
git commit -m "refactor: add pool access to MetadataStoreCompat"
```

---

## Task 2: Implement is_user_in_group in Compat Layer

**Files:**
- Modify: `backend/crates/storage/src/metadata_v2/compat.rs:320-327`
- Test: `backend/tests/group_sharing_test.rs` (add test)

**Step 1: Write failing test**

Add to `backend/tests/group_sharing_test.rs`:

```rust
#[tokio::test]
async fn test_compat_layer_group_membership() {
    let (state, _test_db) = setup_test_state().await;
    
    // Create a user and group
    let user_id = create_test_user(&state.db_pool, "test@example.com").await;
    let group_id = create_test_group(&state.db_pool, "Test Group", user_id).await;
    add_user_to_group(&state.db_pool, group_id, user_id).await;
    
    // Test via compat layer
    let metadata_store = state.metadata_store.clone();
    let is_member = metadata_store
        .is_user_in_group(user_id, group_id)
        .await
        .expect("should not fail");
    
    assert!(is_member, "user should be a member of the group via compat layer");
    
    // Test non-member
    let other_user = create_test_user(&state.db_pool, "other@example.com").await;
    let is_other_member = metadata_store
        .is_user_in_group(other_user, group_id)
        .await
        .expect("should not fail");
    
    assert!(!is_other_member, "other user should not be a member");
}
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test test_compat_layer_group_membership --test group_sharing_test 2>&1`

Expected: FAIL — assertion failed, got false when expecting true

**Step 3: Implement is_user_in_group with SQL**

Replace the stub in `backend/crates/storage/src/metadata_v2/compat.rs:320-327`:

```rust
async fn is_user_in_group(&self, user_id: uuid::Uuid, group_id: uuid::Uuid) -> anyhow::Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM group_members
            WHERE group_id = $1 AND user_id = $2
        )
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_one(&self.pool)
    .await?;
    
    Ok(exists)
}
```

**Step 4: Run test to verify it passes**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test test_compat_layer_group_membership --test group_sharing_test 2>&1`

Expected: PASS

**Step 5: Commit**

```bash
git add backend/crates/storage/src/metadata_v2/compat.rs backend/tests/group_sharing_test.rs
git commit -m "fix: implement is_user_in_group in compat layer"
```

---

## Task 3: Implement find_user_by_id in Compat Layer

**Files:**
- Modify: `backend/crates/storage/src/metadata_v2/compat.rs:230-235`

**Step 1: Write failing test**

Add to `backend/tests/group_sharing_test.rs`:

```rust
#[tokio::test]
async fn test_compat_layer_find_user_by_id() {
    let (state, _test_db) = setup_test_state().await;
    
    // Create a user
    let user_id = create_test_user(&state.db_pool, "test@example.com").await;
    
    // Test via compat layer
    use rustshare_core::services::ShareMetadataStoreOps;
    let metadata_store = state.metadata_store.clone();
    let user = metadata_store
        .find_user_by_id(user_id)
        .await
        .expect("should not fail");
    
    assert!(user.is_some(), "should find the user");
    let user = user.unwrap();
    assert_eq!(user.id, user_id);
    assert_eq!(user.email, "test@example.com");
    
    // Test non-existent user
    let non_existent = Uuid::new_v4();
    let not_found = metadata_store
        .find_user_by_id(non_existent)
        .await
        .expect("should not fail");
    
    assert!(not_found.is_none(), "should not find non-existent user");
}
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test test_compat_layer_find_user_by_id --test group_sharing_test 2>&1`

Expected: FAIL — assertion failed, got None when expecting Some

**Step 3: Implement find_user_by_id with SQL**

Replace the stub in `backend/crates/storage/src/metadata_v2/compat.rs:230-235`:

```rust
async fn find_user_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<rustshare_core::domain::User>> {
    let row = sqlx::query_as::<_, rustshare_storage::metadata::UserRow>(
        r#"
        SELECT 
            id, email, display_name, password_hash, 
            tenant_id, is_admin, avatar_path, 
            storage_quota, storage_used, 
            created_at, updated_at, deleted_at
        FROM users 
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(&self.pool)
    .await?;
    
    Ok(row.map(|r| r.into()))
}
```

**Step 4: Check compilation**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo check --package rustshare-storage 2>&1 | head -30`

Expected: May need to add UserRow import or conversion

**Step 5: Add User conversion if needed**

If compilation fails, add a conversion impl or use the existing one from metadata.rs:

```rust
use crate::metadata::UserRow;
```

**Step 6: Run test to verify it passes**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test test_compat_layer_find_user_by_id --test group_sharing_test 2>&1`

Expected: PASS

**Step 7: Commit**

```bash
git add backend/crates/storage/src/metadata_v2/compat.rs backend/tests/group_sharing_test.rs
git commit -m "fix: implement find_user_by_id in compat layer"
```

---

## Task 4: Fix Token Hash/Token Mismatch

**Files:**
- Modify: `backend/crates/storage/src/metadata_v2/compat.rs:505`
- Modify: `backend/crates/storage/src/metadata_v2/schemas.rs:417` (if needed)

**Analysis:**

The issue is at line 505 in compat.rs:
```rust
share_token: doc.token_hash.clone(), // Note: this is the hash, not the original token
```

The `ShareDocument` stores `token_hash: Option<String>` (MD5 hash of token), but the domain `Share` model expects `share_token: Option<String>` (the actual token).

**Options:**
1. Store original token in document store (change schema, migration needed)
2. Accept that token_hash is returned and let consumers handle it
3. Don't return token at all for public shares via compat layer

**Decision:** Option 1 is correct — the document store should store the original token, not the hash. The hash is only for lookups.

**Step 1: Update ShareDocument schema**

In `backend/crates/storage/src/metadata_v2/schemas.rs:417`:

```rust
pub struct ShareDocument {
    // ... other fields ...
    pub token_hash: Option<String>,  // For lookup
    pub share_token: Option<String>, // Original token (new field)
    // ...
}
```

**Step 2: Update share_to_document to store both**

In `backend/crates/storage/src/metadata_v2/compat.rs:471`:

```rust
token_hash: share.share_token.as_ref().map(|t| format!("{:x}", md5::compute(t))),
share_token: share.share_token.clone(), // Store original token
```

**Step 3: Update share_from_document to use share_token**

In `backend/crates/storage/src/metadata_v2/compat.rs:505`:

```rust
share_token: doc.share_token.clone(), // Use original token, not hash
```

**Step 4: Check all ShareDocument construction sites**

Run: `grep -rn "ShareDocument {" --include="*.rs" backend/crates/storage/`

Update any places that construct ShareDocument to include the new field.

**Step 5: Run tests**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test share --test group_sharing_test 2>&1`

Expected: All tests pass

**Step 6: Commit**

```bash
git add backend/crates/storage/src/metadata_v2/
git commit -m "fix: store original token in ShareDocument, not just hash"
```

---

## Task 5: Unify SharePermissions Enums

**Files:**
- Modify: `backend/crates/storage/src/metadata_v2/schemas.rs:364-370`
- Modify: `backend/crates/storage/src/metadata_v2/compat.rs:458-475`
- Modify: `backend/crates/core/src/domain/share.rs:10`

**Analysis:**

Two enums exist:
1. `SharePermissions` in domain — with `sqlx::Type` derive
2. `SharePermission` in schemas — without `sqlx::Type`

**Decision:** Keep the domain enum as source of truth. Remove the schemas enum and use domain enum everywhere.

**Step 1: Remove duplicate enum from schemas.rs**

Delete lines 364-370 in `backend/crates/storage/src/metadata_v2/schemas.rs`:
```rust
// REMOVE THIS:
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SharePermission {
    View,
    Edit,
    Admin,
}
```

**Step 2: Update ShareDocument to use domain enum**

Change line 421 in schemas.rs:
```rust
// FROM:
pub permissions: SharePermission,
// TO:
pub permissions: rustshare_core::domain::SharePermissions,
```

**Step 3: Remove conversion functions from compat.rs**

Delete or simplify lines 458-475 in compat.rs:
```rust
// REMOVE these conversion functions:
fn permission_to_document(_perm: SharePermissions) -> SharePermission { ... }
fn permission_from_document(_perm: SharePermission) -> SharePermissions { ... }
```

**Step 4: Update share_to_document to use enum directly**

In compat.rs line 472:
```rust
// FROM:
permissions: permission_to_document(share.permissions),
// TO:
permissions: share.permissions,
```

**Step 5: Update share_from_document to use enum directly**

In compat.rs line 502:
```rust
// FROM:
permissions: permission_from_document(doc.permissions),
// TO:
permissions: doc.permissions,
```

**Step 6: Check compilation and fix imports**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo check --package rustshare-storage 2>&1`

Fix any missing import errors by adding:
```rust
use rustshare_core::domain::SharePermissions;
```

**Step 7: Run tests**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test share 2>&1 | tail -20`

Expected: All tests pass

**Step 8: Commit**

```bash
git add backend/crates/storage/src/metadata_v2/ backend/crates/core/src/domain/share.rs
git commit -m "refactor: unify SharePermissions enums, remove duplicate"
```

---

## Task 6: Remove ShareType::Invalid Variant

**Files:**
- Modify: `backend/crates/core/src/domain/share.rs:52-58`
- Modify: `backend/crates/core/src/domain/share.rs:90-110` (determine_share_type function)

**Analysis:**

The `ShareType::Invalid` variant represents an error state. This should be handled via `Result` types.

**Step 1: Update determine_share_type to return Result**

In `backend/crates/core/src/domain/share.rs`:

```rust
// Change from:
pub fn determine_share_type(&self) -> ShareType {
    match (self.share_token.is_some(), self.recipient_user_id.is_some(), self.recipient_group_id.is_some()) {
        (true, false, false) => ShareType::Public,
        (false, true, false) => ShareType::User,
        (false, false, true) => ShareType::Group,
        _ => ShareType::Invalid,
    }
}

// To:
pub fn determine_share_type(&self) -> Result<ShareType, ShareTypeError> {
    match (self.share_token.is_some(), self.recipient_user_id.is_some(), self.recipient_group_id.is_some()) {
        (true, false, false) => Ok(ShareType::Public),
        (false, true, false) => Ok(ShareType::User),
        (false, false, true) => Ok(ShareType::Group),
        _ => Err(ShareTypeError::InvalidShareConfiguration),
    }
}
```

**Step 2: Define ShareTypeError**

Add to `backend/crates/core/src/domain/share.rs`:

```rust
#[derive(Debug, Clone, Error)]
pub enum ShareTypeError {
    #[error("Invalid share configuration: exactly one of share_token, recipient_user_id, or recipient_group_id must be set")]
    InvalidShareConfiguration,
}
```

**Step 3: Remove Invalid variant from enum**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareType {
    Public,  // share_token is Some
    User,    // recipient_user_id is Some
    Group,   // recipient_group_id is Some
    // Invalid removed — use Result instead
}
```

**Step 4: Find all usages of ShareType::Invalid**

Run: `grep -rn "ShareType::Invalid" --include="*.rs" backend/`

Update each usage to handle the Result type.

**Step 5: Check compilation**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo check 2>&1 | head -40`

**Step 6: Run tests**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test share 2>&1 | tail -20`

Expected: All tests pass

**Step 7: Commit**

```bash
git add backend/crates/core/src/domain/share.rs
git commit -m "refactor: remove ShareType::Invalid, use Result type"
```

---

## Task 7: Consolidate Error Types

**Files:**
- Modify: `backend/crates/core/src/services/share_errors.rs:8-25`

**Analysis:**

`ShareError` has overlapping variants:
- `NotFound` — generic
- `NotFoundById(Uuid)` — specific
- `FileNotFound(Uuid)` — specific to files

**Step 1: Consolidate NotFound variants**

Replace in `backend/crates/core/src/services/share_errors.rs`:

```rust
// FROM:
#[derive(Debug, Error)]
pub enum ShareError {
    #[error("Share not found")]
    NotFound,
    #[error("Share {0} not found")]
    NotFoundById(Uuid),
    #[error("File {0} not found")]
    FileNotFound(Uuid),
    // ... rest
}

// TO:
#[derive(Debug, Error)]
pub enum ShareError {
    #[error("Share {0} not found")]
    ShareNotFound(Uuid),
    #[error("File {0} not found")]
    FileNotFound(Uuid),
    #[error("Folder {0} not found")]
    FolderNotFound(Uuid),
    // ... rest
}
```

**Step 2: Update all usages**

Run: `grep -rn "ShareError::NotFound" --include="*.rs" backend/ | grep -v "test"`

Replace each with appropriate variant:
- `ShareError::NotFound` → `ShareError::ShareNotFound(id)` (pass the ID)
- `ShareError::NotFoundById(id)` → `ShareError::ShareNotFound(id)`

**Step 3: Check compilation**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo check 2>&1 | head -50`

**Step 4: Run tests**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test 2>&1 | tail -30`

Expected: All tests pass

**Step 5: Commit**

```bash
git add backend/crates/core/src/services/share_errors.rs
git commit -m "refactor: consolidate ShareError NotFound variants"
```

---

## Task 8: Centralize Error-to-HTTP Mapping

**Files:**
- Create: `backend/server/src/error_mapping.rs`
- Modify: `backend/server/src/handlers/groups.rs`
- Modify: `backend/server/src/handlers/user_shares.rs`
- Modify: `backend/server/src/main.rs`

**Step 1: Create centralized error mapping module**

Create `backend/server/src/error_mapping.rs`:

```rust
//! Centralized error-to-HTTP response mapping for share operations.

use axum::{
    http::StatusCode,
    Json,
};
use rustshare_core::services::ShareError;

/// Convert a ShareError to an HTTP response.
pub fn share_error_to_response(err: ShareError) -> (StatusCode, Json<serde_json::Value>) {
    match err {
        ShareError::ShareNotFound(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Share not found" })),
        ),
        ShareError::FileNotFound(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "File not found" })),
        ),
        ShareError::FolderNotFound(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Folder not found" })),
        ),
        ShareError::GroupNotFound(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Group not found" })),
        ),
        ShareError::CrossTenantSharingNotAllowed => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Cross-tenant sharing not allowed" })),
        ),
        ShareError::NotGroupMember(_) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "You must be a group member to share" })),
        ),
        ShareError::GroupShareAlreadyExists => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Group already has access" })),
        ),
        ShareError::InsufficientPermission { .. } => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Admin permission required" })),
        ),
        ShareError::PermissionDenied { .. } => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Permission denied" })),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to process share operation" })),
        ),
    }
}
```

**Step 2: Add module to main.rs**

In `backend/server/src/main.rs`:
```rust
mod error_mapping;
```

**Step 3: Update groups.rs to use centralized mapping**

Replace manual error mapping in `backend/server/src/handlers/groups.rs:277-310`:

```rust
// FROM:
.map_err(|e| {
    tracing::error!("Failed to create group share: {}", e);
    match e {
        ShareError::FileNotFound(_) => (...),
        // ... many lines
    }
})?;

// TO:
.map_err(|e| {
    tracing::error!("Failed to create group share: {}", e);
    crate::error_mapping::share_error_to_response(e)
})?;
```

**Step 4: Update user_shares.rs similarly**

Find and replace manual error mapping in user_shares.rs with centralized function.

**Step 5: Check compilation**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo check --package rustshare-server 2>&1 | head -30`

**Step 6: Run tests**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test 2>&1 | tail -20`

Expected: All tests pass

**Step 7: Commit**

```bash
git add backend/server/src/error_mapping.rs backend/server/src/handlers/groups.rs backend/server/src/handlers/user_shares.rs backend/server/src/main.rs
git commit -m "refactor: centralize share error to HTTP mapping"
```

---

## Task 9: Final Integration Test

**Files:**
- Create: `backend/tests/compat_layer_integration_test.rs`

**Step 1: Create comprehensive integration test**

```rust
//! Integration tests for MetadataStoreCompat layer.
//! 
//! These tests verify that the compat layer properly delegates to SQL
//! for operations not yet in the MetadataRepository trait.

use rustshare_core::services::ShareMetadataStoreOps;

#[tokio::test]
async fn test_group_sharing_works_via_compat_layer() {
    // Setup test state with compat layer
    let (state, _test_db) = setup_test_state().await;
    
    // Create test data
    let owner = create_test_user(&state.db_pool, "owner@example.com").await;
    let member = create_test_user(&state.db_pool, "member@example.com").await;
    let group_id = create_test_group(&state.db_pool, "Test Group", owner).await;
    add_user_to_group(&state.db_pool, group_id, member).await;
    
    let file_id = create_test_file(&state.db_pool, owner).await;
    
    // Create group share via service (which uses compat layer internally)
    let share = state.share_service
        .create_group_share(
            rustshare_core::services::Resource::File(file_id),
            group_id,
            rustshare_core::domain::SharePermissions::View,
            owner,
            state.tenant_id,
        )
        .await
        .expect("should create group share");
    
    // Verify member can access via permission resolver
    let permission = state.permission_resolver
        .resolve_file_permission(member, file_id)
        .await
        .expect("should resolve permission");
    
    assert!(permission.is_some(), "member should have permission via group share");
    assert_eq!(permission.unwrap(), rustshare_core::domain::SharePermissions::View);
}

// Helper functions for test setup
async fn setup_test_state() -> (TestState, TestDb) {
    // ... implementation
}

async fn create_test_user(pool: &PgPool, email: &str) -> Uuid {
    // ... implementation
}

async fn create_test_group(pool: &PgPool, name: &str, owner_id: Uuid) -> Uuid {
    // ... implementation
}

async fn add_user_to_group(pool: &PgPool, group_id: Uuid, user_id: Uuid) {
    // ... implementation
}

async fn create_test_file(pool: &PgPool, owner_id: Uuid) -> Uuid {
    // ... implementation
}
```

**Step 2: Run integration test**

Run: `cd /Users/scolak/Projects/x/rustshare/backend && cargo test --test compat_layer_integration_test 2>&1`

Expected: PASS (after all previous tasks complete)

**Step 3: Commit**

```bash
git add backend/tests/compat_layer_integration_test.rs
git commit -m "test: add compat layer integration test"
```

---

## Task 10: Update Migration Checksum (If Needed)

**Files:**
- Check: `backend/migrations/20260404000002_add_tenant_sharing_config.sql`

**If the migration was modified after being applied in production:**

**Step 1: Document the fix**

Add to `docs/DEPLOYMENT.md`:

```markdown
## Migration Checksum Fix

If migration `20260404000002_add_tenant_sharing_config.sql` fails with checksum mismatch:

```sql
-- Run this on the database before deploying:
DELETE FROM _sqlx_migrations WHERE version = '20260404000002';
```

Then restart the backend.
```

**Step 2: Commit**

```bash
git add docs/DEPLOYMENT.md
git commit -m "docs: document migration checksum fix procedure"
```

---

## Summary

| Task | Issue | Files Changed |
|------|-------|---------------|
| 1 | Add pool access to compat layer | `compat.rs`, `lib.rs` |
| 2 | Implement `is_user_in_group` | `compat.rs`, test file |
| 3 | Implement `find_user_by_id` | `compat.rs`, test file |
| 4 | Fix token hash/token mismatch | `compat.rs`, `schemas.rs` |
| 5 | Unify SharePermissions enums | `schemas.rs`, `compat.rs` |
| 6 | Remove ShareType::Invalid | `share.rs` |
| 7 | Consolidate error types | `share_errors.rs` |
| 8 | Centralize error mapping | `error_mapping.rs`, handlers |
| 9 | Integration test | new test file |
| 10 | Document migration fix | `DEPLOYMENT.md` |

**Estimated effort:** 2-3 hours human, 45-60 minutes CC+gstack

**Risk level:** Medium — touches core sharing infrastructure

**Rollback plan:** Revert commits in reverse order if issues arise.
