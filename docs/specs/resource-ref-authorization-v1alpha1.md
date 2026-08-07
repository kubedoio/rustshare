# Specification: ResourceRef and Source Authorization v1alpha1

Status: Draft  
Date: 2026-08-07  
Related: ADR-0032

## Purpose

Define the portable resource identity and authority context used when one Elembra Application refers to, searches, fetches or acts on a resource owned by another Application.

The central rule is:

> A `ResourceRef` identifies a resource. It never grants access to that resource.

The owning Application remains the final resource-level authorization authority.

## ResourceRef

Canonical JSON shape:

```json
{
  "application": "io.elembra.files",
  "resourceType": "file",
  "resourceId": "01K2ABC...",
  "version": "sha256:0123..."
}
```

Fields:

- `application` — stable Application ID that owns/resolves the resource.
- `resourceType` — Application-owned stable resource type.
- `resourceId` — opaque resource identifier interpreted only by the owner.
- `version` — optional immutable/version selector for provenance or historical access.

### URI rendering

A canonical URI form may be used for logs, links and event subjects:

```text
elembra://io.elembra.files/file/01K2ABC...?version=sha256%3A0123...
```

The JSON structure remains the preferred typed API representation.

### Validation

- Application ID must be registered/known for internal Elembra refs.
- Resource type syntax is owner-namespaced/validated.
- Resource ID is length-bounded and treated as opaque untrusted input.
- Version is optional and owner-defined.
- Runtime endpoint, database table, storage key and hostname are forbidden as identity fields.
- Unknown/malformed refs fail closed.

## PrincipalContext

Canonical logical shape:

```json
{
  "principalId": "01KPRINCIPAL...",
  "tenantId": "01KTENANT...",
  "workspaceId": "01KWORKSPACE...",
  "groupIds": ["01KGROUP..."],
  "applicationGrants": ["files.read", "memory.query"],
  "authentication": {
    "method": "oidc",
    "issuer": "https://auth.example.invalid/realms/elembra",
    "strength": "mfa"
  },
  "delegation": [],
  "correlationId": "01KCORRELATION..."
}
```

This is a logical contract. Do not blindly trust client-supplied group/grant arrays. The receiving trusted boundary derives or verifies authority context from authenticated Elembra claims/service-to-service delegation.

### Workload identity vs Principal

A service/worker authenticates **who is calling the API transport**.

`PrincipalContext` identifies **whose business authority the operation exercises**.

These must not be conflated.

Example:

```text
Memory worker workload identity
  acts for Principal 01KUSER...
  to request files.read on File ResourceRef
```

The worker's trusted service credential does not imply `files.read` for every resource.

## Delegation

An Agent/service action may carry a bounded delegation chain.

Minimum logical fields:

```json
{
  "issuerPrincipalId": "01KUSER...",
  "delegatePrincipalId": "01KAGENT...",
  "actions": ["files.read", "chat.post"],
  "workspaceId": "01KWORKSPACE...",
  "resourceScope": null,
  "expiresAt": "2026-08-08T20:00:00Z",
  "grantId": "01KGRANT..."
}
```

Rules:

- Agent remains the acting Principal; it does not become the issuer.
- Delegated action set is an upper bound, not an automatic resource allow.
- Source Application still evaluates resource-level authorization.
- Expired/revoked delegation fails closed.
- Delegation/initiator is retained in audit/correlation context.

## Action names

Action capability names are stable dotted strings owned by Applications.

Examples:

```text
files.read
files.write
files.delete
files.share
notes.read
notes.write
mail.read
mail.send
chat.read
chat.post
memory.query
agents.run
```

Avoid broad wildcard grants in normal user/agent flows. Administrative actions, if needed, remain distinct from resource actions.

## Authorization API semantics

Transport may initially be an in-process Rust adapter or HTTP/JSON service. Semantics are identical.

### Single authorization

Logical request:

```json
{
  "principal": { "...": "PrincipalContext" },
  "action": "files.read",
  "resource": { "...": "ResourceRef" }
}
```

Logical response:

```json
{
  "decision": "allow",
  "resource": { "...": "ResourceRef" },
  "authorizationVersion": "acl:..."
}
```

Allowed decision values:

```text
allow
deny
not_found
invalid
```

Externally exposed security-sensitive endpoints may deliberately coalesce `deny` and `not_found` to avoid existence leakage. Internal typed APIs may retain the distinction only where policy permits it and callers do not expose it unsafely.

### Batch authorization

Logical request:

```json
{
  "principal": { "...": "PrincipalContext" },
  "action": "files.read",
  "resources": [
    { "application": "io.elembra.files", "resourceType": "file", "resourceId": "A" },
    { "application": "io.elembra.files", "resourceType": "file", "resourceId": "B" }
  ]
}
```

Logical response:

```json
{
  "decisions": [
    { "resource": { "resourceId": "A", "...": "..." }, "decision": "allow" },
    { "resource": { "resourceId": "B", "...": "..." }, "decision": "deny" }
  ]
}
```

Requirements:

- batch size is bounded by the owner;
- every decision is explicitly associated with a ResourceRef; do not depend solely on array ordering;
- tenant/workspace context applies to every entry;
- mixed-owner batches are split/routed by the caller/Application registry;
- one malformed ref must not accidentally make other refs allowed;
- partial decisions are supported;
- timeout/error defaults to deny/no materialization for security-sensitive consumers such as RAG.

## Resolve metadata

A source Application may expose authorized metadata resolution.

Logical operation:

```text
resolve(principal, resourceRef, purpose)
```

Possible authorized output:

```json
{
  "resource": { "...": "ResourceRef" },
  "displayName": "architecture.md",
  "mediaType": "text/markdown",
  "size": 12420,
  "updatedAt": "2026-08-07T18:00:00Z",
  "available": true
}
```

Metadata itself can be sensitive. Resolution therefore requires authorization appropriate to the `purpose`/action and must not become a way to enumerate cross-tenant resources.

## Fetch/materialize

A source Application may expose:

```text
fetch(principal, resourceRef, representation)
```

Representations might include:

```text
raw
text
preview
thumbnail
metadata
```

Rules:

- owner authorizes before returning bytes/text;
- large/binary content should stream where appropriate;
- temporary delivery URLs are generated only after authorization and have short TTL/scope;
- permanent bearer/presigned URLs must not be persisted as the canonical cross-Application relationship;
- requested historical `version` is honored only if owner policy allows it;
- deleted/revoked resources fail closed according to owner semantics.

## Purpose and audit

Security-sensitive source access can include a purpose field such as:

```text
user_open
search_preview
memory_index
rag_context
agent_tool
chat_unfurl
export
```

Purpose does not grant authority. It allows policy/audit/representation decisions.

Minimum audit context where policy requires:

- Principal/Agent;
- service/workload caller;
- action;
- ResourceRef;
- allow/deny category;
- purpose;
- correlation/delegation IDs;
- timestamp.

Do not log resource content by default.

## Search/RAG contract

Memory/Search follows this mandatory sequence:

```text
1. Retrieve candidate ResourceRefs using coarse tenant/workspace/index ACL filtering.
2. Group candidates by owning Application.
3. Batch authorize current Principal for the required read action.
4. Remove denied/not-found/error/stale candidates.
5. Fetch/materialize only allowed source representations.
6. Assemble LLM context.
7. Preserve ResourceRef + location/version in citations.
```

A stale/malicious index that marks a forbidden source as visible must not cause content leakage because source authorization happens before materialization.

If the source owner is unavailable, the safe default is to omit that content rather than use stale cached content unless a separately defined owner-authorized immutable archival policy permits it.

## Chat attachment contract

A signed Buzz/Chat event may durably contain:

```json
{
  "resource": {
    "application": "io.elembra.files",
    "resourceType": "file",
    "resourceId": "01KFILE...",
    "version": "sha256:..."
  },
  "display": {
    "name": "design.pdf",
    "mediaType": "application/pdf",
    "size": 482103
  }
}
```

It must not durably contain a long-lived Files bearer token/presigned URL.

When a viewer opens or unfurls it:

```text
Chat viewer Principal
  -> Files authorize/read
  -> optional safe metadata/preview
  -> short-lived delivery or stream
```

Therefore Files revocation can deny future access while the historical signed Chat event remains intact.

## Error/failure semantics

Consumers must distinguish:

- **authorization denial/not found** — do not retry as infrastructure failure;
- **owner unavailable/timeout** — retry where appropriate, but do not materialize stale unauthorized content;
- **invalid ref/schema** — fail closed and record diagnostic without leaking sensitive data;
- **version unavailable** — surface as historical source unavailable, not silently substitute current content unless explicitly requested.

## Compatibility policy

This is `v1alpha1`. There is no obligation to preserve obsolete pre-release shapes.

Until declared stable:

- coordinated first-party breaking changes are allowed;
- do not add legacy aliases/version negotiation by default;
- migrate all first-party callers together;
- preserve Resource identity/user data where meaningful, not accidental wire shapes.

## Non-goals

- global authorization database containing every Application ACL;
- permanent global materialized-resource cache as an authorization source;
- cross-Application SQL foreign keys as public contracts;
- service credentials that bypass Principals;
- hidden Agent impersonation;
- authorization based on vector-index visibility;
- embedding access tokens in ResourceRefs.

## Contract tests

- ResourceRef JSON/URI round trip.
- Runtime strategy/endpoint change does not change ResourceRef identity.
- Unknown/malformed Application/type/ref fails closed.
- Cross-tenant ref cannot resolve/fetch.
- Workload service identity alone cannot authorize a resource.
- Revoked group/share membership changes current decision without requiring index refresh.
- Batch response associates decisions with correct refs under partial denial.
- Batch timeout/error causes secure omission in RAG path.
- Stale index allow cannot put denied source content into LLM context.
- Chat attachment can remain in history while later Files fetch is denied.
- Agent delegation expiry/revocation is enforced and actor remains the Agent Principal.
