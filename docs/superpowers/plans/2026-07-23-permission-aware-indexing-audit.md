# Permission-Aware AI Indexing Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the permission-aware AI indexing security audit and hardening described in the design spec. Prove that RustShare never indexes, retrieves, returns, or sends content to an LLM for a principal who does not currently have permission to access that content.

**Architecture:** Introduce a canonical, typed ACL projection (`IndexAclProjection`, `IndexPrincipal`, `IndexVisibility`, `EmbeddingPolicy`, `RetrievalPrincipal`) in `rustshare-core`. Store the full projection in both `InMemoryVectorStore` and `PgVectorStore`, enforce ACL pre-filtering at retrieval, and fail closed on missing, malformed, or stale ACL metadata. Wire all note lifecycle events (create, save, delete, move, duplicate, visibility toggle, share/revoke, group membership changes) to update or remove indexed chunks, and switch `AiService::semantic_search` to `search_with_acl`.

**Tech Stack:** Rust, SQLx/PostgreSQL/pgvector, Axum, Tokio

---

## File Structure

| File | Responsibility |
|------|----------------|
| `docs/superpowers/specs/2026-07-23-permission-aware-indexing-design.md` | Source design spec (read-only). |
| `docs/audits/2026-permission-aware-indexing-audit.md` | As-is/to-be pipeline map and contradiction findings. |
| `docs/contracts/permission-aware-indexing-contract.md` | Canonical ACL projection and principal model. |
| `docs/audits/2026-permission-aware-indexing-test-matrix.md` | Permission matrix and negative-test checklist. |
| `backend/migrations/20260723000000_note_index_acl_columns.sql` | Add `workspace_id`/`source_folder_id` to `note_index_chunks`. |
| `backend/crates/core/src/services/ai/indexing.rs` | Typed ACL types, `can_access`, `ContentIndexer`. |
| `backend/crates/core/src/services/ai/vector_store.rs` | `VectorStore` trait and `InMemoryVectorStore`. |
| `backend/crates/core/src/services/ai/mod.rs` | Re-exports. |
| `backend/crates/core/src/services/ai_service.rs` | `AiService::semantic_search` with `RetrievalPrincipal`. |
| `backend/crates/infrastructure/src/vector/pg_vector_store.rs` | `PgVectorStore` persistence and ACL enforcement. |
| `backend/crates/core/src/services/permission_resolver.rs` | Add `resolve_user_group_ids` helper. |
| `backend/server/src/services/note_service.rs` | ACL projection build, lifecycle wiring. |
| `backend/server/src/services/note_index_sink.rs` | `NoteIndexSink` trait (already has update/remove). |
| `backend/server/src/handlers/ai.rs` | Handler stays unchanged; service does the work. |
| `backend/server/src/bootstrap.rs` | Bootstrap unchanged unless service wiring changes. |
| `backend/server/src/handlers/shares.rs` | Refresh note ACL after revoke. |
| `backend/server/src/handlers/user_shares.rs` | Refresh note ACL after create/update/revoke. |
| `backend/server/src/handlers/groups.rs` | Refresh note ACL after group share revoke or membership change. |
| `backend/tests/ai_permission_contract.rs` | Replace with backend-agnostic contract tests. |
| `backend/tests/ai_index_permission_contract.rs` | New backend-agnostic contract tests for both stores. |
| `CHANGELOG.md` | Release notes. |

---

## Task 1: Map indexing/retrieval pipeline in `docs/audits/2026-permission-aware-indexing-audit.md`

**Files:**
- Create: `docs/audits/2026-permission-aware-indexing-audit.md`

- [ ] **Step 1: Create the audit document**

Document the current flow and contradictions:

```markdown
# Permission-Aware AI Indexing Audit

## Current pipeline

1. `NoteService::emit_index_note` resolves principals via `PermissionResolver::resolve_read_principals`.
2. `NoteService::build_acl_payload` creates `NoteAclPayload` with `workspace_id = tenant_id` and `embedding_policy = "allowed"`.
3. `ContentIndexer::index_note` strips frontmatter, generates embedding, and calls `VectorStore::upsert_chunk`.
4. `PgVectorStore::upsert_chunk` stores `NoteAclPayload` fields but hardcodes `workspace_id` and `source_folder_id` on read.
5. `AiService::semantic_search` calls `ContentIndexer::search(tenant_id, ...)` (tenant-only) and post-filters with `PermissionResolver`.
6. `InMemoryVectorStore::search_with_acl` falls back to `None` ACL => allowed.

## Contradictions (from design spec)

| ID | Location | Finding | Response |
|----|----------|---------|----------|
| C1 | `ai_service.rs:178` | Tenant-only scan + post-filter; stored `read_acl` unused. | Use `search_with_acl(&RetrievalPrincipal, ...)`. |
| C2 | `note_service.rs` | `delete_note`, `move_note`, `toggle_visibility`, `duplicate_note`, share/revoke do not update index. | Wire every lifecycle event. |
| C4 | `pg_vector_store.rs:53-56` | Hardcoded `workspace_id`/`source_folder_id`. | Add columns, persist real values. |
| C5 | `note_service.rs:705` | `embedding_policy` hardcoded. | Read from frontmatter/metadata. |
| C6 | `ai_permission_contract.rs` | Tests only run on `InMemoryVectorStore`. | Run on both backends. |
| C7 | `indexing.rs:38-42` | Owner-only placeholder comment. | Remove; use real resolver. |
| C8 | `indexing.rs:84` | `acl: None` treated as legacy tenant-wide. | Fail closed. |

## Target pipeline

1. Resolve authenticated caller -> `RetrievalPrincipal`.
2. Query vector store with tenant + ACL constraints.
3. Reject missing/malformed/stale ACL entries.
4. Post-check with `PermissionResolver` as defense-in-depth.
5. Return only authorized results.
```

- [ ] **Step 2: Commit the audit doc**

```bash
git add docs/audits/2026-permission-aware-indexing-audit.md
git commit -s -m "docs(audit): map permission-aware indexing pipeline and contradictions"
```

**Expected outcome:** Audit doc exists and matches the design spec contradictions table.

---

## Task 2: Define canonical ACL projection and principal model in `docs/contracts/permission-aware-indexing-contract.md`

**Files:**
- Create: `docs/contracts/permission-aware-indexing-contract.md`

- [ ] **Step 1: Write the contract**

