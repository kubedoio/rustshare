# Permission-Aware Indexing Test Matrix

> **Status:** Task 3 of the permission-aware AI indexing security audit.  
> **Branch:** `security/permission-aware-indexing-audit`  
> **Date:** 2026-07-23  
> **Applies to:** All vector stores, indexers, and retrieval paths that feed RustShare content to AI models.  
> **Contract:** [`docs/contracts/permission-aware-indexing-contract.md`](../contracts/permission-aware-indexing-contract.md)

This document enumerates the concrete positive and negative test scenarios that must pass for RustShare's AI indexing and retrieval to satisfy the permission-aware indexing contract. Every scenario must be covered by automated tests against both supported vector-store backends.

## Retrieval matrix

| Scenario | Expected |
|----------|----------|
| Owner searches own private note | Allowed |
| Unrelated user in same tenant searches private note | Denied |
| User with direct read share searches note | Allowed |
| User after direct share revocation | Denied |
| Group member searches group-shared note | Allowed |
| User removed from group | Denied |
| User in different group | Denied |
| Workspace-visible note, non-owner workspace member | Allowed |
| Workspace-visible note, other tenant | Denied |
| Public note through supported public context | Allowed when caller is unauthenticated or any principal and accesses the note through an explicit public share context within the note's tenant |
| Private note with missing ACL payload | Denied |
| Stale ACL version (lower than required) | Denied |
| Trashed note | Denied |
| Deleted note | Denied and removed from index |
| Embedding-denied note | Not indexed and not retrieved |
| Same user UUID represented in another tenant | Denied |
| Folder-inherited share | Allowed only while inheritance is active |
| Expired share | Denied |
| Disabled user | Denied |
| Index-store error | No unauthorized fallback |
| Permission-resolver error | Denied |
| Group-resolver error | Denied |
| Search result with malformed ACL JSON | Denied |

## Revocation matrix

| Event | Initial state | After event | Expected |
|-------|--------------|-------------|----------|
| Revoke direct share | user principal in ACL | principal removed | Not found |
| Revoke group share | group principal in ACL | principal removed | Not found |
| Remove group member | group principal in caller groups | member removed | Not found |
| Move to private folder | inherited access | no inherited access | Not found |
| Public -> private | public visibility | private | Not found (non-owner) |
| Trash/delete | allowed | removed/stale | Not found |

## Store parity

The permission-aware indexing contract is backend-agnostic. The same acceptance criteria and the same test matrix must pass for both implementations without behavioral divergence:

- `InMemoryVectorStore` — used in unit tests and local development.
- `PgVectorStore` — used in production with `note_index_chunks` persisted to PostgreSQL.

Test harness requirements:

- Parameterize every retrieval and revocation scenario on the vector-store implementation.
- Assert identical allow/deny outcomes and identical failure-mode behavior for both backends.
- Reject any store-specific fallback, normalization, or permissive default (for example, treating a missing `read_acl` as tenant-wide access).
- Validate that `acl_version`, `acl_hash`, `tenant_id`, `workspace_id`, `owner_id`, `read_principals`, `visibility`, and `embedding_policy` are stored and enforced consistently in both backends.

## Failure-mode matrix

| Failure | Retrieval behavior | Notes |
|---------|--------------------|-------|
| Index-store error | No unauthorized fallback | A failed vector-store query must return an error or empty results; it must not silently return tenant-wide or unfiltered results. (see Retrieval matrix) |
| Permission-resolver error | Denied | If the authoritative `PermissionResolver` cannot confirm access, the retrieval fails closed. (see Retrieval matrix) |
| Group-resolver error | Denied | If group membership cannot be resolved, the caller is treated as not a member of any group principal in the ACL. (see Retrieval matrix) |
| Malformed ACL JSON | Denied | An unparseable ACL projection in a returned chunk is rejected; the chunk is not returned and should be flagged for reindexing. (see Retrieval matrix) |
| Missing ACL payload | Denied | Legacy or partially indexed chunks with `read_acl IS NULL` or an absent ACL projection are rejected; no tenant-wide fallback is permitted. (see Retrieval matrix) |
| Stale ACL version | Denied | When the stored `acl_version` does not match the source object's current `acl_version`, the chunk is rejected until the ACL propagation path refreshes or removes it. (see Retrieval matrix) |

## Mapping to contract guarantees

Each matrix row traces directly to a rule in the contract:

- Tenant isolation: rows for "other tenant" and "same user UUID represented in another tenant" enforce contract rule 6.
- Full ACL enforcement: rows for owner, direct share, group share, workspace, and public visibility enforce contract rules 1, 2, and 7.
- Revocation without rebuild: the revocation matrix and stale-ACL row enforce contract rules 4 and 5.
- Embedding policy: the "embedding-denied note" row enforces contract rule 3.
- Lifecycle: the trashed/deleted rows enforce contract rule 8.
- Failure modes: the final rows enforce the legacy-content and error-handling requirements in the contract's Legacy Content Handling and Enforcement Rules sections.

## Test coverage checklist

- [ ] Retrieval matrix: all 23 scenarios implemented as automated tests.
- [ ] Revocation matrix: all 6 events implemented as automated tests.
- [ ] Failure-mode matrix: all 6 failure modes implemented as automated tests.
- [ ] Store parity: every scenario runs against both `InMemoryVectorStore` and `PgVectorStore`.
- [ ] No test bypasses the ACL projection or falls back to tenant-only filtering.
- [ ] Negative outcomes are asserted explicitly (result is empty or error is returned), not merely implied by absence.
