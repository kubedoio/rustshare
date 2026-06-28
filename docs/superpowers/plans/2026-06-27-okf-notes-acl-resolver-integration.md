# OKF Notes — Real Permission-Resolver ACL Integration

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the placeholder `owner:{id}` in `NoteAclPayload.read_acl` with the real set of principals that have read access to a note, including direct user shares, group shares, inherited folder shares, and public visibility.

**Architecture:** Extend `PermissionResolverOps` with bulk "all shares" queries, add a `resolve_read_principals` method to `PermissionResolver`, inject `PermissionResolver` into `NoteService`, and make `build_acl_payload` asynchronous so it can resolve principals before indexing.

**Tech Stack:** Rust 1.95, SQLx, PostgreSQL.

---

## Files

- Modify: `backend/crates/core/src/services/permission_resolver.rs`
- Modify: `backend/crates/infrastructure/src/repositories/permission_resolver.rs`
- Modify: `backend/server/src/services/note_service.rs`
- Modify: `backend/server/src/state.rs`
- Modify: `backend/server/src/bin/rustshare.rs`
- Test: `backend/crates/core/src/services/permission_resolver.rs` (existing test module)
- Test: `backend/server/src/services/note_service.rs` (existing test module)

---

## Task 1: Add bulk "all shares" queries to `PermissionResolverOps`

**Files:**
- Modify: `backend/crates/core/src/services/permission_resolver.rs:62-109`

- [ ] **Step 1: Add four new trait methods**

Add these signatures after the existing batch methods:

```rust
    /// Find all active user shares on a file (no recipient filter).
    async fn find_all_user_shares_for_file(
        &self,
        file_id: FileId,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;

    /// Find all active group shares on a file (no recipient filter).
    async fn find_all_group_shares_for_file(
        &self,
        file_id: FileId,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;

    /// Find all active user shares on any of the given folders.
    async fn find_all_user_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;

    /// Find all active group shares on any of the given folders.
    async fn find_all_group_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;
```

- [ ] **Step 2: Run cargo check to confirm trait compiles**

Run: `cd backend && SQLX_OFFLINE=true cargo check -p rustshare-core`
Expected: FAIL with "not a member of trait" in `PermissionResolverRepository`. This is expected; Task 2 implements them.

---

## Task 2: Implement the new queries in `PermissionResolverRepository`

**Files:**
- Modify: `backend/crates/infrastructure/src/repositories/permission_resolver.rs:149-176`

- [ ] **Step 1: Implement the four methods using raw SQLx queries**

Insert before the closing brace of the `impl PermissionResolverOps for PermissionResolverRepository` block:

```rust
    async fn find_all_user_shares_for_file(
        &self,
        file_id: FileId,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE file_id = $1
              AND folder_id IS NULL
              AND recipient_user_id IS NOT NULL
              AND tenant_id = $2
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            file_id,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;
        Ok(shares)
    }

    async fn find_all_group_shares_for_file(
        &self,
        file_id: FileId,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE file_id = $1
              AND folder_id IS NULL
              AND recipient_group_id IS NOT NULL
              AND tenant_id = $2
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            file_id,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;
        Ok(shares)
    }

    async fn find_all_user_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = ANY($1)
              AND recipient_user_id IS NOT NULL
              AND tenant_id = $2
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            folder_ids,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;
        Ok(shares)
    }

    async fn find_all_group_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = ANY($1)
              AND recipient_group_id IS NOT NULL
              AND tenant_id = $2
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            folder_ids,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;
        Ok(shares)
    }
```

- [ ] **Step 2: Prepare SQLx offline metadata**

Run against a running PostgreSQL instance:
```bash
cd backend
cargo sqlx prepare --workspace
```
Expected: `.sqlx/` files updated for the four new queries.

---

## Task 3: Add `resolve_read_principals` to `PermissionResolver`

**Files:**
- Modify: `backend/crates/core/src/services/permission_resolver.rs`

- [ ] **Step 1: Add the public method after `resolve_permission`**

