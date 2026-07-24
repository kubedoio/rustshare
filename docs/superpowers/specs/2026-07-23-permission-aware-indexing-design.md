# Permission-Aware AI Indexing Security Audit — Design

> **Status:** Awaiting implementation plan.  
> **Branch:** `security/permission-aware-indexing-audit`  
> **PR title:** `Audit and harden permission-aware AI indexing`

## Goal

Prove that RustShare never indexes, retrieves, returns, or sends content to an LLM for a principal who does not currently have permission to access that content. Resolve contradictions between documentation claims and the current implementation.

## Current contradictions found

| # | Location | Finding | Design response |
|---|----------|---------|-----------------|
| C1 | `backend/crates/core/src/services/ai_service.rs:327` `semantic_search` calls `self.indexer.search(tenant_id, ...)` | Retrieval uses a tenant-only vector scan followed by `PermissionResolver` post-filtering. The stored `read_acl` in `note_index_chunks` is never consulted, directly contradicting ADR-0020's requirement for database-level ACL pre-filtering. | Replace the call with `search_with_acl(&RetrievalPrincipal, ...)`; enforce ACL pre-filtering in both `InMemoryVectorStore` and `PgVectorStore`; add post-retrieval `can_access` re-check only as defense-in-depth. |
| C2 | `backend/server/src/services/note_service.rs` `delete_note` (`:1857`), `move_note` (`:1908`), `toggle_visibility` (`:2365`), `duplicate_note` (`:1968`); share/revoke handlers | None of these lifecycle events emit index updates or removals. `VectorStore::update_note_acl` and `remove_note_chunks` exist but have no production callers. | Wire every lifecycle event to the appropriate vector-store operation: `upsert` with fresh ACL projection, `update_note_acl`, or `remove_note_chunks`. Add revocation regression tests. |
| C3 | `backend/crates/core/src/services/ai_service.rs:407` `index_file`; `backend/server/src/services/file_service.rs` / `upload_service.rs` | `AiService::index_file` exists but is never called on file upload. Vault sync also does not index content. | Keep generic file indexing out of this PR unless a production caller exists. Document the gap; do not invent new indexing sources. The permission contract must be ready for them but implementation is limited to currently wired sources (Notes). |
| C4 | `backend/migrations/20260627000000_note_vectors.up.sql`; `backend/crates/infrastructure/src/vector/pg_vector_store.rs` `row_to_indexed_doc` | `note_index_chunks` has no `workspace_id` or `source_folder_id` columns. The store hardcodes `workspace_id = tenant_id` and `source_folder_id = None`. | Add `workspace_id` and `source_folder_id` columns to the migration (additive only). Update `PgVectorStore` to persist and read them. `workspace_id` must never equal `tenant_id` unless that identity is guaranteed by the domain. |
| C5 | `backend/server/src/services/note_service.rs` `build_acl_payload` | `embedding_policy` is hardcoded to `"allowed"`; external frontmatter/policy is ignored. | Read the object's embedding policy from the authoritative source and store it in the projection. `denied` objects must be removed from the index and never returned. |
| C6 | `backend/crates/core/src/services/ai/vector_store.rs`; `backend/tests/ai_permission_contract.rs` | Permission tests run only against `InMemoryVectorStore`. The pgvector path is effectively untested for ACL semantics. | Build a backend-agnostic contract test suite and run it against both `InMemoryVectorStore` and `PgVectorStore`. |
| C7 | `backend/crates/core/src/services/ai/indexing.rs:38-42` `NoteAclPayload::read_acl` comment | A `TODO(#118)` claims the owner principal is a placeholder. `PermissionResolver::resolve_read_principals` already exists and is used by `NoteService`, but `index_file` still synthesizes owner-only ACLs. | Remove stale comments where behavior is correct; centralize ACL projection so no path can synthesize owner-only ACLs for objects that may have broader permissions. |
| C8 | `backend/crates/core/src/services/ai/indexing.rs:84` `IndexedDocument` | ACL is `None` for legacy/non-note files, creating a tenant-wide fallback path. | Reject `None`/missing ACLs at retrieval; do not treat them as legacy tenant-wide content. |

## Non-negotiable security rules

