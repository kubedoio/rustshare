Of course — here is the ADR in full so you can copy it directly:

````md
# ADR: Storage-Native Metadata Architecture for Shared File-Based Applications

- **Status:** Proposed
- **Date:** 2026-03-27
- **Decision Makers:** RustShare / RustChat architecture owners
- **Scope:** Shared storage contract for RustShare, RustChat, and future tools using RustFS
- **Related ADRs:** RustShare repository/storage refactor, RustShare Postgres removal migration plan

---

## Context

RustShare currently uses:

- PostgreSQL as the canonical metadata store
- RustFS (S3-compatible object storage) for file content
- Axum as the backend framework
- WebSocket-based real-time updates
- repository abstractions around file, folder, share, and user metadata

This design works for a single application, but it introduces an architectural split:

- **file bytes** live in object storage
- **application truth** lives in PostgreSQL

That split becomes limiting when we want to support a broader system architecture in which multiple tools can use the same file substrate.

### Strategic direction

The long-term direction is to support multiple applications, including RustShare and RustChat, on top of the same RustFS deployment.

Examples:

- RustShare uploads and manages user files
- RustChat references or attaches files already stored in RustFS
- future tools reuse the same blob storage without duplicating content
- multiple tools share file references, permissions, and metadata contracts where appropriate

To support this, the storage model must become:

- portable across applications
- independent from a single relational schema
- explicit about file identity, blob identity, and metadata identity
- safe under concurrent access
- rebuildable and auditable

A direct “replace Postgres rows with JSON files” approach is not sufficient.  
RustFS is an S3-compatible object store, not a relational database. It does not provide relational joins, query planning, or multi-row transactions. Therefore the replacement architecture must be **object-store-native**, not a filesystem imitation of SQL tables.

---

## Decision

We will adopt a **storage-native metadata architecture** in which RustFS becomes the durable storage substrate for both file content and canonical metadata objects.

### Core decision

The system will treat RustFS-backed objects as the durable source of truth for:

- file metadata
- folder metadata
- file version metadata
- share metadata
- tombstones
- durable projections / indexes
- append-only domain events

PostgreSQL will no longer be the canonical metadata authority once migration is complete.

### High-level model

The architecture will distinguish between four layers:

1. **Immutable blob storage**
   - stores file content only
   - content is immutable
   - blob keys are content-addressed where practical

2. **Canonical metadata documents**
   - one logical document per resource
   - stable IDs are canonical
   - path is a derived property, not identity

3. **Durable projections / indexes**
   - optimized for read flows
   - derived from canonical metadata
   - rebuildable at any time

4. **Append-only domain events**
   - emitted on mutation
   - used for audit, sync, reconciliation, and rebuild support

This architecture is intended to be reusable by RustShare, RustChat, and future applications.

---

## Why this decision was made

### 1. Shared storage across applications

We want multiple tools to be able to use the same physical blob store without each tool owning an isolated relational database schema as the sole source of truth.

This requires:

- shared blob references
- app-scoped metadata namespaces
- stable cross-tool resource identifiers
- a storage contract that is not bound to one application’s database

### 2. Separation of blob identity from metadata identity

A file content blob and a file metadata record are different things.

This architecture keeps them separate:

- blob identity = content object reference
- file identity = logical application resource
- folder identity = logical container
- share identity = permission/grant object

This makes rename, move, versioning, and cross-application reference much cleaner.

### 3. Object storage is already the operational substrate

RustFS is already part of the system. Using it as the metadata substrate reduces the architectural split between “where bytes live” and “where truth lives.”

### 4. Better portability for standalone deployments

A storage-native design makes it easier to deploy applications in environments where operators want a file/object-backed system without requiring relational infrastructure for the core file model.

### 5. Future integration with RustChat and other tools

RustChat should later be able to reference the same blobs and possibly the same shared metadata patterns without depending on RustShare’s PostgreSQL schema or internal ORM assumptions.

---

## Architecture principles

### Principle 1: Stable IDs are canonical

All durable resources must have stable IDs.

Examples:

- `file_id`
- `folder_id`
- `share_id`
- `namespace_id`
- `principal_id`
- `version_id`

Paths and names are mutable attributes, not identity.

**Implication:** rename and move operations do not create new identities.

---

### Principle 2: Blob content is immutable

File content objects must be treated as immutable.

A new version of a file creates:

- a new blob reference, or
- a new version document pointing to a different blob

It must not overwrite canonical content in place.

**Implication:** versioning, audit, rollback, and cross-tool reuse become predictable.

---

### Principle 3: Metadata documents are canonical truth

Each logical entity has a canonical metadata document stored in RustFS.

Examples:

- folder document
- file head document
- file version document
- share document
- tombstone document

These documents are the authoritative source of truth.

**Implication:** read models and caches can be rebuilt from canonical data.

---

### Principle 4: Read indexes are derived, durable, and rebuildable

Listing a folder or resolving “shared with me” must not require bucket-wide scanning.

Therefore the system must maintain durable projections such as:

