# ADR Index

Architecture Decision Records live in this directory as `NNNN-title.md` and use
sequential numbers. New ADRs must pick the next unused number. Statuses used:
`Accepted` / `Proposed` / `Superseded`.

> **Reference rule:** when a number is duplicated (see below), always reference
> an ADR by its full file name (`0036-elembra-chat-zero-config-bootstrap.md`),
> never by number alone.

## Index

| ADR | Title | Status |
| --- | --- | --- |
| `0037-calendar-application-and-external-sync.md` | Calendar Application and External Calendar Sync | Proposed |
| `0038-institutional-organization-and-workspace-access.md` | Institutional Organization and Workspace Access | Proposed |
| `0039-orphan-blob-discovery-and-reconciliation.md` | Orphan Blob Discovery and Reconciliation | Accepted (3.4A); 3.4B Implemented (disabled by default, not activated) |
| `0040-organization-tenant-and-workspace-identity.md` | Organization, Tenant and Workspace Identity | Accepted (2026-10-15) |
| `0041-groups-and-authorization.md` | Groups and Organization Authorization | Accepted (2026-10-15) |
| `0042-postgresql-tenant-isolation-rls-staging.md` | PostgreSQL Tenant Isolation — Least Privilege First, Gated RLS | Accepted (2026-10-15) |
| `0043-legacy-nil-tenant-compatibility.md` | Legacy Nil-Tenant Compatibility and Non-Destructive Migration | Accepted (2026-10-15) |

The 2026-10-15 revisions of ADR-0040–0043 are Accepted following explicit user
approval. Acceptance covers architecture and associated milestone/contract amendments,
not deployment, RLS/GC activation or legacy mapping. Historical audits retain their original baselines. See the
[architecture review](../audits/2026-10-15-pr360-architecture-final-review.md)
for decision ownership, verification boundaries and remaining human gates.

## Known duplicate numbers (documented cleanup — renumbering deferred)

Renumbering an ADR would break existing cross-references and historical
records, so collisions are documented here instead of renumbered. A broad
renumbering is a separate cleanup task and must be reviewed as a batch.

| Number | Files sharing the number |
| --- | --- |
| 0019 | `0019-notes-as-okf-documents.md`, `0019-shared-rich-markdown-editor.md` |
| 0020 | `0020-canonical-markdown-with-editor-cache.md`, `0020-okf-notes-reconciliation-and-rag-safety.md` |
| 0030 | `0030-elembra-application-model.md`, `0030-openapi-auto-generation.md` |
| 0031 | `0031-durable-integration-events.md`, `0031-tenant-isolation-share-links-and-rls.md` |
| 0032 | `0032-resource-refs-and-authorization.md`, `0032-safe-content-addressed-blob-garbage-collection.md` |
| 0036 | `0036-elembra-chat-zero-config-bootstrap.md`, `0036-unified-search-permission-model.md` |

(Confirmed 2026-08-16 during the Elembra Alpha Product Completion milestone;
the 0036 collision was introduced when `0036-elembra-chat-zero-config-bootstrap.md`
landed alongside the pre-existing `0036-unified-search-permission-model.md`.)
