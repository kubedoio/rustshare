<!--
This file follows the Keep a Changelog format.
See https://keepachangelog.com/en/1.1.0/ for details.
All notable changes to this project will be documented in this file.
-->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.3.0] - 2026-06-04

### Added

- Added backend activity feed support with cursor pagination and frontend activity wiring.
- Added readiness checks for database, object storage, events, and auth dependencies, with AI treated as optional.
- Added module event types and WebSocket invalidation coverage for module edits.
- Added shared UI states for loading, empty, error, offline, disabled, and unauthorized module workflows.
- Added canonical module/template contracts and expanded cross-tenant, share-link, attachment, search, AI permission, and module permission tests.
- Added undo and redo controls to the markdown and note editor toolbars.

### Changed

- Dashboard module rendering is now backend-driven through enabled module metadata instead of a static frontend registry.
- Module defaults now normalize toward canonical `/Workspace/<Module>` paths while preserving read compatibility for legacy roots.
- Documented permission-visible dashboard summary semantics, including directly shared Kanban boards whose module root is not shared.
- Documented attachment folder side-effect rules: upload may create the `attachments/` folder, while read/list/delete paths must not.

### Fixed

- Fixed tenant propagation across services, handlers, WebSocket auth, and collaboration paths.
- Fixed public share access checks so revoked and expired shares are revalidated per request.
- Fixed activity event filtering for stored JSON event types.
- Fixed search and AI permission filtering to avoid exposing unauthorized content.
- Fixed attachment path traversal, hidden-file, metadata filtering, and portability edge cases.
- Fixed folder drag-and-drop uploads in the web UI so dropped folders preserve their hierarchy.
- Fixed PDF previews in the web UI by allowing same-origin framing for file preview responses.
- Kanban attachment deletion now rejects non-attachment files without deleting them or creating a missing `attachments/` folder.
- Kanban dashboard summaries now include directly shared board folders even when `/Workspace/Kanban` itself is not shared.

### Security

- Hardened tenant isolation and authorization contracts across module, share, search, AI, public upload, and attachment flows.

## [0.3.0-rc.1] - 2026-05-31

### Added

- Added backend activity feed support with cursor pagination and frontend activity wiring.
- Added readiness checks for database, object storage, events, and auth dependencies, with AI treated as optional.
- Added module event types and WebSocket invalidation coverage for module edits.
- Added shared UI states for loading, empty, error, offline, disabled, and unauthorized module workflows.
- Added canonical module/template contracts and expanded cross-tenant, share-link, attachment, search, AI permission, and module permission tests.

### Changed

- Dashboard module rendering is now backend-driven through enabled module metadata instead of a static frontend registry.
- Module defaults now normalize toward canonical `/Workspace/<Module>` paths while preserving read compatibility for legacy roots.
- Documented permission-visible dashboard summary semantics, including directly shared Kanban boards whose module root is not shared.
- Documented attachment folder side-effect rules: upload may create the `attachments/` folder, while read/list/delete paths must not.

### Fixed

- Fixed tenant propagation across services, handlers, WebSocket auth, and collaboration paths.
- Fixed public share access checks so revoked and expired shares are revalidated per request.
- Fixed activity event filtering for stored JSON event types.
- Fixed search and AI permission filtering to avoid exposing unauthorized content.
- Fixed attachment path traversal, hidden-file, metadata filtering, and portability edge cases.
- Kanban attachment deletion now rejects non-attachment files without deleting them or creating a missing `attachments/` folder.
- Kanban dashboard summaries now include directly shared board folders even when `/Workspace/Kanban` itself is not shared.

### Security

- Hardened tenant isolation and authorization contracts across module, share, search, AI, public upload, and attachment flows.

## [0.2.0] - 2026-05-26

### Added

- Real-time WebSocket sync for module edits, with collaborative cursor support and conflict resolution.
- Note bundle architecture: notes now store as folders with `index.md`, `.rustshare.json`, and `events.jsonl`, enabling richer metadata and attachment handling.
- H1-based folder rename on save: editing the top-level heading in a note automatically renames its backing folder.
- Cross-user share isolation tests and public share validation.

### Changed

- **Dependency updates:**
  - Rust: `rand` 0.9 (with API migration), `keyring` 4.0, `uuid` 1.23, and other cargo dependency bumps.
  - Frontend: 37 npm dependency updates including SvelteKit, TanStack Query, and Excalidraw.
  - Docker: Node.js 22 base image update.

### Fixed

- **Meeting note creation:**
  - Fixed routing so folder-backed module artifacts (meetings, standups, kanban, notes, etc.) navigate to their dedicated editor instead of the generic file browser.
  - Added a title prompt in `MeetingsModuleView` instead of silently creating an "Untitled Meeting Note."
  - Fixed default meeting and standup `.rustshare.json` template schemas to match `MeetingMetadata` and `StandupMetadata` structs.
- **Frontend type safety:** Fixed pre-existing svelte-check and eslint errors across `MarkdownEditor`, `KanbanCardModal`, `Topbar.test`, admin forms, and dashboard widgets.
- **Backend tests:** Hardened integration tests for cross-user isolation, validated JSON doctests, and brainstorming handler formatting.
- **CI:** Added `RUSTFS_ALLOW_INSECURE_DEFAULT_CREDENTIALS` to integration test workflow; fixed DCO sign-off checks.

[Unreleased]: https://github.com/kubedoio/rustshare/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/kubedoio/rustshare/compare/v0.3.0-rc.1...v0.3.0
[0.3.0-rc.1]: https://github.com/kubedoio/rustshare/compare/v0.2.0...v0.3.0-rc.1
[0.2.0]: https://github.com/kubedoio/rustshare/compare/v0.1.1...v0.2.0