- folder children indexes
- per-user roots
- per-user recent items
- per-user shared-with-me views
- share lookup indexes if needed

These are not the source of truth. They are read models.

**Implication:** normal reads are efficient without pretending object storage is a query engine.

---

### Principle 5: Runtime caches are acceleration only

In-memory indexes or caches may be used for hot paths, but they are never canonical truth.

They must be:

- optional for correctness
- rebuildable after restart
- updated or invalidated explicitly after writes

**Implication:** system correctness does not depend on process-local memory.

---

### Principle 6: Concurrency must be explicit

Object storage does not provide SQL-style multi-row transactions.

Therefore multi-object mutations must use an explicit concurrency strategy:

- optimistic concurrency using version/etag semantics where supported
- fallback short lease / lock objects where required
- deterministic conflict detection
- repair and reconciliation where atomicity is not possible

**Implication:** no silent lost updates.

---

### Principle 7: Normal operation must not rely on full scans

Full namespace scans are allowed only for:

- migration import
- repair/reconciliation
- index rebuild
- local development bootstrap, if a local-fs backend exists

Normal user-facing operations must rely on direct metadata reads and durable indexes.

---

## Shared storage model

The storage model is intentionally divided into shared and app-scoped areas.

### Shared namespace

Used for reusable content objects.

Example:

```text
shared/
  blobs/
    sha256/
      ab/
        cd/
          <content-hash>
````

This namespace is intended to be reusable across applications.

### Application-scoped namespace

Each application keeps its own metadata under an application namespace.

Example:

```text
apps/
  rustshare/
    meta/
      ...
  rustchat/
    meta/
      ...
```

This allows:

* shared blobs
* isolated metadata ownership
* controlled future interoperability

---

## Canonical object layout

The exact key names may evolve, but the model should remain close to this:

```text
shared/
  blobs/
    sha256/
      ab/
        cd/
          <content-hash>

apps/
  rustshare/
    meta/
      namespaces/
        <namespace-id>.json
      folders/
        <folder-id>.json
      files/
        <file-id>.json
      file_versions/
        <file-id>/
          <version-id>.json
      shares/
        <share-id>.json
      users/
        <user-id>.json
      tombstones/
        files/<file-id>.json
        folders/<folder-id>.json
      events/
        YYYY/MM/DD/<event-id>.json
      indexes/
        folders/<folder-id>/children.json
        users/<user-id>/roots.json
        users/<user-id>/recent.json
        users/<user-id>/shared_with_me.json
