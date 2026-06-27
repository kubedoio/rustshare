# OKF Notes — Persistent Vector Database for Indexing

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the in-memory `HashMap<Uuid, ContentIndex>` in `ContentIndexer` with a persistent vector store so note embeddings survive restarts and scale beyond a single process.

**Architecture:** Introduce a `VectorStore` trait with the same operations `ContentIndexer` currently performs on its HashMap. Provide a `PgVectorStore` implementation backed by `pgvector`. Keep the `ContentIndexer` public API unchanged so callers (including `NoteIndexSink`) need no changes.

**Tech Stack:** Rust 1.95, SQLx, PostgreSQL 16+, `pgvector` extension, `pgvector` crate (or raw SQL with `vector` casts).

---

## Files

- Create: `backend/migrations/20260627000000_note_vectors.up.sql`
- Create: `backend/migrations/20260627000000_note_vectors.down.sql`
- Create: `backend/crates/core/src/services/ai/vector_store.rs`
- Create: `backend/crates/infrastructure/src/vector/pg_vector_store.rs`
- Modify: `backend/crates/core/src/services/ai/indexing.rs`
- Modify: `backend/crates/core/src/services/ai/mod.rs`
- Modify: `backend/crates/infrastructure/src/lib.rs`
- Modify: `backend/server/src/bootstrap.rs` or `state.rs` (where `ContentIndexer` is constructed)
- Test: `backend/crates/core/src/services/ai/indexing.rs`

---

## Task 1: Add the database migration

**Files:**
- Create: `backend/migrations/20260627000000_note_vectors.up.sql`
- Create: `backend/migrations/20260627000000_note_vectors.down.sql`

- [ ] **Step 1: Write the up migration**

```sql
-- Enable pgvector. Safe to run if already enabled.
CREATE EXTENSION IF NOT EXISTS vector;

-- Dimension must match rustshare_core::services::ai::embedding::EMBEDDING_DIM (currently 768).
-- Store one row per indexed chunk. file_id is the chunk id for notes.
CREATE TABLE IF NOT EXISTS note_index_chunks (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    note_id uuid NOT NULL,
    source_file_id uuid NOT NULL,
    file_name text NOT NULL,
    file_path text NOT NULL,
    content text NOT NULL,
    mime_type text NOT NULL,
    owner_id uuid NOT NULL,
    embedding vector(768) NOT NULL,
    acl_hash text NOT NULL DEFAULT '',
    acl_version bigint NOT NULL DEFAULT 1,
    read_acl text[] NOT NULL DEFAULT '{}',
    visibility text NOT NULL DEFAULT 'private',
    embedding_policy text NOT NULL DEFAULT 'allowed',
    indexed_at timestamptz NOT NULL DEFAULT NOW(),

    CONSTRAINT note_index_chunks_positive_acl_version CHECK (acl_version > 0)
);

-- Fast tenant-scoped similarity search with ACL pre-filtering.
CREATE INDEX IF NOT EXISTS idx_note_index_chunks_tenant_note
    ON note_index_chunks(tenant_id, note_id);

CREATE INDEX IF NOT EXISTS idx_note_index_chunks_embedding
    ON note_index_chunks USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);
```

- [ ] **Step 2: Write the down migration**

```sql
DROP INDEX IF EXISTS idx_note_index_chunks_embedding;
DROP INDEX IF EXISTS idx_note_index_chunks_tenant_note;
DROP TABLE IF EXISTS note_index_chunks;
```

- [ ] **Step 3: Verify migration runs**

Run: `cd backend && sqlx migrate run`
Expected: SUCCESS.

---

## Task 2: Define the `VectorStore` trait

**Files:**
- Create: `backend/crates/core/src/services/ai/vector_store.rs`

- [ ] **Step 1: Create the trait file**

