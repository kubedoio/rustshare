# ResourceRef and Source Authorization — Implementation Notes (#211)

Status: Implemented (v1alpha1)  
Date: 2026-08-09  
Related: ADR-0030, ADR-0032, `docs/specs/resource-ref-authorization-v1alpha1.md`, #210

This document records how #211 was implemented, where the canonical
`PrincipalContext` is constructed, how Elembra Files became the first owner
adapter, and which authorization duplications/deferred items were identified.

## 1. Contract location

The shared contract lives in a new pure crate:

`backend/crates/resource-auth` (package `rustshare-resource-auth`)

It depends only on `rustshare-core` (identity newtypes: `ApplicationId`,
`TenantId`, `WorkspaceId`, `PrincipalId`, `ActionCapability`, `CorrelationId`)
and has no I/O. Modules:

- `resource_ref.rs` — `ResourceRef`, canonical `elembra://` URI, validation.
- `principal.rs` — `PrincipalContext`, `PrincipalKind`, `Delegation`,
  `WorkloadIdentity`, effective-principal resolution.
- `decision.rs` — `Decision`, `BatchDecision`.
- `contract.rs` — `ResourceOwner` trait, `Purpose`, `Representation`,
  payload types, `SourceError`, `MAX_BATCH_SIZE`.
- `registry.rs` — `ResourceOwnerRegistry` (typed, no `Any` service locator).
- `authorizer.rs` — `SourceAuthorizer` Platform-Core facade, batch routing,
  Search/RAG `materialize` proof contract.
- `actions.rs` — canonical action capability names (`files.read`, ...).

## 2. ResourceRef

```rust
ResourceRef {
    application: ApplicationId,   // e.g. io.elembra.files
    resource_type: String,        // owner-owned stable type, e.g. "file"
    resource_id: String,          // opaque, length-bounded, treated as untrusted
    version: Option<String>,      // optional immutable selector, e.g. "sha256:<hex>"
}
```

- Canonical URI: `elembra://io.elembra.files/file/<id>?version=sha256%3A...`
- Forbidden identity fields (validated out): database table, SQL ID beyond the
  opaque id, deployment/runtime kind, service URL, storage key, presigned URL.
- Validation: namespace syntax for application/type; resource id non-empty,
  ≤ 512 chars, no whitespace/control; version `prefix:value`, ≤ 256 chars.
- Fail closed on: wrong scheme, missing/extra path segments, unknown query
  parameters, duplicates, fragments, userinfo, oversized/whitespace values.

## 3. PrincipalContext

```rust
PrincipalContext {
    principal_id, principal_kind (User|Service|Agent),
    tenant_id, workspace_id,
    group_ids: Vec<Uuid>, grants: Vec<ActionCapability>,   // informational
    authentication: Option<AuthenticationContext>,         // method/issuer/strength
    delegation: Option<Delegation>,
    workload_identity: Option<WorkloadIdentity>,           // transport caller
    correlation_id: Option<CorrelationId>,
}
```

### Where it is constructed

Today the per-request authority is `AuthenticatedUser { user_id, tenant_id }`
(`backend/server/src/handlers/extractors.rs`), with workspace == tenant (1:1).
The intended construction point for `PrincipalContext` is the **trusted
handler/service boundary**: a gated helper (not yet added — the first
consumer, Chat/Memory, adds it) converts an authenticated request into a
`User`-kind context, optionally carrying the request correlation id. Until
then, only the integration tests construct contexts directly, and no
production request path can build a `PrincipalContext` at all — the authorizer
is exercised exclusively through the test suites.

The tenant/workspace scope is always derived from the authenticated
Principal, never from client input. `group_ids`/`grants` in a context are
informational: owners **derive** group membership from authoritative DB state
(`PermissionResolverOps::get_user_group_ids`) and never trust a client-supplied
list as a grant.

### Delegation (Agents/Services)

`Delegation { issuer_principal_id, delegate_principal_id, actions,
workspace_id?, resource_scope?, expires_at?, grant_id? }`.

