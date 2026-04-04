# Fix Group Share Inconsistencies Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all architectural inconsistencies between group share implementation and existing data models to prevent data loss and incorrect behavior.

**Architecture:** Group shares exist in SQL schema but are not properly supported in ShareDocument schema, compat layer, or some queries. Need to extend schema, fix conversions, and align queries.

**Tech Stack:** Rust, sqlx, PostgreSQL, S3-compatible storage

---

## Background

Group sharing was added to the SQL schema with `recipient_group_id` column, but several components were not updated:

1. `ShareDocument` schema (metadata_v2) lacks group share support
2. `share_to_document`/`share_from_document` conversions lose group data
3. `get_public_shares_for_user` incorrectly includes group shares
4. Frontend types don't expose recipient fields

---

## Task 1: Fix SQL Query for Public Shares

**Problem:** `get_public_shares_for_user` returns group shares as public shares because it only checks `recipient_user_id IS NULL`

**Files:**
- Modify: `backend/crates/storage/src/metadata.rs:2003`

**Step 1: Write failing test**

Create test in `backend/crates/storage/src/metadata.rs` (in test module at end of file):

```rust
#[tokio::test]
async fn test_get_public_shares_excludes_group_shares() {
    let (store, pool) = setup_metadata_store().await;
    
    // Create test user and group
    let user_id = Uuid::new_v4();
    let group_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    
    // Create file
    let file = create_test_file(user_id, tenant_id).await;
    store.create_file(&file).await.unwrap();
    
    // Create public share
    let public_share = Share {
        id: Uuid::new_v4(),
        file_id: Some(file.id),
        folder_id: None,
        share_token: Some("public_token".to_string()),
        permissions: SharePermissions::View,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: None,
        recipient_group_id: None,
        created_by: user_id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id,
    };
    store.create_share(&public_share).await.unwrap();
    
    // Create group share (same file)
    let group_share = Share {
        id: Uuid::new_v4(),
        file_id: Some(file.id),
        folder_id: None,
        share_token: None,
        permissions: SharePermissions::Edit,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: None,
        recipient_group_id: Some(group_id),
        created_by: user_id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id,
    };
    store.create_share(&group_share).await.unwrap();
    
    // Query public shares
    let public_shares = store.get_public_shares_for_user(user_id).await.unwrap();
    
    // Should only return 1 (the public share), not 2
    assert_eq!(public_shares.len(), 1);
    assert_eq!(public_shares[0].share.id, public_share.id);
}
```

**Step 2: Run test to verify it fails**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo test -p rustshare-storage test_get_public_shares_excludes_group_shares -- --nocapture
```

Expected: FAIL - test returns 2 shares instead of 1

**Step 3: Fix the query**

In `backend/crates/storage/src/metadata.rs:2003`, change:

```rust
WHERE s.created_by = $1
  AND s.recipient_user_id IS NULL
  AND s.revoked_at IS NULL
```

To:

```rust
WHERE s.created_by = $1
  AND s.recipient_user_id IS NULL
  AND s.recipient_group_id IS NULL
  AND s.revoked_at IS NULL
```

**Step 4: Run test to verify it passes**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo test -p rustshare-storage test_get_public_shares_excludes_group_shares -- --nocapture
```

Expected: PASS

**Step 5: Commit**

```bash
git add backend/crates/storage/src/metadata.rs
git commit -m "fix: exclude group shares from get_public_shares_for_user"
```

---

## Task 2: Add Group Variant to ShareScope Enum

**Problem:** `ShareScope` enum only has `Public, User` - missing `Group`

**Files:**
- Modify: `backend/crates/storage/src/metadata_v2/schemas.rs:389-392`

**Step 1: Add Group variant**

Change:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareScope {
    Public,
    User,
}
```

To:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareScope {
    Public,
    User,
    Group,
}
```

