# Permission-Aware AI Indexing Security Audit — Result

> **Branch:** `security/permission-aware-indexing-audit`  
> **Final commit:** `29fdd5c7`  
> **Status:** Implementation complete; awaiting human security review before merge.

## Summary

This phase audited and hardened the RustShare AI indexing and retrieval pipeline so that content is never indexed, retrieved, returned, or sent to an LLM for a principal who does not currently have permission to access it.

PR 175 and PR 176 completed the stabilization milestone. This security-focused branch resolves the contradictions found between documentation claims and the implementation, hardens the index write and retrieval paths, wires ACL change propagation, and adds backend-agnostic permission contract tests.

## What was implemented

### Canonical ACL projection

- Added typed `IndexAclProjection`, `RetrievalPrincipal`, `IndexPrincipal`, `IndexVisibility`, and `EmbeddingPolicy` in `rustshare-core`.
- `IndexAclProjection` carries `tenant_id`, `workspace_id`, `object_id`, `source_folder_id`, `owner_id`, `read_principals`, `visibility`, `acl_hash`, `acl_version`, and `embedding_policy`.
- `NoteService::build_acl_payload` now returns `Result<IndexAclProjection, NoteError>` and:
  - parses `embedding_policy` from `NoteMetadata`, defaulting invalid values to `EmbeddingPolicy::Denied` (fail closed);
  - parses every resolved read principal and returns an error on malformed principals;
  - populates `workspace_id` from `File::workspace_id()` and `source_folder_id` from `file.parent_folder_id`.

### Index write path

- `ContentIndexer::index_note` and `ContentIndexer::index_file` both accept `IndexAclProjection`.
- `EmbeddingPolicy::Denied` causes existing chunks to be removed and no new chunks to be inserted.
- A shared conversion helper maps the typed projection to the storage-level `NoteAclPayload` without synthesizing owner-only ACLs.

### Retrieval hardening

- `AiService::semantic_search` now uses `ContentIndexer::search_with_acl(&RetrievalPrincipal, ...)` instead of the tenant-only `search` path.
- Both `InMemoryVectorStore` and `PgVectorStore` enforce:
  - tenant scoping;
  - `embedding_policy = 'allowed'` pre-filter;
  - caller-principal overlap (`owner`, `user`, `group`, `workspace`, `public`);
  - post-retrieval `can_access` verification;
  - rejection of missing, malformed, and stale ACL entries.
- Legacy ACL-less chunks fail closed.

### ACL change propagation

- `NoteService` now emits index updates on:
  - `create_note`, `save_note`, `rename_note` (existing);
  - `move_note`, `duplicate_note`, `toggle_visibility`;
  - `delete_note` removes the note from the index.
- Share handlers call `NoteService::refresh_note_index_acl` after create/revoke/permission-update events for user shares, group shares, and group share permission changes.
- `ShareService::get_share_by_id` was added so handlers can resolve the affected file before a share is revoked.

### Schema

- Migration `20260723000000_note_index_acl_columns` adds `workspace_id` and `source_folder_id` to `note_index_chunks`.
- `PgVectorStore` persists and reads both columns.

### Tests

- `backend/tests/ai_vector_store_permission_contract.rs` runs the same semantic contract against `InMemoryVectorStore` and `PgVectorStore`.
- Covered scenarios: owner access, direct share, group share, workspace visibility, public, cross-tenant denial, embedding-denied, missing ACL, malformed ACL, stale ACL version, and share revocation without index rebuild.
- Existing `ai_permission_contract.rs` service-level tests continue to pass.

### Operational visibility

- Structured `tracing::error!` and `tracing::warn!` logs for:
  - index search/update/removal failures;
  - malformed or missing ACL chunks at retrieval;
  - ACL principal resolution and projection failures in `NoteService`;
  - lifecycle-hook refresh failures in share handlers.
- No document content, embeddings, or private filenames are logged.

## Validation commands and results

```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo check --workspace --all-features
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
DATABASE_URL="postgresql://rustshare:1f7b27220d83a11de6bca8b63c0ca491a3001c0c73471eda@127.0.0.1:5432/rustshare" \
    SQLX_OFFLINE=true cargo test --workspace --all-features --lib
DATABASE_URL="postgresql://rustshare:1f7b27220d83a11de6bca8b63c0ca491a3001c0c73471eda@127.0.0.1:5432/rustshare" \
    SQLX_OFFLINE=true cargo test --workspace --all-features
DATABASE_URL="postgresql://rustshare:1f7b27220d83a11de6bca8b63c0ca491a3001c0c73471eda@127.0.0.1:5432/rustshare" \
    cargo sqlx prepare --workspace --check
cargo deny --all-features check
```

Results:

- `cargo fmt --all --check` ✅
- `cargo check --workspace --all-features` ✅
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-features --lib` — 302 passed, 9 ignored ✅ (requires `DATABASE_URL` for auth handler tests)
- `cargo test --workspace --all-features` — all runnable tests passed; 45 notes tests ignored because they require database + S3; `openapi_spec_is_fresh` initially failed and was resolved by regenerating the OpenAPI contract ✅
- `cargo sqlx prepare --workspace --check` ✅
- `cargo deny --all-features check` ✅ (passes; emits pre-existing duplicate `nom` version warnings, no advisories/bans/licenses/sources errors)

## Contradictions resolved

| ID | Finding | Resolution |
|----|---------|------------|
| C1 | `semantic_search` used tenant-only scan + post-filter | Switched to `search_with_acl` with `RetrievalPrincipal` |
| C2 | Lifecycle events did not update the index | Wired `move_note`, `toggle_visibility`, `duplicate_note`, `delete_note`, and share handlers to index updates/removals |
| C4 | `workspace_id`/`source_folder_id` were fabricated | Added migration columns and persist real values |
| C5 | `embedding_policy` hardcoded to `"allowed"` | Read from `NoteMetadata`, fail closed on invalid values |
| C6 | pgvector path untested for ACL semantics | Added backend-agnostic contract tests |
| C7 | Stale TODO about placeholder owner principal | Removed stale comment; `index_file` now requires full projection |
| C8 | `None` ACLs fell back to tenant-wide access | Reject missing ACLs at retrieval |

C3 (generic file indexing has no production caller) remains a documented gap and is out of scope for this PR.

## Security-impact statement

This PR changes the permission boundary for AI indexing and retrieval. Unauthorized content is filtered before retrieval and before RAG context assembly. Missing, stale, malformed, and cross-tenant ACL data fail closed. Share revocation removes indexed access without requiring a full index rebuild.

## Compatibility statement

- The schema migration is additive only and reversible.
- ACL projection changes do not remove existing authorized access; they tighten or correctly represent current permissions.
- Legacy content with missing ACL data fails closed until reindexed, which is safe by design.

## Deferred risks

- Generic file indexing (`AiService::index_file`) is not wired to uploads or vault sync; it accepts `IndexAclProjection` but has no production caller.
- Folder-inherited permissions and explicit denial are modeled in the contract but rely on the authoritative `PermissionResolver` to produce correct `read_principals`.
- Group membership changes take effect at retrieval time; object `acl_version` is not incremented on group membership changes.
- Disabled users and expired shares are enforced by the `PermissionResolver` post-check and service-level tests, not by a dedicated vector-store filter.

## Rollback considerations

- Revert the branch. The migration is additive, so existing data remains valid.
- If the new retrieval filtering causes unexpected empty results, investigate whether source objects have stale or missing ACL metadata that needs reindexing.