```rust
//! Vector storage abstraction for the AI content index.

use std::collections::HashMap;
use uuid::Uuid;

use super::indexing::{AclSearchFilter, IndexedDocument, NoteAclPayload};

/// A persistent or ephemeral backend for indexed document chunks.
#[async_trait::async_trait]
pub trait VectorStore: Send + Sync {
    /// Upsert a chunk for a note.
    async fn upsert_chunk(
        &self,
        tenant_id: Uuid,
        chunk_id: Uuid,
        doc: &IndexedDocument,
        acl: &NoteAclPayload,
    ) -> anyhow::Result<()>;

    /// Search for chunks similar to the query, pre-filtered by tenant and ACL.
    async fn search_with_acl(
        &self,
        filter: &AclSearchFilter,
        query_embedding: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<(IndexedDocument, f32)>>;

    /// Search for chunks similar to the query, scoped to a tenant only.
    /// This preserves the legacy non-ACL search behavior for non-note content.
    async fn search(
        &self,
        tenant_id: Uuid,
        query_embedding: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<(IndexedDocument, f32)>>;

    /// Update the ACL projection for every chunk of a note.
    async fn update_note_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        new_acl: &NoteAclPayload,
    ) -> anyhow::Result<usize>;

    /// Remove every chunk for a note.
    async fn remove_note_chunks(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
    ) -> anyhow::Result<usize>;

    /// Remove a single chunk by id.
    async fn remove_chunk(&self, tenant_id: Uuid, chunk_id: Uuid) -> anyhow::Result<()>;

    /// Remove all chunks for a tenant.
    async fn clear_tenant(&self, tenant_id: Uuid) -> anyhow::Result<()>;

    /// Count chunks for a tenant.
    async fn document_count(&self, tenant_id: Uuid) -> anyhow::Result<usize>;
}

/// In-memory vector store used for tests and local development.
pub struct InMemoryVectorStore {
    // tenant_id -> chunk_id -> (document, acl)
    documents: std::sync::Mutex<HashMap<Uuid, HashMap<Uuid, (IndexedDocument, NoteAclPayload)>>>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            documents: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn upsert_chunk(
        &self,
        tenant_id: Uuid,
        chunk_id: Uuid,
        doc: &IndexedDocument,
        acl: &NoteAclPayload,
    ) -> anyhow::Result<()> {
        let mut docs = self.documents.lock().unwrap();
        docs.entry(tenant_id)
            .or_default()
            .insert(chunk_id, (doc.clone(), acl.clone()));
        Ok(())
    }

    async fn search_with_acl(
        &self,
        filter: &AclSearchFilter,
        query_embedding: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<(IndexedDocument, f32)>> {
        use super::indexing::can_access;

        fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
            if a.len() != b.len() {
                return 0.0;
            }
            let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
            let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm_a == 0.0 || norm_b == 0.0 {
                0.0
            } else {
                (dot / (norm_a * norm_b)).clamp(0.0, 1.0)
            }
        }

        let docs = self.documents.lock().unwrap();
        let tenant_docs = docs.get(&filter.tenant_id).cloned().unwrap_or_default();

        let mut results: Vec<(IndexedDocument, f32)> = tenant_docs
            .values()
            .filter(|(doc, _)| match &doc.acl {
                Some(acl) => can_access(acl, filter),
                None => true,
            })
            .map(|(doc, _)| {
                let score = cosine_similarity(query_embedding, &doc.embedding);
                (doc.clone(), score)
            })
            .filter(|(_, score)| *score > 0.1)
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        Ok(results)
    }

    async fn update_note_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        new_acl: &NoteAclPayload,
    ) -> anyhow::Result<usize> {
        let mut docs = self.documents.lock().unwrap();
        let mut updated = 0;
        if let Some(tenant) = docs.get_mut(&tenant_id) {
            for (doc, acl) in tenant.values_mut() {
                if let Some(existing) = &doc.acl {
                    if existing.note_id == note_id {
                        *acl = new_acl.clone();
                        doc.acl = Some(new_acl.clone());
                        updated += 1;
                    }
                }
            }
        }
        Ok(updated)
    }

    async fn remove_note_chunks(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
    ) -> anyhow::Result<usize> {
        let mut docs = self.documents.lock().unwrap();
        let mut removed = 0;
        if let Some(tenant) = docs.get_mut(&tenant_id) {
            let to_remove: Vec<Uuid> = tenant
                .iter()
                .filter(|(_, (doc, _))| {
                    doc.acl
                        .as_ref()
                        .map(|acl| acl.note_id == note_id)
                        .unwrap_or(false)
                })
                .map(|(id, _)| *id)
                .collect();
            for id in to_remove {
                tenant.remove(&id);
                removed += 1;
            }
        }
        Ok(removed)
    }

    async fn remove_chunk(&self, tenant_id: Uuid, chunk_id: Uuid) -> anyhow::Result<()> {
        let mut docs = self.documents.lock().unwrap();
        if let Some(tenant) = docs.get_mut(&tenant_id) {
            tenant.remove(&chunk_id);
        }
        Ok(())
    }

    async fn clear_tenant(&self, tenant_id: Uuid) -> anyhow::Result<()> {
        let mut docs = self.documents.lock().unwrap();
        docs.remove(&tenant_id);
        Ok(())
    }

    async fn document_count(&self, tenant_id: Uuid) -> anyhow::Result<usize> {
        let docs = self.documents.lock().unwrap();
        Ok(docs.get(&tenant_id).map(|m| m.len()).unwrap_or(0))
    }
}
```