```markdown
# Permission-Aware Indexing Contract

## Principals

Typed identifiers:

- `owner:<uuid>` — resource owner.
- `user:<uuid>` — direct user share recipient.
- `group:<uuid>` — group share recipient.
- `workspace:<uuid>` — workspace-scoped access.
- `public` — unauthenticated public access.

## ACL projection

```rust
pub struct IndexAclProjection {
    pub tenant_id: Uuid,
    pub workspace_id: Uuid,
    pub object_id: Uuid,
    pub owner_id: Uuid,
    pub read_principals: Vec<IndexPrincipal>,
    pub visibility: IndexVisibility,
    pub acl_hash: String,
    pub acl_version: i64,
    pub embedding_policy: EmbeddingPolicy,
}
```

## Retrieval principal

```rust
pub struct RetrievalPrincipal {
    pub tenant_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub user_id: Uuid,
    pub group_ids: Vec<Uuid>,
    pub min_acl_versions: HashMap<Uuid, i64>,
}
```

## Enforcement rules

1. Tenant filtering alone is insufficient.
2. Owner-only filtering is insufficient.
3. `embedding_policy = denied` => never indexed or returned.
4. Missing/malformed/stale ACL => fail closed.
5. Revoked permissions stop retrieval without rebuild.
6. Cross-tenant access always fails closed.
7. Public visibility must be explicit and tenant-safe.
8. Deleted/trashed objects must not be retrievable.
```

- [ ] **Step 2: Commit**

```bash
git add docs/contracts/permission-aware-indexing-contract.md
git commit -s -m "docs(contract): define permission-aware indexing ACL model"
```

**Expected outcome:** Contract doc captures principal grammar, projection schema, retrieval principal, and non-negotiable rules.

---

## Task 3: Create test matrix in `docs/audits/2026-permission-aware-indexing-test-matrix.md`

**Files:**
- Create: `docs/audits/2026-permission-aware-indexing-test-matrix.md`

- [ ] **Step 1: Write the matrix**

```markdown
# Permission-Aware Indexing Test Matrix

## Retrieval matrix

| Caller | Object ACL | Expected |
|--------|-----------|----------|
| Owner | owner principal + private | Found |
| Direct share user | user principal + private | Found |
| Group member | group principal + private | Found |
| Non-member, no share | owner-only | Not found |
| Public note, anonymous | public visibility | Found (tenant-scoped) |
| Workspace member | workspace principal / workspace visibility | Found |
| Other tenant | any | Not found |

## Revocation matrix

| Event | Initial state | After event | Expected |
|-------|--------------|-------------|----------|
| Revoke direct share | user principal in ACL | principal removed | Not found |
| Revoke group share | group principal in ACL | principal removed | Not found |
| Remove group member | group principal in caller groups | member removed | Not found |
| Move to private folder | inherited access | no inherited access | Not found |
| Public -> private | public visibility | private | Not found (non-owner) |
| Trash/delete | allowed | removed/stale | Not found |

## Negative / edge cases

- Cross-tenant query
- Stale `acl_version`
- Malformed `read_acl` string
- Missing ACL payload (`acl = None`)
- Embedding policy `denied`
- Trashed source file
```

- [ ] **Step 2: Commit**

```bash
git add docs/audits/2026-permission-aware-indexing-test-matrix.md
git commit -s -m "docs(audit): create permission-aware indexing test matrix"
```

**Expected outcome:** Matrix covers all scenarios in Task 11-13.

---

## Task 4: Add `workspace_id`/`source_folder_id` columns to `note_index_chunks`

**Files:**
- Create: `backend/migrations/20260723000000_note_index_acl_columns.sql`

- [ ] **Step 1: Create additive migration**

```sql
-- Add workspace and source-folder columns to the vector index.
-- Existing rows are backfilled from tenant_id; source_folder_id remains nullable.

ALTER TABLE note_index_chunks
    ADD COLUMN workspace_id uuid,
    ADD COLUMN source_folder_id uuid;

UPDATE note_index_chunks
    SET workspace_id = tenant_id
    WHERE workspace_id IS NULL;

ALTER TABLE note_index_chunks
    ALTER COLUMN workspace_id SET NOT NULL;

-- Keep existing indexes; add helper index for workspace-scoped ACL queries.
CREATE INDEX IF NOT EXISTS idx_note_index_chunks_workspace_note
    ON note_index_chunks(workspace_id, note_id);
```

- [ ] **Step 2: Verify migration ordering**

Ensure the filename timestamp is later than `20260627000000_note_vectors.up.sql`.

- [ ] **Step 3: Prepare SQLx offline metadata**

Run against a running PostgreSQL instance:

```bash
cargo sqlx prepare --workspace
```

- [ ] **Step 4: Commit**

```bash
git add backend/migrations/20260723000000_note_index_acl_columns.sql backend/.sqlx/
git commit -s -m "feat(index): add workspace_id and source_folder_id to note_index_chunks"
```

**Expected outcome:** `cargo sqlx prepare --workspace --check` passes after the column is used by code.

---

## Task 5: Centralize ACL projection types in `rustshare-core`

**Files:**
- Modify: `backend/crates/core/src/services/ai/indexing.rs`
- Modify: `backend/crates/core/src/services/ai/mod.rs`

- [ ] **Step 1: Add typed enums and structs**

In `backend/crates/core/src/services/ai/indexing.rs`, add after the imports:

```rust
use std::collections::HashSet;
use std::str::FromStr;

/// Visibility level stored in the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexVisibility {
    Private,
    Workspace,
    Public,
}

impl std::fmt::Display for IndexVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Private => write!(f, "private"),
            Self::Workspace => write!(f, "workspace"),
            Self::Public => write!(f, "public"),
        }
    }
}

impl FromStr for IndexVisibility {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "private" => Ok(Self::Private),
            "workspace" => Ok(Self::Workspace),
            "public" => Ok(Self::Public),
            _ => Err(anyhow::anyhow!("unknown visibility: {s}")),
        }
    }
}

/// Embedding/indexing policy for an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingPolicy {
    #[default]
    Allowed,
    Denied,
}

impl std::fmt::Display for EmbeddingPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allowed => write!(f, "allowed"),
            Self::Denied => write!(f, "denied"),
        }
    }
}

impl FromStr for EmbeddingPolicy {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "allowed" => Ok(Self::Allowed),
            "denied" => Ok(Self::Denied),
            _ => Err(anyhow::anyhow!("unknown embedding policy: {s}")),
        }
    }
}

/// A typed principal that may appear in an indexed ACL.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum IndexPrincipal {
    Owner(Uuid),
    User(Uuid),
    Group(Uuid),
    Workspace(Uuid),
    Public,
}

impl std::fmt::Display for IndexPrincipal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Owner(id) => write!(f, "owner:{id}"),
            Self::User(id) => write!(f, "user:{id}"),
            Self::Group(id) => write!(f, "group:{id}"),
            Self::Workspace(id) => write!(f, "workspace:{id}"),
            Self::Public => write!(f, "public"),
        }
    }
}

impl FromStr for IndexPrincipal {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "public" {
            return Ok(Self::Public);
        }
        let (kind, id) = s.split_once(':')
            .ok_or_else(|| anyhow::anyhow!("principal missing colon: {s}"))?;
        let id = Uuid::parse_str(id)
            .map_err(|e| anyhow::anyhow!("invalid principal uuid {id}: {e}"))?;
        match kind {
            "owner" => Ok(Self::Owner(id)),
            "user" => Ok(Self::User(id)),
            "group" => Ok(Self::Group(id)),
            "workspace" => Ok(Self::Workspace(id)),
            _ => Err(anyhow::anyhow!("unknown principal kind: {kind}")),
        }
    }
}

/// Canonical ACL projection for an indexed object.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexAclProjection {
    pub tenant_id: Uuid,
    pub workspace_id: Uuid,
    pub object_id: Uuid,
    pub owner_id: Uuid,
    pub read_principals: Vec<IndexPrincipal>,
    pub visibility: IndexVisibility,
    pub acl_hash: String,
    pub acl_version: i64,
    pub embedding_policy: EmbeddingPolicy,
}

/// Principal used at retrieval time.
#[derive(Debug, Clone, Default)]
pub struct RetrievalPrincipal {
    pub tenant_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub user_id: Uuid,
    pub group_ids: Vec<Uuid>,
    /// object_id -> minimum accepted acl_version.
    pub min_acl_versions: HashMap<Uuid, i64>,
}

impl RetrievalPrincipal {
    /// Return the principal strings to match against the stored ACL.
    pub fn to_index_principals(&self) -> Vec<String> {
        let mut out = vec![
            format!("owner:{}", self.user_id),
            format!("user:{}", self.user_id),
        ];
        for gid in &self.group_ids {
            out.push(format!("group:{gid}"));
        }
        if let Some(wid) = self.workspace_id {
            out.push(format!("workspace:{wid}"));
        }
        out
    }
}
```

- [ ] **Step 2: Replace `AclSearchFilter` with `RetrievalPrincipal`**

Change `AclSearchFilter` to an alias or remove it. The stores will use `RetrievalPrincipal` directly.

- [ ] **Step 3: Update `mod.rs` re-exports**

```rust
pub use indexing::{
    can_access, ContentIndexer, EmbeddingPolicy, IndexAclProjection, IndexPrincipal,
    IndexVisibility, IndexedDocument, NoteAclPayload, RetrievalPrincipal,
};
```

- [ ] **Step 4: Check compilation**

```bash
SQLX_OFFLINE=true cargo check -p rustshare-core
```

- [ ] **Step 5: Commit**

```bash
git add backend/crates/core/src/services/ai/indexing.rs backend/crates/core/src/services/ai/mod.rs
git commit -s -m "feat(core): add typed IndexAclProjection and RetrievalPrincipal types"
```

**Expected outcome:** New types compile; existing code is temporarily broken until Task 6-8.

---

## Task 6: Update `PgVectorStore` to persist and read full ACL projection

**Files:**
- Modify: `backend/crates/infrastructure/src/vector/pg_vector_store.rs`

- [ ] **Step 1: Update `upsert_chunk` to store new columns**

Change the INSERT/ON CONFLICT to include `workspace_id` and `source_folder_id`:

```rust
sqlx::query(
    r#"
    INSERT INTO note_index_chunks (
        id, tenant_id, workspace_id, source_folder_id, note_id, source_file_id,
        file_name, file_path, content, mime_type, owner_id, embedding,
        acl_hash, acl_version, read_acl, visibility, embedding_policy, indexed_at
    ) VALUES (
        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12::vector,
        $13, $14, $15, $16, $17, $18
    )
    ON CONFLICT (id) DO UPDATE SET
        tenant_id = EXCLUDED.tenant_id,
        workspace_id = EXCLUDED.workspace_id,
        source_folder_id = EXCLUDED.source_folder_id,
        note_id = EXCLUDED.note_id,
        source_file_id = EXCLUDED.source_file_id,
        file_name = EXCLUDED.file_name,
        file_path = EXCLUDED.file_path,
        content = EXCLUDED.content,
        mime_type = EXCLUDED.mime_type,
        owner_id = EXCLUDED.owner_id,
        embedding = EXCLUDED.embedding,
        acl_hash = EXCLUDED.acl_hash,
        acl_version = EXCLUDED.acl_version,
        read_acl = EXCLUDED.read_acl,
        visibility = EXCLUDED.visibility,
        embedding_policy = EXCLUDED.embedding_policy,
        indexed_at = EXCLUDED.indexed_at
    "#,
)
.bind(chunk_id)
.bind(tenant_id)
.bind(acl.workspace_id)
.bind(acl.source_folder_id)
.bind(acl.note_id)
.bind(acl.source_file_id)
...
```

- [ ] **Step 2: Update `row_to_indexed_doc` to read real columns**

```rust
fn row_to_indexed_doc(row: &PgRow, _tenant_id: Uuid) -> Result<IndexedDocument> {
    let acl = NoteAclPayload {
        tenant_id: row.try_get("tenant_id")?,
        workspace_id: row.try_get("workspace_id")?,
        note_id: row.try_get("note_id")?,
        source_file_id: row.try_get("source_file_id")?,
        source_folder_id: row.try_get("source_folder_id")?,
        owner_id: row.try_get("owner_id")?,
        read_acl: row.try_get("read_acl")?,
        visibility: row.try_get("visibility")?,
        acl_hash: row.try_get("acl_hash")?,
        acl_version: row.try_get("acl_version")?,
        embedding_policy: row.try_get("embedding_policy")?,
    };
    // ... rest unchanged
}
```

Update `row_to_doc` to not pass `tenant_id`.

- [ ] **Step 3: Update `search_with_acl` to use `RetrievalPrincipal` and fail closed**