- Permission enforcement before retrieval and before RAG context assembly.
- Tenant filtering alone is insufficient; owner-only filtering is insufficient; frontend filtering is not a security boundary.
- Indexed ACL metadata never grants more access than the source object.
- Revoked permissions stop retrieval without a full index rebuild.
- Cross-tenant access always fails closed.
- Missing, malformed, unknown, or stale ACL metadata fails closed.
- Public visibility must be explicit and tenant-safe.
- Deleted or trashed objects must not remain retrievable.
- Embedding-denied objects must not be indexed or retrieved.
- RustShare permissions remain authoritative.
- Every permission scenario has negative regression tests.
- Human review required before merge.

## Design

### Canonical permission model

Principals are typed, immutable identifiers:

```text
owner:<user_uuid>
user:<user_uuid>
group:<group_uuid>
workspace:<workspace_uuid>
public
```

Owner access is represented both by `owner:<uuid>` and by matching `owner_id`. The `read_acl` vector contains all principals that currently have read access, resolved from the authoritative `PermissionResolver`.

### Canonical ACL projection

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

- `acl_hash`: deterministic hash of the canonical ACL inputs.
- `acl_version`: monotonic counter incremented on any ACL change.
- `embedding_policy`: `allowed` or `denied`.

### Retrieval principal

```rust
pub struct RetrievalPrincipal {
    pub tenant_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub user_id: Uuid,
    pub group_ids: Vec<Uuid>,
}
```

### Secure retrieval order

1. Resolve authenticated caller.
2. Resolve tenant and workspace scope.
3. Resolve current groups and other principals.
4. Query index with tenant and ACL constraints.
5. Reject missing, malformed, or stale ACL entries.
6. Verify current source-object permission where required.
7. Load only authorized source content.
8. Build the LLM context.

### Legacy ACL-less content

Choose strategy (1): reject legacy ACL-less documents until reindexed. Tenant-wide fallback is not acceptable. Documents with `read_acl IS NULL` or missing `acl_version` will be rejected at retrieval time.

### ACL change propagation

Every relevant lifecycle event causes one of:

- immediate ACL projection update;
- object reindexing;
- index removal;
- ACL version invalidation followed by fail-closed retrieval.

Events to wire: direct share creation/removal, group share creation/removal, group membership addition/removal, visibility changes, ownership changes, folder moves, parent-folder permission changes, trash, restore, delete, embedding-policy changes.

### Store parity

The same semantic contract must pass for both `InMemoryVectorStore` and `PgVectorStore`. Tests must drive both backends.

### Operational visibility

Structured metrics/logs for:

- index writes rejected due to unresolved ACLs;
- search results rejected due to stale ACLs;
- ACL update failures;
- index removal failures;
- reindex retries;
- permission verification failures;
- legacy ACL-less chunks encountered;
- malformed ACL payloads.

No logging of document content, embeddings, private filenames, tokens, complete prompts, or user emails.

## Files likely to change

- `backend/crates/core/src/services/ai/indexing.rs`
- `backend/crates/core/src/services/ai/vector_store.rs`
- `backend/crates/core/src/services/ai/mod.rs`
- `backend/crates/core/src/services/ai_service.rs`
- `backend/crates/infrastructure/src/vector/pg_vector_store.rs`
- `backend/crates/core/src/services/permission_resolver.rs`
- `backend/server/src/services/note_service.rs`
- `backend/server/src/handlers/ai.rs`
- `backend/server/src/bootstrap.rs`
- `backend/migrations/20260627000000_note_vectors.up.sql`
- `backend/tests/ai_permission_contract.rs`
- `docs/audits/2026-permission-aware-indexing-audit.md`
- `docs/audits/2026-permission-aware-indexing-test-matrix.md`
- `docs/contracts/permission-aware-indexing-contract.md`
- `docs/audits/2026-permission-aware-indexing-result.md`
- `CHANGELOG.md`

## Validation

- `cargo fmt --all --check`
- `SQLX_OFFLINE=true cargo check --workspace --all-features`
- `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `SQLX_OFFLINE=true cargo test --workspace --all-features --lib`
- `SQLX_OFFLINE=true cargo test --workspace --all-features`
- `cargo sqlx prepare --workspace --check`
- `cargo deny --all-features check`
- Frontend checks if contracts change.

## Rollback considerations

- Any schema migration must be reversible or additive only.
- ACL projection changes must not remove existing authorized access; they may only tighten or correctly represent current permissions.
- New retrieval filtering should be deployable without reindexing because legacy content fails closed.