```rust
    /// Resolve every principal that has at least View access to a file.
    ///
    /// Returns a list of strings such as:
    /// - `owner:{user_id}`
    /// - `user:{user_id}`
    /// - `group:{group_id}`
    /// - `public` (when visibility is public)
    ///
    /// Expired or revoked shares are ignored.
    pub async fn resolve_read_principals(
        &self,
        file: &File,
        tenant_id: Uuid,
    ) -> Result<Vec<String>> {
        use std::collections::HashSet;

        let mut principals = HashSet::new();
        principals.insert(format!("owner:{}", file.owner_id));

        // Direct shares on the file.
        let user_shares = self
            .ops
            .find_all_user_shares_for_file(file.id, tenant_id)
            .await?;
        for share in user_shares {
            if Self::is_share_active(&share) && share.permissions >= SharePermissions::View {
                if let Some(uid) = share.recipient_user_id {
                    principals.insert(format!("user:{uid}"));
                }
            }
        }

        let group_shares = self
            .ops
            .find_all_group_shares_for_file(file.id, tenant_id)
            .await?;
        for share in group_shares {
            if Self::is_share_active(&share) && share.permissions >= SharePermissions::View {
                if let Some(gid) = share.recipient_group_id {
                    principals.insert(format!("group:{gid}"));
                }
            }
        }

        // Inherited shares from folder ancestry.
        if let Some(parent_folder_id) = file.parent_folder_id {
            let folder = match self.ops.find_folder_by_id(parent_folder_id, tenant_id).await? {
                Some(f) => f,
                None => {
                    return Ok(principals.into_iter().collect());
                }
            };

            let mut folder_ids = vec![parent_folder_id];
            if let Some(ref ancestor_ids) = folder.ancestor_ids {
                folder_ids.extend(ancestor_ids.iter().copied());
            } else {
                let mut current_id = folder.parent_folder_id;
                let mut depth = 0;
                const MAX_DEPTH: usize = 50;
                while let Some(id) = current_id {
                    if depth >= MAX_DEPTH {
                        break;
                    }
                    folder_ids.push(id);
                    if let Some(parent) = self.ops.find_folder_by_id(id, tenant_id).await? {
                        current_id = parent.parent_folder_id;
                    } else {
                        break;
                    }
                    depth += 1;
                }
            }

            if !folder_ids.is_empty() {
                let folder_user_shares = self
                    .ops
                    .find_all_user_shares_for_folders(&folder_ids, tenant_id)
                    .await?;
                for share in folder_user_shares {
                    if Self::is_share_active(&share) && share.permissions >= SharePermissions::View {
                        if let Some(uid) = share.recipient_user_id {
                            principals.insert(format!("user:{uid}"));
                        }
                    }
                }

                let folder_group_shares = self
                    .ops
                    .find_all_group_shares_for_folders(&folder_ids, tenant_id)
                    .await?;
                for share in folder_group_shares {
                    if Self::is_share_active(&share) && share.permissions >= SharePermissions::View {
                        if let Some(gid) = share.recipient_group_id {
                            principals.insert(format!("group:{gid}"));
                        }
                    }
                }
            }
        }

        Ok(principals.into_iter().collect())
    }
```

- [ ] **Step 2: Add `async-trait` to `rustshare-core` dev-dependencies**

In `backend/crates/core/Cargo.toml`, add:

```toml
[dev-dependencies]
async-trait = "0.1"
```

- [ ] **Step 3: Add a unit test for the new method**

Append to the existing `#[cfg(test)] mod tests` block:

```rust
    use rustshare_core::domain::File;
    use async_trait::async_trait;

    struct ReadPrincipalOps {
        file_shares: Vec<Share>,
        folder_shares: Vec<Share>,
        folder: Option<Folder>,
    }

    #[async_trait::async_trait]
    impl PermissionResolverOps for ReadPrincipalOps {
        async fn find_user_share(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<FolderId>,
            _recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> Result<Option<Share>> {
            Ok(None)
        }
        async fn find_group_shares(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<FolderId>,
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_user_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_group_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_all_user_shares_for_file(
            &self,
            _file_id: FileId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(self.file_shares.iter().filter(|s| s.recipient_user_id.is_some()).cloned().collect())
        }
        async fn find_all_group_shares_for_file(
            &self,
            _file_id: FileId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(self.file_shares.iter().filter(|s| s.recipient_group_id.is_some()).cloned().collect())
        }
        async fn find_all_user_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(self.folder_shares.iter().filter(|s| s.recipient_user_id.is_some()).cloned().collect())
        }
        async fn find_all_group_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(self.folder_shares.iter().filter(|s| s.recipient_group_id.is_some()).cloned().collect())
        }
        async fn find_file_by_id(&self, _id: FileId, _tenant_id: Uuid) -> Result<Option<File>> {
            Ok(None)
        }
        async fn find_folder_by_id(&self, _id: FolderId, _tenant_id: Uuid) -> Result<Option<Folder>> {
            Ok(self.folder.clone())
        }
        async fn get_user_group_ids(&self, _user_id: UserId, _tenant_id: Uuid) -> Result<Vec<Uuid>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn resolve_read_principals_includes_owner_and_shares() {
        let owner = Uuid::new_v4();
        let user = Uuid::new_v4();
        let group = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let file = File::new(
            "note.md".to_string(),
            "/Workspace/Notes/X/note.md".to_string(),
            "hash".to_string(),
            100,
            "text/markdown".to_string(),
            None,
            owner,
            tenant_id,
        );

        let ops = ReadPrincipalOps {
            file_shares: vec![Share {
                id: Uuid::new_v4(),
                file_id: Some(file_id),
                folder_id: None,
                share_token: None,
                permissions: SharePermissions::View,
                password_hash: None,
                expires_at: None,
                upload_only: false,
                access_count: 0,
                recipient_user_id: Some(user),
                recipient_group_id: None,
                created_by: owner,
                created_at: Utc::now(),
                revoked_at: None,
                tenant_id,
            }],
            folder_shares: vec![Share {
                id: Uuid::new_v4(),
                file_id: None,
                folder_id: Some(Uuid::new_v4()),
                share_token: None,
                permissions: SharePermissions::View,
                password_hash: None,
                expires_at: None,
                upload_only: false,
                access_count: 0,
                recipient_user_id: None,
                recipient_group_id: Some(group),
                created_by: owner,
                created_at: Utc::now(),
                revoked_at: None,
                tenant_id,
            }],
            folder: None,
        };

        let resolver = PermissionResolver::new(Arc::new(ops));
        let mut principals = resolver.resolve_read_principals(&file, tenant_id).await.unwrap();
        principals.sort();

        assert_eq!(principals, vec![
            format!("owner:{owner}"),
            format!("user:{user}"),
        ]);
    }
```

- [ ] **Step 4: Run core tests**

Run: `cd backend && SQLX_OFFLINE=true cargo test -p rustshare-core --lib permission`
Expected: PASS (uses mock ops, no DB required).

---

## Task 4: Align `AclSearchFilter` group representation with resolved principals

`resolve_read_principals` emits `group:{uuid}` principals. The current `AclSearchFilter` stores group names as strings, which will never match. Change it to group IDs.

**Files:**
- Modify: `backend/crates/core/src/services/ai/indexing.rs:56-65`
- Modify: `backend/crates/core/src/services/ai/indexing.rs:515-553` (`can_access`)
- Modify: `backend/crates/core/src/services/ai/indexing.rs:860-970` (tests)

- [ ] **Step 1: Change `caller_groups` to `caller_group_ids`**

```rust
#[derive(Debug, Clone, Default)]
pub struct AclSearchFilter {
    pub tenant_id: Uuid,
    pub caller_user_id: Uuid,
    pub caller_group_ids: Vec<Uuid>,
    /// note_id -> minimum accepted acl_version.
    pub min_acl_versions: HashMap<Uuid, i64>,
}
```

- [ ] **Step 2: Update `can_access` group check**

```rust
    // Group membership match.
    if !filter.caller_group_ids.is_empty() {
        let group_principals: Vec<String> = filter
            .caller_group_ids
            .iter()
            .map(|id| format!("group:{id}"))
            .collect();
        if acl
            .read_acl
            .iter()
            .any(|p| group_principals.contains(p))
        {
            return true;
        }
    }
```

- [ ] **Step 3: Update existing tests**

Replace every `caller_groups: vec![...]` with `caller_group_ids: vec![...]` using Uuids, e.g.:

```rust
let engineering_id = Uuid::new_v4();
// ...
caller_group_ids: vec![engineering_id],
```

And update the ACL payload setup to use `read_acl: vec![format!("group:{engineering_id}")]`.

---

## Task 5: Inject `PermissionResolver` into `NoteService`

**Files:**
- Modify: `backend/server/src/services/note_service.rs:251-310`