```rust
async fn search_with_acl(
    &self,
    principal: &RetrievalPrincipal,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<(IndexedDocument, f32)>> {
    let caller_principals = principal.to_index_principals();
    let query_vector_text = encode_vector(query_embedding);
    let limit = limit as i64;

    let rows = sqlx::query(
        r#"
        SELECT
            id, tenant_id, workspace_id, source_folder_id, note_id, source_file_id,
            file_name, file_path, content, mime_type, owner_id, embedding::text as embedding,
            acl_hash, acl_version, read_acl, visibility, embedding_policy, indexed_at,
            1 - (embedding <=> $1::vector) AS similarity
        FROM note_index_chunks
        WHERE tenant_id = $2
          AND embedding_policy = 'allowed'
          AND 1 - (embedding <=> $1::vector) > 0.1
          AND (
              owner_id = $3
              OR visibility = 'public'
              OR visibility = 'workspace'
              OR read_acl && $4::text[]
          )
        ORDER BY embedding <=> $1::vector
        LIMIT $5
        "#,
    )
    .bind(&query_vector_text)
    .bind(principal.tenant_id)
    .bind(principal.user_id)
    .bind(&caller_principals)
    .bind(limit)
    .fetch_all(&self.pool)
    .await?;

    let mut results = Vec::new();
    for row in rows {
        let doc = row_to_indexed_doc(&row, principal.tenant_id)?;
        if let Some(acl) = &doc.acl {
            match validate_and_project(acl) {
                Ok(projection) => {
                    if can_access(&projection, principal) {
                        results.push((doc, similarity_from_row(&row)?));
                    }
                }
                Err(e) => {
                    metrics::counter!("ai_search_malformed_acl_total").increment(1);
                    tracing::warn!("Rejecting malformed ACL chunk {}: {}", doc.chunk_id, e);
                }
            }
        } else {
            metrics::counter!("ai_legacy_aclless_chunk_total").increment(1);
            tracing::warn!("Rejecting legacy ACL-less chunk {}", doc.chunk_id);
        }
    }
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results)
}
```

Add helper `similarity_from_row` or keep `row_to_doc` after validation.

- [ ] **Step 4: Add `validate_and_project` helper**

```rust
fn validate_and_project(acl: &NoteAclPayload) -> Result<IndexAclProjection> {
    if acl.embedding_policy != "allowed" {
        anyhow::bail!("embedding_policy is denied");
    }
    let visibility = IndexVisibility::from_str(&acl.visibility)?;
    let read_principals: Result<Vec<_>, _> = acl.read_acl.iter().map(|s| s.parse()).collect();
    Ok(IndexAclProjection {
        tenant_id: acl.tenant_id,
        workspace_id: acl.workspace_id,
        object_id: acl.note_id,
        owner_id: acl.owner_id,
        read_principals: read_principals?,
        visibility,
        acl_hash: acl.acl_hash.clone(),
        acl_version: acl.acl_version,
        embedding_policy: EmbeddingPolicy::Allowed,
    })
}
```

- [ ] **Step 5: Update `update_note_acl` to use `IndexAclProjection`**

Change signature to accept `&IndexAclProjection` and update `workspace_id`, `source_folder_id`, `visibility`, `embedding_policy`.

- [ ] **Step 6: Update `search` to fail closed on missing ACL**

`search` should also validate ACLs; if `acl` is `None`, reject the chunk.

- [ ] **Step 7: Check compilation**

```bash
SQLX_OFFLINE=true cargo check -p rustshare-infrastructure
```

- [ ] **Step 8: Commit**

```bash
git add backend/crates/infrastructure/src/vector/pg_vector_store.rs
git commit -s -m "feat(pgvector): persist full ACL projection and fail closed on malformed ACLs"
```

**Expected outcome:** `PgVectorStore` compiles and stores/reads `workspace_id`/`source_folder_id`.

---

## Task 7: Update `InMemoryVectorStore` parity and fail-closed behavior

**Files:**
- Modify: `backend/crates/core/src/services/ai/vector_store.rs`

- [ ] **Step 1: Update trait signature**

```rust
#[async_trait::async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert_chunk(
        &self,
        tenant_id: Uuid,
        chunk_id: Uuid,
        doc: &IndexedDocument,
        acl: &NoteAclPayload,
    ) -> anyhow::Result<()>;

    async fn search_with_acl(
        &self,
        principal: &RetrievalPrincipal,
        query_embedding: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<(IndexedDocument, f32)>>;

    async fn update_note_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        new_acl: &NoteAclPayload,
    ) -> anyhow::Result<usize>;

    // ... rest unchanged
}
```

- [ ] **Step 2: Update `InMemoryVectorStore::search_with_acl`**

```rust
async fn search_with_acl(
    &self,
    principal: &RetrievalPrincipal,
    query_embedding: &[f32],
    limit: usize,
) -> anyhow::Result<Vec<(IndexedDocument, f32)>> {
    use super::indexing::{validate_and_project, can_access};

    let docs = self.documents.lock().unwrap();
    let tenant_docs = docs.get(&principal.tenant_id).cloned().unwrap_or_default();

    let allowed = tenant_docs.into_values().filter(|doc| {
        match &doc.acl {
            Some(acl) => match validate_and_project(acl) {
                Ok(projection) => can_access(&projection, principal),
                Err(e) => {
                    metrics::counter!("ai_search_malformed_acl_total").increment(1);
                    tracing::warn!("Rejecting malformed ACL chunk {}: {}", doc.chunk_id, e);
                    false
                }
            },
            None => {
                metrics::counter!("ai_legacy_aclless_chunk_total").increment(1);
                false
            }
        }
    });
    Ok(score_and_rank(allowed, query_embedding, limit))
}
```

Move `validate_and_project` to a shared location (e.g., `indexing.rs`) so both stores use it.

- [ ] **Step 3: Update in-memory tests**

Change `AclSearchFilter` to `RetrievalPrincipal` in tests; expect legacy/None ACL chunks to be rejected.

- [ ] **Step 4: Check compilation and tests**

```bash
SQLX_OFFLINE=true cargo test -p rustshare-core --lib vector_store
```

- [ ] **Step 5: Commit**

```bash
git add backend/crates/core/src/services/ai/vector_store.rs
git commit -s -m "feat(memory-vector-store): parity ACL enforcement and fail-closed behavior"
```

**Expected outcome:** In-memory store rejects `None`/malformed ACLs just like pgvector.

---

## Task 8: Update `ContentIndexer` to use canonical ACL projection

**Files:**
- Modify: `backend/crates/core/src/services/ai/indexing.rs`

- [ ] **Step 1: Update `index_note` to accept `IndexAclProjection`**