`PrincipalContext::effective_user_authority(action, resource)` enforces:

- user principals may not carry a delegation;
- Service/Agent require a delegation naming them, not expired, with the
  action in the delegated set, workspace in scope, and the resource in the
  optional scope (matched on application/type/id);
- issuer != delegate (no self-delegation/impersonation);
- delegated actions bounded (≤ 64).

The acting principal stays the Agent/Service; the owner evaluates the
**issuer's current authority** (so revocations apply immediately) bounded by
the delegation. Service identity alone never bypasses Principal
authorization. Grant issuance/verification storage is deferred to the Agents
Application; the trusted in-process boundary supplies the delegation.

## 4. Source authorization contract and the Files owner adapter

`ResourceOwner` (owned by the Application that owns the resource):

```text
authorize(ctx, action, ref)              -> Decision (allow|deny|not_found|invalid)
authorize_batch(ctx, action, refs[])     -> Vec<BatchDecision>   (bounded, per-ref association)
resolve(ctx, ref, purpose)               -> ResolvedResource     (authorized safe metadata)
fetch(ctx, ref, representation)          -> FetchedResource      (authorized content)
fetch_delivery_url(ctx, ref, purpose, ttl) -> short-lived URL    (only after authorization)
```

The Files adapter (`backend/server/src/authz/files_owner.rs`,
`FilesResourceOwner` for `io.elembra.files`) **delegates** to the existing,
now-tested Files semantics — no ACL/share rules are duplicated:

- decisions go through `PermissionResolver::check_file_permission` /
  `check_folder_permission` (owner implicit Admin, direct user shares, group
  shares, folder-ancestry inheritance);
- existence/tenant scoping through tenant-scoped
  `PermissionResolverRepository::find_file_by_id/find_folder_by_id`
  (deleted/cross-tenant resources → not-found, no existence leak);
- action→level mapping preserves Files behavior: `files.read`→View,
  `files.write`→Edit, `files.delete`→Admin (incl. shared-Admin subtree-delete
  gate), `files.share`→Admin. Note: the legacy operation-level gates remain
  the final authority — folder/group/recipient share management requires
  Admin (which the mapping matches), while the legacy public-link
  `create_share` endpoint additionally requires file ownership (owner-only);
  the contract decision is a pre-check and never bypasses those operation
  gates.
- content via `ObjectStore::get` and short-lived presigned URLs generated
  **only after** authorization (TTL clamped to ≤ 900 s);
- version selectors `sha256:<content_hash>` resolve through
  `MetadataStore::list_file_versions`; unknown versions fail closed
  (`VersionUnavailable` / `available: false`).

The ApplicationRegistry from #210 supplies the Application identity;
`authz::build_source_authorizer` seeds the typed `ResourceOwnerRegistry` with
the Files adapter. Core never queries Files private tables: it routes through
the `ResourceOwner` contract.

## 5. Batch authorization

- Bounded at `MAX_BATCH_SIZE = 64`; oversized batches are rejected outright
  (`BatchTooLarge`).
- Every result is explicitly associated with its `ResourceRef`; input order is
  preserved; one denied/missing/invalid ref never authorizes another; a
  malformed ref yields `Invalid` without affecting siblings.
- Mixed-owner batches are split by application and routed once per owner.
- Cross-tenant refs fail closed (tenant-scoped lookups resolve to not-found).
- Owners re-resolve the effective principal per ref (delegation bounds).

## 6. Search/RAG proof contract

`SourceAuthorizer::materialize(ctx, action, candidates)` implements the
mandatory reauthorization-before-materialization sequence:

1. batch-reauthorize candidate refs with their owners;
2. drop denied/not-found/invalid/stale candidates;
3. fetch only authorized source content (each fetch re-authorizes);
4. `cached_text` (a stale/malicious index hint) **never** enters the output.

This is the contract #119 (Memory/RAG) will consume; Memory itself is not
implemented here. Post-generation filtering remains prohibited.

## 7. Chat attachment compatibility

