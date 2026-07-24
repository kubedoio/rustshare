# Permission-Aware AI Indexing Security Audit

> **Status:** Resolved by `security/permission-aware-indexing-audit` — see [`2026-permission-aware-indexing-result.md`](2026-permission-aware-indexing-result.md).  
> **Branch:** `security/permission-aware-indexing-audit`  
> **Date:** 2026-07-23  
> **Goal:** Prove that RustShare never returns indexed content to a principal who lacks permission, and document the gaps between the current implementation and that goal.

This audit maps the current AI indexing and retrieval pipeline, enumerates contradictions found between the design spec (`docs/superpowers/specs/2026-07-23-permission-aware-indexing-design.md`) and the code, and defines the target secure pipeline. Implementation changes are intentionally left for subsequent tasks.

## Current pipeline

The code paths below are the ones exercised for notes today. File indexing exists as an API on `AiService` but has no production caller.

1. **Note creation / update / rename** — `backend/server/src/services/note_service.rs`
   - `create_note` (line 940) and `save_note` (line 1586) call `emit_index_note` (line 709), which resolves the current read principals via `PermissionResolver::resolve_read_principals` and builds a `NoteAclPayload` via `NoteService::build_acl_payload` (line 688).
   - `rename_note` (line 1743) also calls `emit_index_note`, re-indexing with the latest metadata.
2. **ACL payload construction** — `NoteService::build_acl_payload` (line 688)
   - Projects the current `File`, `NoteMetadata`, tenant ID, workspace ID (currently hardcoded to `tenant_id`), and resolved `read_acl` into `NoteAclPayload`. It also hardcodes `embedding_policy = "allowed"`.
3. **Index sink / content indexer** — `backend/crates/core/src/services/ai/indexing.rs`
   - `ContentIndexer::index_note` (line 187) strips frontmatter, generates an embedding, wraps the content in `IndexedDocument` with `acl: Some(acl)`, and calls `VectorStore::upsert_chunk`.
   - `ContentIndexer::index_file` (line 126) is exposed on `AiService` but synthesizes an owner-only ACL and is not wired to uploads or vault sync.
4. **Vector store backends**
   - `InMemoryVectorStore` — `backend/crates/core/src/services/ai/vector_store.rs` (line 68). Used by tests and local development. Supports ACL pre-filtering (`search_with_acl`) and tenant-only search (`search`).
   - `PgVectorStore` — `backend/crates/infrastructure/src/vector/pg_vector_store.rs` (line 15). Persists chunks in `note_index_chunks`. `row_to_indexed_doc` (line 50) hardcodes `workspace_id = tenant_id` and `source_folder_id = None` because the table does not store those columns.
5. **Migration/schema** — `backend/migrations/20260627000000_note_vectors.up.sql`
   - Creates `note_index_chunks` with `tenant_id`, `note_id`, `source_file_id`, `read_acl`, `visibility`, `embedding_policy`, `acl_hash`, `acl_version`, etc. No `workspace_id` or `source_folder_id` columns.
6. **Semantic search entry point** — `backend/crates/core/src/services/ai_service.rs`
   - `AiService::semantic_search` (line 159) validates the query, then calls `ContentIndexer::search(tenant_id, query, limit * 3)` (line 178), i.e. the **tenant-only** path.
   - It post-filters each returned document by calling `PermissionResolver::resolve_permission(user_id, tenant_id, Resource::File(document.file_id))` (line 197). The stored `read_acl` from the chunk is never used as the primary access gate.
7. **RAG Q&A** — `AiService::ask_question` (line 327)
   - Delegates to `semantic_search`, inheriting the same tenant-only retrieval + post-filter behavior.
8. **Lifecycle events that do NOT update the index**
   - `NoteService::delete_note` (line 1857) removes files, sidecars, and bundle folders but never calls `VectorStore::remove_note_chunks` or `ContentIndexer::remove_file`.
   - `NoteService::move_note` (line 1908) moves the bundle/folder and updates the sidecar but does not reindex or update `source_folder_id`.
   - `NoteService::toggle_visibility` (line 2365) flips `NoteVisibility` and updates the sidecar but never updates the indexed `visibility`/`read_acl`.
   - `NoteService::duplicate_note` (line 1968) creates a new copy but does not emit an index update for the new note (it does call `emit_index_note` only for `rename_note`, `save_note`, and `create_note`).
   - `backend/server/src/handlers/shares.rs` `create_public_file_share` (line 53), `create_public_folder_share` (line 120), and `revoke_share` (line 357) call `state.share_service` but never propagate share creation/revocation to `VectorStore::update_note_acl` or `remove_note_chunks`.