```rust
pub async fn index_note(
    &self,
    file_id: Uuid,
    file_name: String,
    file_path: String,
    content: String,
    mime_type: String,
    owner_id: Uuid,
    acl: IndexAclProjection,
) -> anyhow::Result<()> {
    if acl.embedding_policy == EmbeddingPolicy::Denied {
        self.store
            .remove_note_chunks(acl.tenant_id, acl.object_id)
            .await?;
        return Ok(());
    }
    let body = strip_frontmatter(&content);
    let body = truncate(body, MAX_CONTENT_LENGTH);
    let combined_text = format!("{} {} {}", file_name, file_path, body);
    let embedding = self.embedding_generator.generate(&combined_text).await;

    let note_acl = NoteAclPayload {
        tenant_id: acl.tenant_id,
        workspace_id: acl.workspace_id,
        note_id: acl.object_id,
        source_file_id: file_id,
        source_folder_id: None, // caller sets via workspace_id or stores separately
        owner_id: acl.owner_id,
        read_acl: acl.read_principals.iter().map(|p| p.to_string()).collect(),
        visibility: acl.visibility.to_string(),
        acl_hash: acl.acl_hash,
        acl_version: acl.acl_version,
        embedding_policy: acl.embedding_policy.to_string(),
    };

    let document = IndexedDocument {
        file_id,
        file_name,
        file_path,
        content: body,
        embedding,
        mime_type,
        owner_id: acl.owner_id,
        tenant_id: acl.tenant_id,
        indexed_at: chrono::Utc::now(),
        acl: Some(note_acl.clone()),
        chunk_id: file_id,
    };

    self.store
        .upsert_chunk(acl.tenant_id, file_id, &document, &note_acl)
        .await
}
```

- [ ] **Step 2: Update `search_with_acl` signature**

```rust
pub async fn search_with_acl(
    &self,
    principal: &RetrievalPrincipal,
    query: &str,
    limit: usize,
) -> Vec<(IndexedDocument, f32)> {
    let query_embedding = self.embedding_generator.generate(query).await;
    self.store
        .search_with_acl(principal, query_embedding.as_slice(), limit)
        .await
        .unwrap_or_default()
}
```

- [ ] **Step 3: Update `update_note_acl` signature**

```rust
pub async fn update_note_acl(
    &self,
    tenant_id: Uuid,
    note_id: Uuid,
    new_acl: IndexAclProjection,
) -> usize {
    // convert to NoteAclPayload, call store
}
```

- [ ] **Step 4: Remove owner-only synthesis from `index_file`**

Change `index_file` to create a document with `acl: None`. Generic file indexing is out of scope; this prevents accidental owner-only fallback.

```rust
pub async fn index_file(...) -> anyhow::Result<()> {
    let content = truncate(content, MAX_CONTENT_LENGTH);
    let combined_text = format!("{} {} {}", file_name, file_path, content);
    let embedding = self.embedding_generator.generate(&combined_text).await;
    let document = IndexedDocument {
        file_id,
        file_name,
        file_path,
        content,
        embedding,
        mime_type,
        owner_id,
        tenant_id,
        indexed_at: chrono::Utc::now(),
        acl: None,
        chunk_id: file_id,
    };
    // Use an empty/denied ACL payload so the store has metadata but retrieval fails closed.
    let acl = NoteAclPayload {
        tenant_id,
        workspace_id: tenant_id,
        note_id: file_id,
        source_file_id: file_id,
        source_folder_id: None,
        owner_id,
        read_acl: vec![],
        visibility: "private".to_string(),
        acl_hash: String::new(),
        acl_version: 1,
        embedding_policy: "denied".to_string(),
    };
    self.store.upsert_chunk(tenant_id, file_id, &document, &acl).await
}
```

- [ ] **Step 5: Update `can_access` to use typed projection**

```rust
pub fn can_access(acl: &IndexAclProjection, principal: &RetrievalPrincipal) -> bool {
    if acl.embedding_policy != EmbeddingPolicy::Allowed {
        return false;
    }
    if let Some(min_version) = principal.min_acl_versions.get(&acl.object_id) {
        if acl.acl_version < *min_version {
            return false;
        }
    }
    if principal.user_id == acl.owner_id {
        return true;
    }
    let caller_principals: HashSet<_> = principal.to_index_principals().into_iter().collect();
    if acl.read_principals.iter().any(|p| caller_principals.contains(&p.to_string())) {
        return true;
    }
    if acl.visibility == IndexVisibility::Workspace {
        if let Some(wid) = principal.workspace_id {
            if wid == acl.workspace_id {
                return true;
            }
        }
    }
    if acl.visibility == IndexVisibility::Public {
        return true;
    }
    false
}
```

- [ ] **Step 6: Update unit tests in `indexing.rs`**

Replace `AclSearchFilter` with `RetrievalPrincipal`; update `make_acl_payload` helper; ensure `index_file` tests expect fail-closed or use `index_note`.

- [ ] **Step 7: Check core tests**

```bash
SQLX_OFFLINE=true cargo test -p rustshare-core --lib ai::indexing
```

- [ ] **Step 8: Commit**

```bash
git add backend/crates/core/src/services/ai/indexing.rs
git commit -s -m "feat(indexer): use canonical IndexAclProjection and fail-closed retrieval"
```

**Expected outcome:** `ContentIndexer` uses typed ACLs; generic `index_file` no longer synthesizes owner-only access.

---

## Task 9: Wire NoteService lifecycle events to index updates/removals

**Files:**
- Modify: `backend/server/src/services/note_service.rs`
- Modify: `backend/server/src/services/note_index_sink.rs` (if needed)
- Modify: `backend/server/src/handlers/shares.rs`
- Modify: `backend/server/src/handlers/user_shares.rs`
- Modify: `backend/server/src/handlers/groups.rs`