- [ ] **Step 2: Re-export from `ai/mod.rs`**

Add to `backend/crates/core/src/services/ai/mod.rs`:

```rust
pub mod vector_store;
pub use vector_store::{InMemoryVectorStore, VectorStore};
```

- [ ] **Step 3: Add `async-trait` to `rustshare-core`**

In `backend/crates/core/Cargo.toml`, add to `[dependencies]`:

```toml
async-trait = "0.1"
```

- [ ] **Step 4: Confirm core compiles**

Run: `cd backend && SQLX_OFFLINE=true cargo check -p rustshare-core`
Expected: PASS.

---

## Task 3: Adapt `ContentIndexer` to use `VectorStore`

**Files:**
- Modify: `backend/crates/core/src/services/ai/indexing.rs`

- [ ] **Step 1: Replace the HashMap field with an `Arc<dyn VectorStore>`**

Change:

```rust
pub struct ContentIndexer<EG: EmbeddingGenerator> {
    embedding_generator: Arc<EG>,
    indexes: Arc<RwLock<HashMap<Uuid, ContentIndex>>>,
}
```

to:

```rust
pub struct ContentIndexer<EG: EmbeddingGenerator> {
    embedding_generator: Arc<EG>,
    store: Arc<dyn VectorStore>,
}
```

Update `new` to accept a store:

```rust
    pub fn new(embedding_generator: Arc<EG>, store: Arc<dyn VectorStore>) -> Self {
        Self {
            embedding_generator,
            store,
        }
    }
```

- [ ] **Step 2: Reimplement `index_file`**

Replace the body with:

```rust
        let content = if content.len() > MAX_CONTENT_LENGTH {
            content[..MAX_CONTENT_LENGTH].to_string()
        } else {
            content
        };

        let combined_text = format!("{} {} {}", file_name, file_path, content);
        let embedding = self.embedding_generator.generate(&combined_text).await;

        let document = IndexedDocument {
            file_id,
            file_name: file_name.clone(),
            file_path: file_path.clone(),
            content,
            embedding,
            mime_type,
            owner_id,
            tenant_id,
            indexed_at: chrono::Utc::now(),
            acl: None,
            chunk_id: file_id,
        };

        self.store.upsert_chunk(tenant_id, file_id, &document, &NoteAclPayload {
            tenant_id,
            workspace_id: tenant_id,
            note_id: file_id,
            source_file_id: file_id,
            source_folder_id: None,
            owner_id,
            read_acl: vec![format!("owner:{owner_id}")],
            visibility: "private".to_string(),
            acl_hash: String::new(),
            acl_version: 1,
            embedding_policy: "allowed".to_string(),
        }).await
```

- [ ] **Step 3: Reimplement `index_note`**

Replace the body with:

```rust
        if acl.embedding_policy == "denied" {
            self.store.remove_note_chunks(acl.tenant_id, acl.note_id).await?;
            return Ok(());
        }

        let body = strip_frontmatter(&content);
        let body = if body.len() > MAX_CONTENT_LENGTH {
            body[..MAX_CONTENT_LENGTH].to_string()
        } else {
            body
        };

        let combined_text = format!("{} {} {}", file_name, file_path, body);
        let embedding = self.embedding_generator.generate(&combined_text).await;

        let document = IndexedDocument {
            file_id,
            file_name: file_name.clone(),
            file_path: file_path.clone(),
            content: body,
            embedding,
            mime_type,
            owner_id,
            tenant_id: acl.tenant_id,
            indexed_at: chrono::Utc::now(),
            acl: Some(acl.clone()),
            chunk_id: file_id,
        };

        self.store.upsert_chunk(acl.tenant_id, file_id, &document, &acl).await
```

- [ ] **Step 4: Reimplement search, update, remove, count methods**

`search_with_acl`:

```rust
        let query_embedding = self.embedding_generator.generate(query).await;
        let results = self
            .store
            .search_with_acl(filter, query_embedding.as_slice(), limit)
            .await
            .unwrap_or_default();
        results
```

`update_note_acl`:

```rust
        self.store.update_note_acl(tenant_id, note_id, &new_acl).await.unwrap_or(0)
```

`remove_note_chunks`:

```rust
        self.store.remove_note_chunks(tenant_id, note_id).await.unwrap_or(0)
```

`remove_file`:

```rust
        let _ = self.store.remove_chunk(tenant_id, file_id).await;
```

`search` (legacy tenant-only):

```rust
        let filter = AclSearchFilter {
            tenant_id,
            caller_user_id: Uuid::nil(),
            caller_group_ids: Vec::new(),
            min_acl_versions: HashMap::new(),
        };
        let query_embedding = self.embedding_generator.generate(query).await;
        let results = self
            .store
            .search_with_acl(&filter, query_embedding.as_slice(), limit)
            .await
            .unwrap_or_default();
        results
```

`get_document`:

```rust
        // Not implemented in VectorStore; return None for legacy callers.
        None
```

`get_all_documents`:

```rust
        Vec::new()
```

`clear_tenant`:

```rust
        let _ = self.store.clear_tenant(tenant_id).await;
```

`document_count`:

```rust
        self.store.document_count(tenant_id).await.unwrap_or(0)
```

---

## Task 4: Implement `PgVectorStore`

**Files:**
- Create: `backend/crates/infrastructure/src/vector/pg_vector_store.rs`
- Modify: `backend/crates/infrastructure/src/lib.rs`

- [ ] **Step 1: Create the pgvector implementation**

```rust
//! PostgreSQL/pgvector backend for the AI content index.

use anyhow::Result;
use rustshare_core::services::ai::indexing::{AclSearchFilter, IndexedDocument, NoteAclPayload, can_access};
use rustshare_core::services::ai::vector_store::VectorStore;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgVectorStore {
    pool: PgPool,
}

impl PgVectorStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl VectorStore for PgVectorStore {
    async fn upsert_chunk(
        &self,
        tenant_id: Uuid,
        chunk_id: Uuid,
        doc: &IndexedDocument,
        acl: &NoteAclPayload,
    ) -> Result<()> {
        let embedding = doc.embedding.as_slice();
        let read_acl: Vec<String> = acl.read_acl.clone();

        sqlx::query!(
            r#"
            INSERT INTO note_index_chunks (
                id, tenant_id, note_id, source_file_id, file_name, file_path,
                content, mime_type, owner_id, embedding, acl_hash, acl_version,
                read_acl, visibility, embedding_policy, indexed_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10::vector, $11, $12, $13, $14, $15, $16
            )
            ON CONFLICT (id) DO UPDATE SET
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
            chunk_id,
            tenant_id,
            acl.note_id,
            acl.source_file_id,
            doc.file_name,
            doc.file_path,
            doc.content,
            doc.mime_type,
            doc.owner_id,
            embedding as _,
            acl.acl_hash,
            acl.acl_version,
            &read_acl,
            acl.visibility,
            acl.embedding_policy,
            doc.indexed_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn search_with_acl(
        &self,
        filter: &AclSearchFilter,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(IndexedDocument, f32)>> {
        let caller_user_id = filter.caller_user_id;
        let limit = limit as i64;

        // Build the caller's principal list for a SQL ACL pre-filter.
        // Exact enforcement happens in Rust via can_access after the rows are fetched.
        let mut caller_principals = vec![format!("owner:{caller_user_id}"), format!("user:{caller_user_id}")];
        for group_id in &filter.caller_group_ids {
            caller_principals.push(format!("group:{group_id}"));
        }

        let rows = sqlx::query!(
            r#"
            SELECT
                id, note_id, source_file_id, file_name, file_path,
                content, mime_type, owner_id, embedding as "embedding!: Vec<f32>",
                acl_hash, acl_version, read_acl as "read_acl!: Vec<String>",
                visibility, embedding_policy, indexed_at,
                1 - (embedding <=> $1::vector) AS similarity
            FROM note_index_chunks
            WHERE tenant_id = $2
              AND embedding_policy = 'allowed'
              AND (
                  owner_id = $3
                  OR visibility = 'public'
                  OR read_acl && $4::text[]
              )
            ORDER BY embedding <=> $1::vector
            LIMIT $5
            "#,
            query_embedding as _,
            filter.tenant_id,
            caller_user_id,
            &caller_principals,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let acl = NoteAclPayload {
                tenant_id: filter.tenant_id,
                workspace_id: filter.tenant_id,
                note_id: row.note_id,
                source_file_id: row.source_file_id,
                source_folder_id: None,
                owner_id: row.owner_id,
                read_acl: row.read_acl,
                visibility: row.visibility,
                acl_hash: row.acl_hash,
                acl_version: row.acl_version,
                embedding_policy: row.embedding_policy,
            };

            if !can_access(&acl, filter) {
                continue;
            }

            let doc = IndexedDocument {
                file_id: row.id,
                file_name: row.file_name,
                file_path: row.file_path,
                content: row.content,
                embedding: row.embedding,
                mime_type: row.mime_type,
                owner_id: row.owner_id,
                tenant_id: filter.tenant_id,
                indexed_at: row.indexed_at,
                acl: Some(acl),
                chunk_id: row.id,
            };

            results.push((doc, row.similarity as f32));
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    async fn update_note_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        new_acl: &NoteAclPayload,
    ) -> Result<usize> {
        let read_acl = new_acl.read_acl.clone();
        let result = sqlx::query!(
            r#"
            UPDATE note_index_chunks
            SET acl_hash = $1,
                acl_version = $2,
                read_acl = $3,
                visibility = $4,
                embedding_policy = $5
            WHERE tenant_id = $6 AND note_id = $7
            "#,
            new_acl.acl_hash,
            new_acl.acl_version,
            &read_acl,
            new_acl.visibility,
            new_acl.embedding_policy,
            tenant_id,
            note_id
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }

    async fn remove_note_chunks(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
    ) -> Result<usize> {
        let result = sqlx::query!(
            "DELETE FROM note_index_chunks WHERE tenant_id = $1 AND note_id = $2",
            tenant_id,
            note_id
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }

    async fn remove_chunk(&self, tenant_id: Uuid, chunk_id: Uuid) -> Result<()> {
        sqlx::query!(
            "DELETE FROM note_index_chunks WHERE tenant_id = $1 AND id = $2",
            tenant_id,
            chunk_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn clear_tenant(&self, tenant_id: Uuid) -> Result<()> {
        sqlx::query!(
            "DELETE FROM note_index_chunks WHERE tenant_id = $1",
            tenant_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn document_count(&self, tenant_id: Uuid) -> Result<usize> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM note_index_chunks WHERE tenant_id = $1",
            tenant_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.unwrap_or(0) as usize)
    }
}
```

