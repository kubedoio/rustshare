# ADR-0032: Resource References and Cross-Application Authorization

Status: Proposed  
Date: 2026-08-07

## Context

Elembra Applications need to refer to each other's resources without sharing database schemas or duplicating authoritative data.

Examples:

- a Buzz chat event references an Elembra File;
- a Memory result cites a Mail message;
- an Agent action targets a Note;
- a Meeting record links a Decision;
- a Connector publishes provenance pointing to an external source.

Using raw foreign table IDs creates hidden coupling. Copying resource content into every consumer creates multiple sources of truth. Treating search/index visibility as authorization creates a security boundary failure.

## Decision

Elembra introduces a canonical opaque **ResourceRef** and a strict rule: **the owning Application is the final authority for resource access**.

## ResourceRef

Logical shape:

```json
{
  "application": "io.elembra.files",
  "resource_type": "file",
  "resource_id": "01K...",
  "version": "sha256:..."
}
```

`version` is optional for references to the current mutable resource and recommended when provenance requires an immutable source version.

A URI representation may be used for transport/display:

```text
elembra://io.elembra.files/file/01K...?version=sha256%3A...
```

ResourceRefs are opaque to consumers. Consumers must not infer database table names, storage keys or authorization from them.

## Ownership resolution

The Platform Core/Application registry can resolve:

```text
application id -> Application contract endpoint/adapter
```

It does not resolve the resource by querying the owner's database itself.

## PrincipalContext

Cross-Application calls carry a canonical authority context:

```text
PrincipalContext
- principal_id
- tenant_id
- workspace_id
- group_ids
- global/application grants
- authentication context/strength where relevant
- delegation chain
- workload/service identity
- correlation_id
```

The service workload authenticates the transport. The PrincipalContext identifies whose authority the request is exercising. Service authentication must never silently elevate the end-user Principal.

## Authorization split

### Platform Core

Core decides platform-level facts such as:

- whether Principal belongs to tenant/workspace;
- whether an Application is enabled;
- whether Principal has an Application-level grant;
- delegation validity for an Agent/service action.

### Owning Application

The owner decides resource-level actions:

```text
(files.read, FileRef)
(files.share, FileRef)
(mail.read, MessageRef)
(mail.send, MailAccountRef)
(chat.read, ChannelRef)
(chat.post, ChannelRef)
(notes.write, NoteRef)
```

No consumer may replace the owner's decision with its own cached ACL interpretation for final access.

## Resolve/fetch contract

An owner should expose a bounded contract equivalent to:

```text
resolve(ref, principal, purpose) -> metadata or not_found/forbidden
fetch(ref, principal, representation) -> authorized content/stream
```

Security-sensitive APIs should generally avoid revealing whether a cross-tenant resource exists; fail closed using the same non-disclosure conventions as current tenant-scoped share lookups.

## Batch authorization

Memory/search requires efficient candidate filtering. Owners must support a batch contract:

```text
authorize_batch(
  principal_context,
  action,
  resource_refs[]
) -> decisions[]
```

Properties:

- bounded batch size;
- tenant/workspace validation;
- stable response ordering or explicit ref association;
- fail closed on malformed/unknown refs;
- per-item allow/deny/not-found result;
- audit policy appropriate to action.

## Search and RAG rule

Memory/Search may store a coarse ACL projection to reduce candidate volume, but this is only a performance filter.

Before source content is materialized into an LLM context:

1. candidate ResourceRefs are batch-reauthorized against the owning Applications;
2. denied/stale/deleted refs are removed;
3. authorized content is fetched from the owner or an owner-authorized immutable representation;
4. the source access is auditable.

Post-generation filtering is prohibited.

## Chat attachment rule

A Buzz chat event may contain:

- immutable/versioned ResourceRef;
- safe display name/type/size where policy permits;
- no permanent access token;
- no sensitive preview copied into the signed event unless deliberately classified safe.

Opening the attachment calls Elembra Files and reauthorizes the viewer at read time. Revocation in Files therefore takes effect even though the historical Buzz event remains signed and immutable.

## Delegation and Agents

Agents use their own PrincipalId plus an explicit delegation chain/grant. They do not impersonate the initiating user.

A delegation includes at minimum:

- issuer/initiator;
- agent principal;
- allowed action capabilities;
- tenant/workspace scope;
- optional resource/scope restrictions;
- expiry;
- approval requirements where applicable.

The source Application evaluates the agent's delegated authority like any other Principal.

## External ResourceRef

Connectors may need references to external source objects. These are owned by the Connector/Application responsible for resolving them, not directly by arbitrary consumers.

Example:

```json
{
  "application": "io.elembra.connector.gdrive",
  "resource_type": "drive_file",
  "resource_id": "external-id:1AbC...",
  "version": "drive-revision:..."
}
```

The Connector preserves provider provenance and applies its declared access/mirroring semantics.

## Consequences

### Positive

- No cross-Application foreign-table coupling.
- Revocation remains authoritative at the source.
- Chat history can reference Files without duplicating access control.
- Memory can cite heterogeneous sources safely.
- Runtime extraction does not change cross-Application references.

### Negative

- Source reauthorization adds network/adapter calls.
- Owners need batch APIs for efficient search/RAG.
- Consumers must handle deleted/revoked references gracefully.

## Rejected alternatives

### Shared global resource table containing every domain object

Rejected. A global catalog may store reference/provenance metadata, but it cannot become the authoritative store for all Application content/permissions.

### Put signed/presigned URLs in durable cross-Application records

Rejected as the canonical relationship because URLs expire and may leak authority. Generate short-lived delivery URLs only after authorization.

### Trust indexed ACL forever

Rejected because permissions, group membership and source state change independently of the index.

## Acceptance criteria

- [ ] ResourceRef type and serialization are defined in a shared contract package.
- [ ] Runtime strategy is absent from ResourceRef identity.
- [ ] At least Files implements resolve/fetch/batch-authorize semantics.
- [ ] Cross-tenant/malformed refs fail closed.
- [ ] Search/RAG tests prove source reauthorization occurs before LLM context assembly.
- [ ] Chat attachment design uses ResourceRef without persistent access tokens.
- [ ] Agent calls carry their own Principal and delegation chain.