- [ ] **Step 1: Add `NoteMetadata::embedding_policy` field**

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub embedding_policy: Option<String>,
```

Default in `NoteMetadata::new` to `Some("allowed".to_string())`.

- [ ] **Step 2: Update `build_acl_payload` to use `IndexAclProjection`**

```rust
pub fn build_acl_payload(
    file: &rustshare_core::domain::File,
    meta: &NoteMetadata,
    tenant_id: Uuid,
    workspace_id: Uuid,
    read_acl: Vec<String>,
) -> IndexAclProjection {
    let embedding_policy = meta
        .embedding_policy
        .as_deref()
        .unwrap_or("allowed")
        .parse()
        .unwrap_or(EmbeddingPolicy::Allowed);

    IndexAclProjection {
        tenant_id,
        workspace_id,
        object_id: meta.okf_id.unwrap_or(file.id),
        owner_id: file.owner_id,
        read_principals: read_acl.iter().filter_map(|s| s.parse().ok()).collect(),
        visibility: meta.visibility.as_str().parse().unwrap_or(IndexVisibility::Private),
        acl_hash: meta.acl_hash.clone().unwrap_or_default(),
        acl_version: meta.acl_version.unwrap_or(1),
        embedding_policy,
    }
}
```

- [ ] **Step 3: Compute deterministic ACL hash and bump version in `emit_index_note`**

```rust
async fn emit_index_note(
    &self,
    file: &rustshare_core::domain::File,
    meta: &NoteMetadata,
    content: &str,
    tenant_id: Uuid,
) {
    let Some(sink) = &self.index_sink else {
        tracing::debug!("No note index sink configured; skipping indexing");
        return;
    };

    let workspace_id = self.resolve_workspace_id(file, tenant_id).await.unwrap_or(tenant_id);
    let read_acl = match self.resolve_note_read_principals(file, tenant_id).await {
        Ok(acl) => acl,
        Err(e) => {
            tracing::warn!("Failed to resolve ACL principals for {}: {}", file.id, e);
            metrics::counter!("ai_index_acl_rejected_total", "reason" => "resolve_failed").increment(1);
            return;
        }
    };

    let mut projection = Self::build_acl_payload(file, meta, tenant_id, workspace_id, read_acl);
    let new_hash = compute_index_acl_hash(&projection);
    if new_hash != projection.acl_hash {
        projection.acl_version = projection.acl_version.saturating_add(1);
    }
    projection.acl_hash = new_hash;

    sink.index_note(
        file.id,
        file.name.clone(),
        file.path.clone(),
        content.to_string(),
        meta.mime_type.clone(),
        file.owner_id,
        projection,
    ).await;
}
```

Add `resolve_workspace_id` helper returning `tenant_id` for now (no separate workspace model).

Update `compute_acl_hash` to hash all canonical inputs:

```rust
fn compute_index_acl_hash(projection: &IndexAclProjection) -> String {
    let mut principals = projection.read_principals.iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>();
    principals.sort();
    let input = format!(
        "tenant:{}:workspace:{}:object:{}:owner:{}:visibility:{}:policy:{}:principals:[{}]",
        projection.tenant_id,
        projection.workspace_id,
        projection.object_id,
        projection.owner_id,
        projection.visibility,
        projection.embedding_policy,
        principals.join(",")
    );
    hex::encode(Sha256::digest(input.as_bytes()))
}
```

- [ ] **Step 4: Wire delete, move, toggle_visibility, duplicate**

In `delete_note`, after deletion:

```rust
let note_id = meta.okf_id.unwrap_or(file.id);
if let Some(sink) = &self.index_sink {
    sink.remove_note(tenant_id, note_id).await;
}
```

In `move_note`, after saving metadata:

```rust
self.emit_index_note(&moved_file, &meta, &content, tenant_id).await;
```

In `toggle_visibility`, after saving metadata:

```rust
self.emit_index_note(&file, &meta, &content, tenant_id).await;
```

In `duplicate_note`, after saving new metadata:

```rust
self.emit_index_note(&new_file, &meta, &duplicated_content, tenant_id).await;
```

- [ ] **Step 5: Add public refresh helpers on `NoteService`**

```rust
/// Refresh the indexed ACL projection for a single note.
pub async fn refresh_note_index_acl(
    &self,
    file_id: Uuid,
    user_id: UserId,
    tenant_id: Uuid,
) -> Result<(), NoteError> {
    let file = self.file_service.get_file(file_id, user_id).await?;
    if file.tenant_id != tenant_id {
        return Err(NoteError::PermissionDenied);
    }
    let meta = self.load_metadata(file_id, user_id, tenant_id).await?
        .unwrap_or_else(|| NoteMetadata::new(file.name.trim_end_matches(".md")));
    let storage_key = file.storage_key();
    let content = match self.object_store.get(&storage_key).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(_) => String::new(),
    };
    self.emit_index_note(&file, &meta, &content, tenant_id).await;
    Ok(())
}