- [ ] **Step 2: Export the store**

Add to `backend/crates/infrastructure/src/lib.rs`:

```rust
pub mod vector;
pub use vector::pg_vector_store::PgVectorStore;
```

Create `backend/crates/infrastructure/src/vector/mod.rs`:

```rust
pub mod pg_vector_store;
```

- [ ] **Step 3: Prepare SQLx offline metadata**

Run against a running PostgreSQL instance:
```bash
cd backend
cargo sqlx prepare --workspace
```

---

## Task 5: Wire `PgVectorStore` in production and `InMemoryVectorStore` in tests

**Files:**
- Modify: `backend/server/src/bootstrap.rs` or `state.rs`
- Modify: all `ContentIndexer::new` call sites in tests

- [ ] **Step 1: Find where `ContentIndexer` is constructed**

Search for `ContentIndexer::new` and update the call to pass `Arc::new(PgVectorStore::new(pool.clone()))` in production.

- [ ] **Step 2: Update tests**

Replace existing `ContentIndexer::new(generator)` with:

```rust
let store = Arc::new(InMemoryVectorStore::new());
let indexer = ContentIndexer::new(generator, store);
```

- [ ] **Step 3: Remove dead HashMap fields**

Delete `ContentIndex` struct and the `indexes` field from `ContentIndexer` if no longer referenced.

---

## Task 6: Verify

- [ ] **Step 1: Compile**

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
git add backend/crates/core/src/services/ai/ \
       backend/crates/infrastructure/src/vector/ \
       backend/crates/infrastructure/src/lib.rs \
       backend/server/src/bootstrap.rs \
       backend/migrations/ \
       backend/.sqlx/
git commit -s -m "feat(notes): persist note vector index in PostgreSQL with pgvector"
```