**Step 2: Run compile check**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo check -p rustshare-storage
```

Expected: PASS (no new usages yet)

**Step 3: Commit**

```bash
git add backend/crates/storage/src/metadata_v2/schemas.rs
git commit -m "feat: add Group variant to ShareScope enum"
```

---

## Task 3: Add recipient_group_id to ShareDocument Schema

**Problem:** `ShareDocument` struct missing `recipient_group_id` field

**Files:**
- Modify: `backend/crates/storage/src/metadata_v2/schemas.rs:402-437`

**Step 1: Add field to struct**

Add after `recipient_user_id` field (line 418):

```rust
/// Recipient group ID for group shares (None for public/user)
pub recipient_group_id: Option<Uuid>,
```

**Step 2: Update Default impl for ShareScope**

The default is fine as `Public`, but we should document it. No change needed.

**Step 3: Run compile check**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo check -p rustshare-storage
```

Expected: FAIL - need to update usages (constructors, conversions)

**Step 4: Commit (with compile errors is OK for now)**

Actually, let's fix the compile errors first in the next task.

---

## Task 4: Fix share_to_document Conversion

**Problem:** `share_to_document` misclassifies group shares as public

**Files:**
- Modify: `backend/crates/storage/src/metadata_v2/compat.rs:441-481`

**Step 1: Fix scope determination logic**

Change:

```rust
let scope = if share.recipient_user_id.is_some() {
    ShareScope::User
} else {
    ShareScope::Public
};
```

To:

```rust
let scope = if share.recipient_group_id.is_some() {
    ShareScope::Group
} else if share.recipient_user_id.is_some() {
    ShareScope::User
} else {
    ShareScope::Public
};
```

**Step 2: Add recipient_group_id to document construction**

Add to `ShareDocument` construction:

```rust
recipient_user_id: share.recipient_user_id,
recipient_group_id: share.recipient_group_id,  // NEW
```

**Step 3: Run compile check**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo check -p rustshare-storage
```

Expected: PASS (or close to it - share_from_document next)

**Step 4: Commit**

```bash
git add backend/crates/storage/src/metadata_v2/compat.rs
git commit -m "fix: share_to_document handles group shares correctly"
```

---

## Task 5: Fix share_from_document Conversion

**Problem:** `share_from_document` hardcodes `recipient_group_id: None`

**Files:**
- Modify: `backend/crates/storage/src/metadata_v2/compat.rs:483-515`

**Step 1: Fix field mapping**

Change:

```rust
recipient_user_id: doc.recipient_user_id,
recipient_group_id: None, // Group shares not yet supported in ShareDocument schema
```

To:

```rust
recipient_user_id: doc.recipient_user_id,
recipient_group_id: doc.recipient_group_id,
```

**Step 2: Run compile check**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo check -p rustshare-storage
```

Expected: PASS

**Step 3: Run tests**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo test -p rustshare-storage
```

Expected: All tests pass

**Step 4: Commit**

```bash
git add backend/crates/storage/src/metadata_v2/compat.rs
git commit -m "fix: share_from_document preserves recipient_group_id"
```

---

## Task 6: Fix OwnedPublicShare Mapping

**Problem:** `recipient_group_id: None` with outdated comment

**Files:**
- Modify: `backend/crates/storage/src/metadata.rs:2024`

**Step 1: Fix the mapping**

Change:

```rust
recipient_user_id: row.try_get("recipient_user_id")?,
recipient_group_id: None, // Group shares not yet in database schema
```

To:

```rust
recipient_user_id: row.try_get("recipient_user_id")?,
recipient_group_id: row.try_get("recipient_group_id")?,
```

**Step 2: Verify query selects the column**

The query at line 1995-2005 should include `s.recipient_group_id` in SELECT. Check and add if missing.

**Step 3: Run compile check**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo check -p rustshare-storage
```

Expected: PASS

**Step 4: Commit**

```bash
git add backend/crates/storage/src/metadata.rs
git commit -m "fix: include recipient_group_id in OwnedPublicShare mapping"
```

---

## Task 7: Update Frontend Share Type