/// Remove a single note from the index.
pub async fn remove_note_from_index(
    &self,
    file_id: Uuid,
    tenant_id: Uuid,
) -> Result<(), NoteError> {
    let file = self.metadata_store.find_file_by_id_unchecked(file_id).await
        .map_err(|e| NoteError::Database(e.to_string()))?
        .ok_or(NoteError::NotFound(file_id))?;
    if file.tenant_id != tenant_id {
        return Err(NoteError::PermissionDenied);
    }
    let note_id = file_id; // best-effort; okf_id not available without metadata
    if let Some(meta) = self.load_metadata(file_id, file.owner_id, tenant_id).await? {
        let note_id = meta.okf_id.unwrap_or(file_id);
        if let Some(sink) = &self.index_sink {
            sink.remove_note(tenant_id, note_id).await;
        }
    }
    Ok(())
}
```

- [ ] **Step 6: Call refresh from share/revoke handlers**

In `backend/server/src/handlers/user_shares.rs`, after `create_file_share` and revoke/update handlers:

```rust
let _ = state.note_service.refresh_note_index_acl(file_id, auth.user_id, auth.tenant_id).await;
```

In `backend/server/src/handlers/shares.rs`, after `revoke_share`:

```rust
// Best-effort refresh; share may be on a folder.
if let Some(file_id) = share.file_id {
    let _ = state.note_service.refresh_note_index_acl(file_id, auth.user_id, auth.tenant_id).await;
}
```

In `backend/server/src/handlers/groups.rs`, after `revoke_group_share`:

```rust
// Best-effort refresh if share targets a file.
if let Some(file_id) = share.file_id {
    let _ = state.note_service.refresh_note_index_acl(file_id, auth.user_id, auth.tenant_id).await;
}
```

- [ ] **Step 7: Update existing unit tests**

Update `build_acl_payload_maps_note_metadata` and `build_acl_payload_includes_shared_user_principal` to expect `IndexAclProjection`.

- [ ] **Step 8: Check compilation**

```bash
SQLX_OFFLINE=true cargo check -p rustshare-server
```

- [ ] **Step 9: Commit**

```bash
git add backend/server/src/services/note_service.rs backend/server/src/handlers/shares.rs backend/server/src/handlers/user_shares.rs backend/server/src/handlers/groups.rs
git commit -s -m "feat(notes): wire lifecycle events to permission-aware index updates"
```

**Expected outcome:** Note deletes, moves, visibility toggles, duplicates, and share/revoke events update the index.

---

## Task 10: Switch `AiService::semantic_search` to `search_with_acl` with `RetrievalPrincipal`

**Files:**
- Modify: `backend/crates/core/src/services/ai_service.rs`
- Modify: `backend/crates/core/src/services/permission_resolver.rs`

- [ ] **Step 1: Add `resolve_user_group_ids` to `PermissionResolver`**

```rust
pub async fn resolve_user_group_ids(
    &self,
    user_id: UserId,
    tenant_id: Uuid,
) -> Result<Vec<Uuid>> {
    self.ops.get_user_group_ids(user_id, tenant_id).await
}
```

- [ ] **Step 2: Update `AiService::semantic_search`**

```rust
pub async fn semantic_search(
    &self,
    query: &str,
    user_id: UserId,
    tenant_id: Uuid,
    limit: usize,
) -> Result<Vec<SemanticSearchResult>, AiError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AiError::InvalidQuery("Query cannot be empty".to_string()));
    }
    if query.len() > 1000 {
        return Err(AiError::InvalidQuery("Query too long (max 1000 chars)".to_string()));
    }

    let group_ids = self.permission_resolver
        .resolve_user_group_ids(user_id, tenant_id)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to resolve groups for {}: {}", user_id, e);
            Vec::new()
        })
        .unwrap_or_default();

    let principal = RetrievalPrincipal {
        tenant_id,
        workspace_id: Some(tenant_id),
        user_id,
        group_ids,
        min_acl_versions: HashMap::new(),
    };

    let raw_results = self.indexer.search_with_acl(&principal, query, limit * 3).await;

    let raw_results: Vec<_> = raw_results
        .into_iter()
        .filter(|(doc, _)| {
            !doc.file_name.starts_with(".rustshare")
                && doc.file_name != "events.jsonl"
                && doc.file_name != "index.md"
                && doc.file_name != "__primary__.md"
                && !doc.file_name.ends_with(".editor.json")
        })
        .collect();

    let mut results = Vec::new();
    for (document, score) in raw_results {
        let resource = Resource::File(document.file_id);
        let permission = match self.permission_resolver.resolve_permission(user_id, tenant_id, resource).await {
            Ok(perm) => perm,
            Err(e) => {
                tracing::warn!("Permission resolution failed for file {}: {}. Skipping.", document.file_id, e);
                metrics::counter!("ai_search_permission_verify_failed_total").increment(1);
                continue;
            }
        };
        if let Some(perm) = permission {
            let snippet = sanitize_snippet(&truncate_with_ellipsis(&document.content, 200));
            results.push(SemanticSearchResult {
                file_id: document.file_id,
                file_name: document.file_name.clone(),
                file_path: document.file_path.clone(),
                relevance_score: score,
                snippet,
                mime_type: document.mime_type.clone(),
                owner_id: document.owner_id,
                can_edit: perm >= SharePermissions::Edit,
            });
            if results.len() >= limit {
                break;
            }
        } else {
            metrics::counter!("ai_search_acl_post_filter_denied_total").increment(1);
            tracing::warn!("Post-filter denied file {}", document.file_id);
        }
    }

    Ok(results)
}
```

- [ ] **Step 3: Update `AiService` tests**

Replace `index_file` calls with `index_note` + `IndexAclProjection` in tests that rely on search results. Tests that assert empty results for unregistered files can remain.

- [ ] **Step 4: Check tests**

```bash
SQLX_OFFLINE=true cargo test -p rustshare-core --lib ai_service
```

- [ ] **Step 5: Commit**

```bash
git add backend/crates/core/src/services/ai_service.rs backend/crates/core/src/services/permission_resolver.rs
git commit -s -m "feat(ai): switch semantic_search to ACL-pre-filtered retrieval"
```

**Expected outcome:** `semantic_search` uses `search_with_acl` with `RetrievalPrincipal` and keeps `PermissionResolver` post-filter.

---

## Task 11: Add backend-agnostic permission contract tests

**Files:**
- Create: `backend/tests/ai_index_permission_contract.rs`
- Delete: `backend/tests/ai_permission_contract.rs` (or repurpose)

- [ ] **Step 1: Define shared test harness**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use rustshare_core::services::ai::embedding::MockEmbeddingGenerator;
use rustshare_core::services::{
    can_access, ContentIndexer, EmbeddingPolicy, IndexAclProjection, IndexPrincipal,
    IndexVisibility, InMemoryVectorStore, NoteAclPayload, PgVectorStore, RetrievalPrincipal,
    VectorStore,
};

async fn run_matrix_tests(store: Arc<dyn VectorStore>) {
    let generator = Arc::new(MockEmbeddingGenerator::new());
    let indexer = Arc::new(ContentIndexer::new(generator, store));
    // ... matrix tests
}
```

For `MockEmbeddingGenerator`, either use the existing `SimpleEmbeddingGenerator` or add a deterministic mock that returns fixed vectors.

- [ ] **Step 2: Test owner, direct share, group, public, workspace**

```rust
async fn assert_found(
    indexer: &ContentIndexer<impl EmbeddingGenerator>,
    principal: &RetrievalPrincipal,
    query: &str,
) {
    let results = indexer.search_with_acl(principal, query, 10).await;
    assert_eq!(results.len(), 1, "expected one result");
}

async fn assert_not_found(
    indexer: &ContentIndexer<impl EmbeddingGenerator>,
    principal: &RetrievalPrincipal,
    query: &str,
) {
    let results = indexer.search_with_acl(principal, query, 10).await;
    assert!(results.is_empty(), "expected no results");
}
```

- [ ] **Step 3: Run against both backends**

```rust
#[tokio::test]
async fn in_memory_permission_matrix() {
    let store = Arc::new(InMemoryVectorStore::new());
    run_matrix_tests(store).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn pgvector_permission_matrix() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM note_index_chunks").execute(&pool).await.unwrap();
    let store = Arc::new(PgVectorStore::new(pool));
    run_matrix_tests(store).await;
}
```

- [ ] **Step 4: Remove old `backend/tests/ai_permission_contract.rs`**

It tests the old post-filter path and uses `index_file`.

- [ ] **Step 5: Check tests**

```bash
SQLX_OFFLINE=true cargo test --test ai_index_permission_contract -- --ignored
# Run without --ignored for in-memory only
```

- [ ] **Step 6: Commit**

```bash
git rm backend/tests/ai_permission_contract.rs
git add backend/tests/ai_index_permission_contract.rs
git commit -s -m "test(ai): backend-agnostic permission contract tests for both vector stores"
```

**Expected outcome:** Matrix tests pass for `InMemoryVectorStore`; pgvector tests pass when `DATABASE_URL` is set.

---

## Task 12: Add revocation regression tests

**Files:**
- Modify: `backend/tests/ai_index_permission_contract.rs`

