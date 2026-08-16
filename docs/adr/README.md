# ADR Index

Architecture Decision Records live in this directory as `NNNN-title.md` and use
sequential numbers. New ADRs must pick the next unused number. Statuses used:
`Accepted` / `Proposed` / `Superseded`.

> **Reference rule:** when a number is duplicated (see below), always reference
> an ADR by its full file name (`0036-elembra-chat-zero-config-bootstrap.md`),
> never by number alone.

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
