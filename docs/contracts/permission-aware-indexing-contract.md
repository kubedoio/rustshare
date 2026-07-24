# Contract: Permission-Aware AI Indexing ACL Model

> **Status:** Active contract for the permission-aware AI indexing security audit.  
> **Branch:** `security/permission-aware-indexing-audit`  
> **Applies to:** All vector stores, indexers, and retrieval paths that feed RustShare content to AI models.

## Summary

This document defines the canonical permission model for AI indexing and retrieval in RustShare. It is the authoritative contract that all index implementations and retrieval paths must satisfy.

The fundamental guarantee is:

> RustShare never indexes, retrieves, returns, or sends content to an LLM for a principal who does not currently have permission to access that content.

## Principals

Principals are typed, immutable identifiers. They must be derived from stable UUIDs. Display names, usernames, emails, and other mutable labels must never be used as ACL identities.

```text
owner:<user_uuid>
user:<user_uuid>
group:<group_uuid>
workspace:<workspace_uuid>
public
```

| Principal | Meaning |
|---|---|
| `owner:<user_uuid>` | The object's owner. Owner access is represented both by this principal and by matching the object's `owner_id`. |
| `user:<user_uuid>` | A specific user granted read access, for example through a direct share. |
| `group:<group_uuid>` | A group granted read access. Membership is resolved at retrieval time; the index stores the group principal itself. |
| `workspace:<workspace_uuid>` | A workspace-level grant. The exact semantics of workspace membership are resolved by the authoritative `PermissionResolver`. |
| `public` | Explicit public visibility. Public access is always tenant-scoped and must not allow cross-tenant retrieval. |

## ACL Projection

The canonical ACL projection is the only representation of an object's access control that may be written into an AI index.

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

| Field | Definition |
|---|---|
| `tenant_id` | The object's tenant. Cross-tenant access always fails closed. |
| `workspace_id` | The object's workspace. Must reflect the actual workspace of the object; it must never be silently defaulted to `tenant_id`. |
| `object_id` | The unique identifier of the source object (note, file, etc.). |
| `owner_id` | The owner's user UUID. `owner:<owner_id>` is always present in `read_principals` for allowed objects. |
| `read_principals` | Complete set of principals that currently have read access, resolved from the authoritative `PermissionResolver`. |
| `visibility` | The object's visibility level. `public` visibility must be explicit and tenant-scoped. |
| `acl_hash` | Deterministic hash of the canonical ACL inputs used to detect semantic changes. |
| `acl_version` | Monotonic counter incremented on any ACL change. |
| `embedding_policy` | Either `allowed` or `denied`. Denied objects must never be indexed or returned. |

### `acl_hash` calculation

`acl_hash` is a deterministic hash of the canonical ACL inputs, computed in a stable order:

1. `tenant_id`
2. `workspace_id`
3. `object_id`
4. `owner_id`
5. `visibility` (canonical string representation)
6. `embedding_policy` (canonical string representation)
7. `read_principals` sorted lexicographically after canonical serialization

The exact hash algorithm is an implementation detail, but it must be deterministic, stable, and collision-resistant for the expected principal cardinality.

## Retrieval Principal

At retrieval time the caller is represented as:

```rust
pub struct RetrievalPrincipal {
    pub tenant_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub user_id: Uuid,
    pub group_ids: Vec<Uuid>,
}
```

| Field | Definition |
|---|---|
| `tenant_id` | The caller's tenant. Must match the indexed object's `tenant_id`. |
| `workspace_id` | The workspace scope of the request, if specified. Used for workspace-scoped filtering. |
| `user_id` | The caller's user UUID. Matches `owner:<user_id>` and `user:<user_id>` principals. |
| `group_ids` | The caller's current group memberships. Matches `group:<group_uuid>` principals. |

Group membership is resolved at retrieval time. Group changes do not require reindexing an object because the index stores `group:<group_uuid>` principals and the caller supplies their current `group_ids`.

## Enforcement Rules

1. **Tenant filtering alone is insufficient.** Every retrieval must enforce the full ACL projection, not only the `tenant_id`.
2. **Owner-only filtering is insufficient.** Retrieval must honor all `read_principals`, including users, groups, workspace grants, and explicit public visibility.
3. **`embedding_policy = denied` implies never indexed or returned.** A denied object must be removed from the index and must not be returned by any retrieval path.
4. **Missing, malformed, or stale ACL metadata fails closed.** Any chunk that lacks an ACL projection, has an unparseable projection, or whose `acl_version` does not match the current source object must be rejected.
5. **Revoked permissions stop retrieval without a rebuild.** When a share is revoked or a visibility change removes a principal, the object's index entry is updated or removed; until then the stale entry must fail closed.
6. **Cross-tenant access always fails closed.** A caller's `tenant_id` must exactly match the object's `tenant_id`. No tenant fallback, wildcard, or implicit cross-tenant grant is permitted.
7. **Public visibility must be explicit and tenant-scoped.** `public` is a principal in the ACL projection and is only valid within the object's tenant. Public chunks must still be filtered by `tenant_id`.
8. **Deleted or trashed objects must not be retrievable.** Lifecycle events that delete or trash an object must remove the corresponding index chunks or mark them in a way that causes fail-closed retrieval.

## ACL Versioning Semantics

### What increments `acl_version`

The following changes increment the object's `acl_version`:

- Any change to `read_principals` (direct share creation or revocation, group share creation or revocation).
- Any change to `visibility` (for example toggling public visibility).
- Any change to `owner_id` (ownership transfer).
- Any change to `workspace_id` (folder moves or workspace reassignment).
- Any change to `embedding_policy` (from `allowed` to `denied` or vice versa).

### What does not increment `acl_version`

- Group membership changes do not directly change an object's `acl_version`. The index stores `group:<group_uuid>` principals, and the caller's `group_ids` are resolved at retrieval time, so removal is effective immediately.

### Detecting stale chunks

When an index chunk is returned, its stored `acl_version` is compared against the current source object's `acl_version`. If they differ, the chunk is stale and the retrieval fails closed. The chunk must then be refreshed or removed by the ACL change propagation path.

### ACL update failures

Failed ACL projection updates are retried according to the existing job policy. Until a failed update succeeds, the stale chunk must fail closed at retrieval time.

## Legacy Content Handling

Reject ACL-less chunks until reindexed.

- Any indexed chunk with `read_acl IS NULL`, a missing `acl_version`, a missing `acl_hash`, or an unparseable ACL projection must be rejected at retrieval time.
- Tenant-wide fallback for legacy content is not permitted.
- Legacy chunks are not treated as implicitly public or implicitly accessible to the entire tenant.
- The correct remediation is to reindex the source object, producing a complete `IndexAclProjection`.

## Acceptance Criteria

- [x] All indexed objects carry a complete `IndexAclProjection` before being written or returned.
- [x] Retrieval enforces the full ACL projection for every candidate chunk.
- [x] `embedding_policy = denied` objects are removed from the index and never returned.
- [x] Missing, malformed, and stale ACL projections fail closed.
- [x] Cross-tenant retrieval always fails closed.
- [x] Deleted and trashed objects are not retrievable.
- [x] The contract is verified by backend-agnostic tests running against both `InMemoryVectorStore` and `PgVectorStore`.

Verification: `backend/tests/ai_vector_store_permission_contract.rs` and `backend/tests/ai_permission_contract.rs`; see `docs/audits/2026-permission-aware-indexing-result.md`.