```

---

## Canonical document types

### Folder document

Represents the current logical state of a folder.

Recommended fields:

* `schema_version`
* `id`
* `namespace_id`
* `parent_id`
* `name`
* `owner_id`
* `acl_ref`
* `created_at`
* `updated_at`
* `version`
* `deleted`

---

### File head document

Represents the current visible state of a file.

Recommended fields:

* `schema_version`
* `id`
* `namespace_id`
* `parent_id`
* `name`
* `owner_id`
* `current_version_id`
* `size`
* `mime`
* `content_ref`
* `checksum`
* `acl_ref`
* `created_at`
* `updated_at`
* `version`
* `deleted`

---

### File version document

Represents a specific immutable version of a file.

Recommended fields:

* `schema_version`
* `id`
* `file_id`
* `content_ref`
* `size`
* `checksum`
* `created_by`
* `created_at`

---

### Share document

Represents a grant of access to a resource.

Recommended fields:

* `schema_version`
* `id`
* `resource_type`
* `resource_id`
* `scope`
* `permissions`
* `token_hash` or equivalent secure lookup reference
* `expires_at`
* `created_by`
* `created_at`
* `revoked_at`
* `version`

---

### Tombstone document

Represents a deleted resource with enough information to support reconciliation or restore.

Recommended fields:

* `schema_version`
* `id`
* `resource_type`
* `resource_id`
* `deleted_at`
* `deleted_by`
* `previous_parent_id`
* `restore_metadata`

---

### Event document

Represents an append-only domain event.

Recommended fields:

* `schema_version`
* `id`
* `event_type`
* `actor_id`
* `resource_type`
* `resource_id`
* `occurred_at`
* `correlation_id`
* `payload`

---

### Folder children projection

Represents a durable read model for folder listing.

Recommended fields:

* `schema_version`
* `folder_id`
* `version`
* `children[]`

Each child entry should include lightweight list-view information such as:

* `id`
* `kind`
* `name`
* `deleted` or visibility marker
* relevant lightweight display metadata

---

## Recommended storage abstractions

The service layer should not speak directly in RustFS/S3 terms.
It should depend on explicit storage roles.

### BlobStore

Responsibilities:

* put immutable blob
* get blob
* verify checksum
* optionally deduplicate by content reference

### MetadataStore

Responsibilities:

* read/write single canonical documents
* typed serialization
* version or etag-aware conditional updates
* schema version management

### IndexStore

Responsibilities:

* read/write projections
* rebuild projections
* compare-and-set or safe overwrite where appropriate

### EventStore

Responsibilities:

* append event
* range/prefix read for rebuild/debug
* immutable event storage

### MetadataCoordinationStore

Responsibilities:

* concurrency control abstraction
* optimistic concurrency and/or lease objects
* conflict detection and retry behavior

### RuntimeIndex

Responsibilities:

* in-memory acceleration
* explicit update/invalidation after durable write success
* rebuild from durable state after restart

---

## Consistency model

The system must make its consistency model explicit.

### Canonical truth

Canonical metadata documents are the source of truth.

### Derived state

Durable projections and runtime cache may be stale temporarily, but they must converge and remain repairable.

### Write safety

Multi-object writes must not silently lose updates.

### Repairability

If a multi-step mutation partially succeeds, the system must leave enough evidence for deterministic repair.

### Practical recommendation

A hybrid model is recommended:

* synchronous canonical metadata update
* synchronous durable event append
* synchronous or bounded-latency projection update depending on operation criticality
* runtime cache update/invalidation after durable steps complete

This gives good operational clarity without pretending object storage is transactional in the SQL sense.

---

## Mutation requirements

Every mutation must define:

* canonical documents touched
* projections touched
* events emitted
* runtime cache updates or invalidations
* concurrency strategy
* retry behavior
* repair strategy if an intermediate step fails

This is especially important for:

* file upload
* file rename
* file move
* file delete / restore
* folder create / rename / move / delete / restore
* share create / revoke
* file version creation

---

## Integration contract for RustChat and future tools

This ADR is intentionally broader than RustShare.

Future applications should follow these integration rules.

### 1. Shared blobs, scoped metadata

Applications may read and reference blobs from `shared/blobs/...`, but must keep application-owned metadata in their own namespace unless a truly shared metadata contract is explicitly introduced.

### 2. No physical path references as durable identity

Applications must not use storage paths as the durable identity of logical resources.

They must use stable IDs and blob references.

### 3. Cross-tool file reuse should happen via blob references

If RustChat wants to attach a file already managed by RustShare, it should reuse a blob/content reference rather than duplicate the bytes.

### 4. Cross-tool permissions must be explicit

ACLs and sharing semantics must not be assumed to be globally identical across applications.
A shared principal model may exist, but access rules should remain explicit and application-aware unless a common authorization layer is adopted.

### 5. Read indexes are app-owned unless explicitly shared

RustShare’s folder listings and RustChat’s attachment indexes are separate concerns even if they point to the same blob layer.

---

## Consequences

### Positive consequences

* enables shared blob usage across multiple tools
* removes dependency on PostgreSQL as canonical metadata truth
* supports stable identity independent of path
* makes rebuild and reconciliation possible
* aligns storage of bytes and metadata on the same durable substrate
* creates a reusable storage contract for future products

### Negative consequences

* object storage is not a relational query engine
* projections/indexes must be designed deliberately
* multi-object mutations require explicit concurrency handling
* rebuild and drift tooling become mandatory
* migration complexity is higher than a simple storage backend swap

### Neutral but important consequences

* an in-memory index may still exist, but only as a cache
* a local filesystem backend may exist for development, but it must preserve the same domain invariants
* events are now architectural, not incidental

---

## Alternatives considered

### Alternative 1: Keep PostgreSQL as canonical metadata forever

Rejected as the long-term target because it keeps metadata tightly coupled to one application schema and makes multi-app shared storage less clean.

### Alternative 2: Replace Postgres with sidecar JSON files next to blobs

Rejected as the canonical architecture because it is too path-centric, too filesystem-specific, and not a clean fit for S3-compatible object storage semantics.

### Alternative 3: Full event sourcing

Rejected for now because it adds significant complexity beyond the current need.
Append-only domain events are required, but full event sourcing is not.

---

## Migration guidance

This ADR describes the target architecture, not the complete migration procedure.
However, migration should follow these rules:

1. do not remove PostgreSQL in a big-bang change
2. introduce object-backed metadata first
3. dual-write during transition
4. verify parity
5. switch reads gradually
6. remove Postgres from the canonical path only after rebuild/repair tooling exists

---

## Operational requirements

The architecture requires operational tooling for:

* index rebuild
* consistency verification
* drift detection
* event inspection
* migration verification
* repair / reconciliation workflows

Without these tools, the design is incomplete.

---

## Decision summary

We will move from a **database-centered metadata architecture** to a **storage-native metadata architecture** in which:

* RustFS stores immutable blobs
* RustFS stores canonical metadata documents
* durable projections support efficient reads
* append-only events support audit and repair
* runtime caches remain non-canonical
* stable IDs replace path-based identity
* the storage contract is reusable by RustShare, RustChat, and future tools

This architecture is the foundation for shared file infrastructure across multiple applications.

---

## Status after adoption

If adopted, future integration and implementation ADRs should align with this document and avoid reintroducing:

* path-based durable identity
* scan-based normal reads
* relational-only assumptions in the service layer
* app-specific assumptions in shared blob storage