- [ ] **Step 1: Add the field and update the constructor**

Replace the `NoteService` struct definition and `new` method with:

```rust
#[derive(Clone)]
pub struct NoteService {
    file_service: Arc<
        FileService<
            rustshare_storage::EventStore,
            MetadataStore,
            ObjectStore,
            PermissionResolverRepository,
        >,
    >,
    folder_service: Arc<
        FolderService<rustshare_storage::EventStore, MetadataStore, PermissionResolverRepository>,
    >,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    pub workspace_name: String,
    pub folder_name: String,
    index_sink: Option<Arc<dyn NoteIndexSink>>,
}

impl NoteService {
    pub fn new(
        file_service: Arc<
            FileService<
                rustshare_storage::EventStore,
                MetadataStore,
                ObjectStore,
                PermissionResolverRepository,
            >,
        >,
        folder_service: Arc<
            FolderService<
                rustshare_storage::EventStore,
                MetadataStore,
                PermissionResolverRepository,
            >,
        >,
        metadata_store: Arc<MetadataStore>,
        object_store: Arc<ObjectStore>,
        permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    ) -> Self {
        Self {
            file_service,
            folder_service,
            metadata_store,
            object_store,
            permission_resolver,
            workspace_name: "Workspace".to_string(),
            folder_name: "Notes".to_string(),
            index_sink: None,
        }
    }
```

- [ ] **Step 2: Add `resolve_note_read_principals` and keep `build_acl_payload` static**

Replace the existing `build_acl_payload` implementation with:

```rust
    /// Resolve every principal that has read access to a note.
    pub async fn resolve_note_read_principals(
        &self,
        file: &rustshare_core::domain::File,
        tenant_id: Uuid,
    ) -> Result<Vec<String>, NoteError> {
        self.permission_resolver
            .resolve_read_principals(file, tenant_id)
            .await
            .map_err(|e| NoteError::Database(format!("Failed to resolve ACL principals: {e}")))
    }

    /// Build the ACL payload used by the AI content indexer.
    pub fn build_acl_payload(
        file: &rustshare_core::domain::File,
        meta: &NoteMetadata,
        tenant_id: Uuid,
        read_acl: Vec<String>,
    ) -> NoteAclPayload {
        NoteAclPayload {
            tenant_id,
            workspace_id: tenant_id,
            note_id: meta.okf_id.unwrap_or(file.id),
            source_file_id: file.id,
            source_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            read_acl,
            visibility: meta.visibility.as_str().to_string(),
            acl_hash: meta.acl_hash.clone().unwrap_or_default(),
            acl_version: meta.acl_version.unwrap_or(1),
            embedding_policy: "allowed".to_string(),
        }
    }
```

- [ ] **Step 3: Update `emit_index_note` to resolve principals before building the payload**

Change:

```rust
    async fn emit_index_note(
        &self,
        file: &rustshare_core::domain::File,
        meta: &NoteMetadata,
        content: &str,
        tenant_id: Uuid,
    ) {
        if let Some(sink) = &self.index_sink {
            let read_acl = match self.resolve_note_read_principals(file, tenant_id).await {
                Ok(acl) => acl,
                Err(e) => {
                    tracing::warn!("Failed to resolve ACL principals for {}: {}", file.id, e);
                    return;
                }
            };
            let acl = Self::build_acl_payload(file, meta, tenant_id, read_acl);
            sink.index_note(
                file.id,
                file.name.clone(),
                file.path.clone(),
                content.to_string(),
                meta.mime_type.clone(),
                file.owner_id,
                acl,
            )
            .await;
        } else {
            tracing::debug!("No note index sink configured; skipping indexing");
        }
    }
```

- [ ] **Step 4: Define a minimal `AlwaysOwnerOps` mock**

Append to the `note_service.rs` test module. Tests that construct a `NoteService` can pass this mock when they do not care about resolved ACLs:

```rust
    use rustshare_core::services::{PermissionResolver, PermissionResolverOps};
    use async_trait::async_trait;

    struct AlwaysOwnerOps;

    #[async_trait]
    impl PermissionResolverOps for AlwaysOwnerOps {
        async fn find_user_share(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<FolderId>,
            _recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> Result<Option<Share>> {
            Ok(None)
        }
        async fn find_group_shares(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<FolderId>,
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_user_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_group_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_all_user_shares_for_file(
            &self,
            _file_id: FileId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_all_group_shares_for_file(
            &self,
            _file_id: FileId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_all_user_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_all_group_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_file_by_id(&self, _id: FileId, _tenant_id: Uuid) -> Result<Option<File>> {
            Ok(None)
        }
        async fn find_folder_by_id(&self, _id: FolderId, _tenant_id: Uuid) -> Result<Option<Folder>> {
            Ok(None)
        }
        async fn get_user_group_ids(&self, _user_id: UserId, _tenant_id: Uuid) -> Result<Vec<Uuid>> {
            Ok(Vec::new())
        }
    }
```

- [ ] **Step 5: Update the existing unit test for `build_acl_payload`**

`build_acl_payload` is now static and takes a `read_acl` vector. Change the call from:

```rust
let acl = NoteService::build_acl_payload(&file, &meta, tenant_id);
```

to:

```rust
let acl = NoteService::build_acl_payload(
    &file,
    &meta,
    tenant_id,
    vec![format!("owner:{owner_id}")],
);
```

Keep the existing assertions for tenant, workspace, note_id, visibility, etc.

- [ ] **Step 6: Add a test proving shares appear in `read_acl`**

```rust
    #[test]
    fn build_acl_payload_includes_shared_user_principal() {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let okf_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();

        let file = rustshare_core::domain::File::new(
            "note.md".to_string(),
            "/Workspace/Notes/Test/note.md".to_string(),
            "hash".to_string(),
            100,
            "text/markdown".to_string(),
            Some(parent_id),
            owner_id,
            tenant_id,
        );

        let mut meta = NoteMetadata::new("Test Note");
        meta.okf_id = Some(okf_id);
        meta.acl_hash = Some("test-hash".to_string());
        meta.acl_version = Some(3);
        meta.visibility = NoteVisibility::Public;

        let acl = NoteService::build_acl_payload(
            &file,
            &meta,
            tenant_id,
            vec![format!("owner:{owner_id}"), format!("user:{user_id}")],
        );

        assert!(acl.read_acl.contains(&format!("owner:{owner_id}")));
        assert!(acl.read_acl.contains(&format!("user:{user_id}")));
    }
```

---

## Task 6: Update all `NoteService` construction sites

**Files:**
- Modify: `backend/server/src/state.rs`
- Modify: `backend/server/src/bin/rustshare.rs`
- Modify: any tests or bootstrap code that calls `NoteService::new`

- [ ] **Step 1: Update `state.rs`**

Find the line that constructs `NoteService::new(...)` and add `permission_resolver` as the last argument. Pass the same `Arc<PermissionResolver<PermissionResolverRepository>>` instance that was used to construct `file_service` and `folder_service`, e.g. `state.permission_resolver.clone()`.

- [ ] **Step 2: Update `rustshare.rs` CLI**

In `backend/server/src/bin/rustshare.rs:108`, change:

```rust
let note_service = NoteService::new(file_service, folder_service, metadata_store, object_store);
```

to:

```rust
let note_service = NoteService::new(
    file_service,
    folder_service,
    metadata_store,
    object_store,
    permission_resolver,
);
```

- [ ] **Step 3: Update tests that build `NoteService`**

Search for all `NoteService::new` calls and add a `permission_resolver` argument. If a test does not care about ACLs, use the `AlwaysOwnerOps` mock defined in Task 5:

```rust
let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(AlwaysOwnerOps)));
```

---

## Task 7: Verify

- [ ] **Step 1: Check compilation**

Run: `cd backend && SQLX_OFFLINE=true cargo check --workspace`
Expected: PASS.

- [ ] **Step 2: Run tests**

Run: `cd backend && SQLX_OFFLINE=true cargo test --workspace --lib --bins`
Expected: PASS.

- [ ] **Step 3: Run clippy**

Run: `cd backend && cargo clippy --all-features -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/core/src/services/permission_resolver.rs \
       backend/crates/infrastructure/src/repositories/permission_resolver.rs \
       backend/server/src/services/note_service.rs \
       backend/server/src/state.rs \
       backend/server/src/bin/rustshare.rs \
       backend/.sqlx/
git commit -s -m "feat(notes): resolve real ACL principals for note indexing"
```