**Problem:** Frontend `Share` type missing recipient fields

**Files:**
- Modify: `frontend/src/lib/api/types.ts:52-65`

**Step 1: Add recipient fields**

Change:

```typescript
export interface Share {
	id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	resource_name?: string;
	share_token: string;
	permissions: 'View' | 'Edit' | 'Admin';
	upload_only: boolean;
	password_protected: boolean;
	access_count: number;
	expires_at: string | null;
	created_at: string;
	created_by?: string;
}
```

To:

```typescript
export interface Share {
	id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	resource_name?: string;
	share_token: string;
	permissions: 'View' | 'Edit' | 'Admin';
	upload_only: boolean;
	password_protected: boolean;
	access_count: number;
	expires_at: string | null;
	created_at: string;
	created_by?: string;
	// Share type indicators
	recipient_user_id?: string | null;
	recipient_group_id?: string | null;
}
```

**Step 2: Check for TypeScript errors**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend
npm run check
```

Expected: PASS (or any existing errors unrelated to this change)

**Step 3: Commit**

```bash
git add frontend/src/lib/api/types.ts
git commit -m "feat: add recipient fields to Share type"
```

---

## Task 8: Deduplicate Resource Enum

**Problem:** Two identical `Resource` enums exist in different modules

**Files:**
- Modify: `backend/crates/core/src/services/share_service.rs:22-25`
- Modify: `backend/crates/core/src/services/mod.rs:70`
- Modify: `backend/server/src/handlers/groups.rs:259,355`

**Step 1: Remove local Resource enum from share_service.rs**

Delete:

```rust
/// Resource type for share operations
#[derive(Debug, Clone, Copy)]
pub enum Resource {
    File(FileId),
    Folder(FolderId),
}
```

**Step 2: Import from permission_resolver instead**

Add to imports in share_service.rs:

```rust
use crate::services::permission_resolver::Resource;
```

**Step 3: Update mod.rs exports**

Change in `mod.rs:70`:

```rust
Resource as ShareResource, ShareNotificationRepo, ShareService,
```

To:

```rust
ShareNotificationRepo, ShareService,
```

And update line 51 to export Resource once:

```rust
pub use permission_resolver::{
    PermissionResolver, PermissionResolverOps, Resource,  // Keep Resource here
};
```

**Step 4: Update handlers to use Resource instead of ShareResource**

In `groups.rs:259,355`, change:

```rust
use rustshare_core::services::ShareResource;
```

To:

```rust
use rustshare_core::services::Resource;
```

And update usages from `ShareResource::File` to `Resource::File`, etc.

**Step 5: Run compile check**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo check -p rustshare-core -p rustshare-server
```

Expected: PASS

**Step 6: Commit**

```bash
git add backend/crates/core/src/services/share_service.rs
git add backend/crates/core/src/services/mod.rs
git add backend/server/src/handlers/groups.rs
git commit -m "refactor: deduplicate Resource enum"
```

---

## Task 9: Run Full Test Suite

**Step 1: Run all storage tests**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo test -p rustshare-storage
```

Expected: All pass (including new test from Task 1)

**Step 2: Run all core tests**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo test -p rustshare-core
```

Expected: All pass

**Step 3: Run all server tests**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo test -p rustshare-server
```

Expected: All pass

**Step 4: Full build**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo build
```

Expected: SUCCESS

---

## Summary

After completing all tasks:

1. ✅ Group shares are excluded from public share queries
2. ✅ ShareDocument schema supports group shares
3. ✅ Conversions preserve all share data
4. ✅ Frontend can distinguish share types
5. ✅ No duplicate Resource enum

**Test commands for verification:**

```bash
# Backend tests
cd /Users/scolak/Projects/x/rustshare/backend
cargo test

# Frontend type check
cd /Users/scolak/Projects/x/rustshare/frontend
npm run check
```

**Deploy to remote:**

```bash
# On remote server
cd /path/to/rustshare
git pull
docker compose build backend
docker compose up -d backend
```