9. **ACL projection helpers**
   - `PermissionResolver::resolve_read_principals` — `backend/crates/core/src/services/permission_resolver.rs:682` is the authoritative source for read principals and is used by `NoteService`.
   - `can_access` — `backend/crates/core/src/services/ai/indexing.rs` (line 403) performs Rust-side ACL matching for `search_with_acl`, including `embedding_policy`, `acl_version`, owner, direct user, group, and public checks.

## Contradictions table

| ID | Location | Finding | Design response |
|---|---|---|---|
| C1 | `backend/crates/core/src/services/ai_service.rs:178` `AiService::semantic_search` | Retrieval uses `self.indexer.search(tenant_id, ...)` — a tenant-only vector scan — and then post-filters with `PermissionResolver`. The stored `read_acl` in `note_index_chunks` is never consulted by the search path, contradicting `docs/adr/0020-okf-notes-reconciliation-and-rag-safety.md`'s requirement for database-level ACL pre-filtering. | Replace with `ContentIndexer::search_with_acl(&RetrievalPrincipal, ...)`. Enforce ACL pre-filtering in both `InMemoryVectorStore` and `PgVectorStore`; keep `PermissionResolver` as defense-in-depth only. |
| C2 | `backend/server/src/services/note_service.rs` `delete_note` (`:1857`), `move_note` (`:1908`), `toggle_visibility` (`:2365`), `duplicate_note` (`:1968`); `backend/server/src/handlers/shares.rs` `create_public_file_share` (`:53`), `create_public_folder_share` (`:120`), `revoke_share` (`:357`) | None of these lifecycle events emit index updates or removals. `VectorStore::update_note_acl`, `remove_note_chunks`, and `ContentIndexer::remove_file` exist but have no production callers from note/share handlers. | Wire every lifecycle event to the appropriate vector-store operation: `upsert` with fresh ACL projection, `update_note_acl`, or `remove_note_chunks`. Add revocation regression tests. |
| C3 | `backend/crates/core/src/services/ai_service.rs:407` `AiService::index_file`; `backend/crates/core/src/services/ai/indexing.rs:126` `ContentIndexer::index_file` | `AiService::index_file` exists but is never called on file upload. Vault sync also does not index content. | Keep generic file indexing out of this PR unless a production caller exists. Document the gap; do not invent new indexing sources. The permission contract must be ready for them, but implementation is limited to currently wired sources (notes). |
| C4 | `backend/migrations/20260627000000_note_vectors.up.sql`; `backend/crates/infrastructure/src/vector/pg_vector_store.rs:50` `row_to_indexed_doc` | `note_index_chunks` has no `workspace_id` or `source_folder_id` columns. The store hardcodes `workspace_id = tenant_id` and `source_folder_id = None`. | Add `workspace_id` and `source_folder_id` columns to the migration (additive only). Update `PgVectorStore` to persist and read them. `workspace_id` must never equal `tenant_id` unless that identity is guaranteed by the domain. |
| C5 | `backend/server/src/services/note_service.rs:705` `NoteService::build_acl_payload` | `embedding_policy` is hardcoded to `"allowed"`; external frontmatter/policy is ignored. | Read the object's embedding policy from the authoritative source and store it in the projection. `denied` objects must be removed from the index and never returned. |
| C6 | `backend/crates/core/src/services/ai/vector_store.rs`; `backend/crates/core/src/services/ai/indexing.rs` tests | Permission tests run only against `InMemoryVectorStore`. The pgvector path is effectively untested for ACL semantics. | Build a backend-agnostic contract test suite and run it against both `InMemoryVectorStore` and `PgVectorStore`. |
| C7 | `backend/crates/core/src/services/ai/indexing.rs:38-42` `NoteAclPayload::read_acl` comment | A `TODO(#118)` claims the owner principal is a placeholder. `PermissionResolver::resolve_read_principals` already exists and is used by `NoteService`, but `ContentIndexer::index_file` still synthesizes owner-only ACLs. | Remove stale comments where behavior is correct; centralize ACL projection so no path can synthesize owner-only ACLs for objects that may have broader permissions. |
| C8 | `backend/crates/core/src/services/ai/indexing.rs:84` `IndexedDocument` | `acl: Option<NoteAclPayload>` allows `None`, which `InMemoryVectorStore::search_with_acl` treats as `true` (legacy tenant-wide fallback). | Reject `None`/missing ACLs at retrieval; do not treat them as legacy tenant-wide content. |