A Chat event will carry `ResourceRef + safe display metadata` and no permanent
bearer/presigned URL. `resolve`/`fetch`/`fetch_delivery_url` reauthorize the
viewer at read time, so Files revocation remains effective against immutable
historical events. Buzz/Chat integration is deferred to its own issues.

## 8. Existing duplicated authorization identified (deferred)

- `ShareService::check_resource_permission` (`share_service.rs`) is a weaker,
  non-ancestry share-management check that partially duplicates resolver
  logic. It is intentionally left untouched: share-management behavior is
  security-sensitive and out of #211 scope. The new contract uses the
  canonical resolver path only.
- `MetadataStore::is_user_in_group` (no tenant scoping) vs the tenant-scoped
  `get_user_group_ids` used by the resolver. Left untouched; the adapter uses
  the resolver path.

No duplication was removed in #211 because both sites are security-sensitive
and behavior-changing; they are flagged for the permission-resolver redesign.

## 9. Deferred authorization items discovered

- **Delegation authenticity (hard gate on the first consumer).** The contract
  enforces delegation *bounds* (action set, workspace, resource scope,
  expiry, no self-delegation) and re-evaluates the issuer's current authority
  at the source, but the delegation's *issuance* (that the issuer actually
  granted it) is not yet verified against a grant store — grant
  issuance/verification storage is deferred to the Agents Application. The
  first consumer that wires `source_authorizer` into a request path MUST
  construct `PrincipalContext` only at a trusted boundary fed by an
  authoritative grant store; `Delegation` and `PrincipalContext` carry this
  contract in their doc comments, and a regression test locks that a forged
  delegation to a powerless issuer grants nothing.
- **Application-level grants/enablement gating** on the authorizer path:
  ADR-0032 assigns "whether an Application is enabled / Principal has an
  Application-level grant" to Core. The current model has tenant/workspace
  enablement only (no per-Principal application grants) and the in-memory
  `ApplicationRegistry` enablement is seeded from DB. #211 enforces
  delegation validity and owner authorization; a caller-side enablement gate
  can be layered on `ApplicationRegistry::is_enabled` without contract
  changes. Not blocking for the Files-first implementation.
- **Streaming fetch**: v1alpha1 `fetch` returns in-memory `Bytes`
  (`ObjectStore::get`). Handlers already stream via `get_stream`; a streaming
  transport can be added to the contract when a consumer needs it.
- **Preview/thumbnail representations**: Files adapter returns
  `UnsupportedRepresentation` for `Preview`/`Thumbnail` in v1alpha1.
- **403/404 coalescing**: the typed contract keeps `Deny`/`NotFound` distinct
  (matching current Files 403-vs-404 semantics). Any future external HTTP
  adapter must coalesce per policy to avoid existence leakage, and must map
  `SourceError::Internal` to a generic error without leaking DB/object-store
  detail to untrusted clients.
- **Sensitive-access audit**: the adapter emits `tracing::warn`/`error` on
  denials and failures only; success-path audit emission for sensitive source
  access (spec "Purpose and audit") is deferred until a consumer defines the
  audit sink.
- **Deep delegation chains**: one active delegation per context (the ultimate
  issuer) is modeled; multi-hop chains are deferred to the Agents Application.
- **RLS activation, permission-resolver redesign, dual-ShareService
  consolidation**: deliberately deferred (issue non-goals), unchanged.

## 10. Files behavior preserved

Regression coverage ensures the new contract cannot bypass the semantics
established by #218/#221/#222:

- shared recipients operate within permitted folders (Edit recipient can
  write/delete inside the shared tree; cannot delete the root without Admin);
- destination Edit checks are untouched (they live in File/Folder services);
- shared Admin subtree deletion gate (`files.delete` on a folder root requires
  Admin, exactly as `delete_folder`);
- revoked shares/groups take immediate effect (decisions re-evaluated per
  call — no cached ACL);
- cross-tenant denial;
- public shares only authorize public-share sessions, never arbitrary
  Principals (public/group share distinction preserved).