- [ ] **Step 1: Add direct share revocation test**

Index with `owner` + `user` principals; search as recipient -> found. Update ACL to `owner` only; search as recipient -> not found.

- [ ] **Step 2: Add group membership revocation test**

Index with `owner` + `group` principals; search with group -> found. Update principal to remove group; search -> not found.

- [ ] **Step 3: Add folder inheritance / public-to-private test**

Index with public visibility; search as stranger -> found. Update to private; search -> not found.

- [ ] **Step 4: Add embedding-denied revocation**

Index allowed; search found. Update to denied; search -> not found and `document_count` is 0.

- [ ] **Step 5: Run and commit**

```bash
SQLX_OFFLINE=true cargo test --test ai_index_permission_contract revocation
git add backend/tests/ai_index_permission_contract.rs
git commit -s -m "test(ai): add revocation regression tests for shares, groups, and visibility"
```

**Expected outcome:** All revocation scenarios have negative assertions.

---

## Task 13: Add cross-tenant, stale ACL, malformed ACL, trashed/deleted tests

**Files:**
- Modify: `backend/tests/ai_index_permission_contract.rs`

- [ ] **Step 1: Cross-tenant test**

Index in tenant A. Query with tenant B principal -> no results.

- [ ] **Step 2: Stale ACL version test**

Index with `acl_version = 1`. Query with `min_acl_versions` requiring version 2 -> no results.

- [ ] **Step 3: Malformed ACL test**

Insert a row (via raw SQL for pgvector, or direct store for memory) with invalid `read_acl` string. Query -> no results and metric logged.

- [ ] **Step 4: Legacy ACL-less test**

Insert a row with `acl = None`. Query -> no results.

- [ ] **Step 5: Embedding-denied test**

Index with `EmbeddingPolicy::Denied`. Query as owner -> no results.

- [ ] **Step 6: Run and commit**

```bash
SQLX_OFFLINE=true cargo test --test ai_index_permission_contract edge
git add backend/tests/ai_index_permission_contract.rs
git commit -s -m "test(ai): add cross-tenant, stale, malformed, and denied ACL tests"
```

**Expected outcome:** All edge cases fail closed.

---

## Task 14: Add operational metrics/logging for ACL rejections and failures

**Files:**
- Modify: `backend/crates/core/src/services/ai/indexing.rs`
- Modify: `backend/crates/core/src/services/ai/vector_store.rs`
- Modify: `backend/crates/infrastructure/src/vector/pg_vector_store.rs`
- Modify: `backend/server/src/services/note_service.rs`

- [ ] **Step 1: Add metrics counters**

Use `metrics::counter!` with labels:

```rust
metrics::counter!("ai_index_acl_rejected_total", "reason" => "resolve_failed").increment(1);
metrics::counter!("ai_search_acl_rejected_total", "reason" => "stale_version").increment(1);
metrics::counter!("ai_search_acl_rejected_total", "reason" => "no_acl").increment(1);
metrics::counter!("ai_search_malformed_acl_total").increment(1);
metrics::counter!("ai_legacy_aclless_chunk_total").increment(1);
metrics::counter!("ai_index_acl_update_failed_total").increment(1);
metrics::counter!("ai_index_remove_failed_total").increment(1);
metrics::counter!("ai_permission_verify_failed_total").increment(1);
```

- [ ] **Step 2: Add structured logs**

Use `tracing::warn!` with `file_id`, `note_id`, `tenant_id`, `reason`. Never log content, embeddings, or filenames beyond file_id.

- [ ] **Step 3: Verify log safety**

Search for any log line that includes `content`, `embedding`, `prompt`, or email. Remove or redact.

- [ ] **Step 4: Run tests**

```bash
SQLX_OFFLINE=true cargo test -p rustshare-core --lib ai
```

- [ ] **Step 5: Commit**

```bash
git add backend/crates/core/src/services/ai/indexing.rs backend/crates/core/src/services/ai/vector_store.rs backend/crates/infrastructure/src/vector/pg_vector_store.rs backend/server/src/services/note_service.rs
git commit -s -m "feat(ai): add operational metrics and safe logging for ACL enforcement"
```

**Expected outcome:** All ACL rejections and failures emit metrics and safe logs.

---

## Task 15: Update docs and `CHANGELOG.md`

**Files:**
- Modify: `docs/audits/2026-permission-aware-indexing-audit.md`
- Modify: `docs/contracts/permission-aware-indexing-contract.md`
- Modify: `docs/audits/2026-permission-aware-indexing-test-matrix.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Add Security section entry in CHANGELOG.md**

Under `## [Unreleased] -> ### Security`, add:

```markdown
- Hardened permission-aware AI indexing: retrieval now pre-filters by stored ACLs in both `InMemoryVectorStore` and `PgVectorStore`, legacy ACL-less chunks and malformed ACLs fail closed, and all note lifecycle events propagate ACL changes to the index.
```

- [ ] **Step 2: Update audit and contract docs**

Mark contradictions as resolved and link to PR.

- [ ] **Step 3: Commit**

```bash
git add docs/audits/2026-permission-aware-indexing-audit.md docs/contracts/permission-aware-indexing-contract.md docs/audits/2026-permission-aware-indexing-test-matrix.md CHANGELOG.md
git commit -s -m "docs: update audit, contract, and changelog for permission-aware indexing"
```

**Expected outcome:** Documentation reflects the implemented behavior.

---

## Task 16: Run full validation and create draft PR

- [ ] **Step 1: Run all Rust validation commands from the design spec**

```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo check --workspace --all-features
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
SQLX_OFFLINE=true cargo test --workspace --all-features
cargo sqlx prepare --workspace --check
cargo deny --all-features check
```

- [ ] **Step 2: Run pgvector contract tests with a live database**

```bash
# Requires DATABASE_URL and running PostgreSQL with pgvector.
SQLX_OFFLINE=true cargo test --test ai_index_permission_contract -- --ignored
```

- [ ] **Step 3: Open draft PR**

Use the PR title from the design spec:

```bash
git push -u origin security/permission-aware-indexing-audit
```

Create PR: `Audit and harden permission-aware AI indexing`.

Include:
- Summary of contradictions resolved.
- Non-negotiable security rules checklist.
- Test matrix summary.
- Request human review per `AGENTS.md` safety boundaries (permissions, indexing/search).

- [ ] **Step 4: Commit**

```bash
git commit -s --allow-empty -m "chore: finalize permission-aware indexing audit validation"
```

**Expected outcome:** All validation commands pass; draft PR is open and marked for human review.