## Target pipeline

The secure retrieval pipeline, aligned with the design spec, is:

1. **Resolve authenticated caller** to a `RetrievalPrincipal` (`tenant_id`, optional `workspace_id`, `user_id`, `group_ids`).
2. **Resolve tenant and workspace scope** from the request context; cross-tenant access fails closed.
3. **Query the vector store with tenant + ACL constraints** via `VectorStore::search_with_acl` / `ContentIndexer::search_with_acl`. The store must filter by:
   - `tenant_id` equality;
   - `embedding_policy = 'allowed'`;
   - `read_acl` overlap with caller principals (`owner:<id>`, `user:<id>`, `group:<id>`, `workspace:<id>`, `public`);
   - optional `workspace_id` scoping;
   - optional `min_acl_versions` staleness rejection.
4. **Reject missing, malformed, or stale ACL entries** at retrieval time. Legacy ACL-less chunks (`acl IS NULL` / `read_acl` empty with no public visibility) must fail closed, not fall back to tenant-wide access.
5. **Post-check with `PermissionResolver`** as defense-in-depth. For each candidate chunk, verify the caller still has current read access to the source object. Any mismatch drops the result and is logged/metric'd.
6. **Return only authorized results** to the caller, then build RAG context from those results.

### Non-negotiable rules from the design spec

- Permission enforcement happens before retrieval and before RAG context assembly.
- Tenant filtering alone is insufficient; owner-only filtering is insufficient; frontend filtering is not a security boundary.
- Indexed ACL metadata never grants more access than the source object.
- Revoked permissions stop retrieval without a full index rebuild.
- Cross-tenant access always fails closed.
- Missing, malformed, unknown, or stale ACL metadata fails closed.
- Public visibility must be explicit and tenant-scoped.
- Deleted or trashed objects must not remain retrievable.
- Embedding-denied objects must not be indexed or retrieved.
- RustShare permissions remain authoritative.
- Every permission scenario has negative regression tests.

## Security gaps status

| Gap | Current state | Required fix | Priority | Status |
|---|---|---|---|---|
| Search does not use ACL pre-filtering | `AiService::semantic_search` calls tenant-only `indexer.search`. | Switch to `search_with_acl` with `RetrievalPrincipal`. | Critical | ✅ Resolved |
| `read_acl` not consulted at retrieval | Stored ACL is ignored by the production search path. | Enforce `read_acl` in vector store query + Rust-side `can_access`. | Critical | ✅ Resolved |
| Lifecycle events do not propagate ACL changes | `delete_note`, `move_note`, `toggle_visibility`, `duplicate_note`, share create/revoke never update or remove chunks. | Wire every event to `upsert`/`update_note_acl`/`remove_note_chunks`. | Critical | ✅ Resolved |
| `workspace_id` and `source_folder_id` are fabricated | `PgVectorStore::row_to_indexed_doc` hardcodes both; migration lacks columns. | Add columns and persist real values. | High | ✅ Resolved |
| `embedding_policy` hardcoded to `"allowed"` | `NoteService::build_acl_payload` ignores frontmatter policy. | Read authoritative policy; `denied` removes from index. | High | ✅ Resolved |
| pgvector path untested for ACL semantics | Tests only exercise `InMemoryVectorStore`. | Backend-agnostic contract tests against both stores. | High | ✅ Resolved |
| Stale TODO about owner-only placeholder | `NoteAclPayload::read_acl` comment references `TODO(#118)`. | Remove stale comment; prevent owner-only synthesis in `index_file`. | Medium | ✅ Resolved |
| `IndexedDocument::acl: Option<...>` allows tenant-wide fallback | `None` ACLs are treated as accessible in `InMemoryVectorStore::search_with_acl`. | Reject `None` ACLs at retrieval; fail closed. | Critical | ✅ Resolved |
| File indexing is un-wired | `AiService::index_file` / `ContentIndexer::index_file` have no production callers. | Keep documented gap; do not expand scope until wired. | Low (out of scope) | ⚠️ Deferred |

---

*Document produced as Task 1 of the `security/permission-aware-indexing-audit` branch. No code changes were made in this commit.*
