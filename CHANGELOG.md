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

- Added request-scoped PostgreSQL RLS context middleware that sets `app.current_tenant_id` and `app.current_user_id` for authenticated requests. Repository-level tenant filtering remains the primary isolation mechanism.

### Changed

### Deprecated

### Removed

### Fixed

### Security

- Hardened chat integration webhooks:
  - Added signature verification for incoming chat webhook events.
  - Enforced HTTPS-only webhook registration (HTTP only allowed in debug builds or when `RUSTSHARE_ALLOW_HTTP_WEBHOOKS` is set to `"true"` or `"1"`).
  - Sanitized `Content-Disposition` filename parameters to strip control characters and backslashes.
- Enforced admin authentication on chat integration admin routes and replication admin routes.
- Switched to secure session cookie defaults in the production Docker Compose file.

## [0.5.1] - 2026-06-12

### Fixed

- Aligned `Cargo.lock` and `backend/Cargo.lock` with the `0.5.0` workspace version. The `v0.5.0` tag shipped with internal crates still listed as `0.1.0` in the lockfiles; this hotfix regenerates both lockfiles and bumps `backend/Cargo.toml` `workspace.package.version` to `0.5.0` for consistency.

## [0.5.0] - 2026-06-12

### Added

- Added automatic OpenAPI specification generation (`/api-docs/openapi.json`) and Swagger UI integration via utoipa.
- Added optional bearer-token authentication for the Prometheus `/metrics` endpoint (`METRICS_API_TOKEN`).
- Added pagination support for file, folder, share, and module list endpoints.
- Added retention cleanup support and admin replication summary endpoints.
- Added OpenAPI annotation tooling (`backend/scripts/add_openapi_annotations.py`).

### Changed

- Redesigned the workspace overview dashboard to be backend-driven by enabled module metadata.
- Bumped Rust and npm dependencies via Dependabot rollup.
- Hardened CI pipelines: added PostgreSQL service to dependency checks, pinned `aws-smithy-types` and `time` for build compatibility, and enforced DCO sign-off.

### Removed

- Removed stale root files and directories: `CLAUDE.md`, `convert_files.py`, `quick-fix.sh`, `test-deployment.sh`, root `package-lock.json`, and the `rustshare_public_preview_handover/` directory.

### Fixed

- Fixed unbounded list views in the frontend by walking paginated endpoints for decisions, standups, meetings, shares, kanban boards, and brainstorming boards.
- Fixed CSRF lockout when a browser holds a stale session cookie but no CSRF cookie by exempting `POST /api/v1/auth/login` from CSRF checks.
- Fixed hardcoded database credentials in test code; tests now require `DATABASE_URL` or a local `.env` file.
- Fixed overlapping `/openapi.json` route.

### Security

- Removed hardcoded credentials from scripts and test helpers.
- Restricted access to the Prometheus `/metrics` endpoint with an optional bearer token.
- Improved dependency auditing with `cargo-deny` and `cargo-audit` configurations.

## [0.4.0] - 2026-06-05

### Added

- Added third-party vault synchronization with Obsidian vault support.
- Added `/api/vault-sync/v1` endpoints for vault creation, listing, manifest retrieval, file upload/download/delete/rename, and device registration.
- Added the `rustshare-vault-sync` Obsidian plugin for manual vault synchronization.
- Added vault file metadata, badges, and Obsidian deep links in the web UI.
- Added undo and redo controls to the markdown and note editor toolbars.

### Changed

- Hardened vault sync state convergence and consistency behavior.
- Reduced the vault sync PR surface while preserving the v1 API contract.

### Fixed

- Fixed file content API prefix handling.
- Fixed folder drag-and-drop uploads in the web UI so dropped folders preserve their hierarchy.
- Fixed PDF previews in the web UI by allowing same-origin framing for file preview responses.
- Fixed vault sync atomic revision increments for upload, delete, and rename operations.
- Fixed vault sync handling for non-UUID device IDs when creating vaults.
- Fixed vault sync RLS, deleted-file filtering, API contract edge cases, content length handling, and unique-name behavior.
- Fixed the Obsidian plugin to detect MIME types and avoid unnecessary CSRF headers.

### Security

- Added vault sync authorization and tenant isolation coverage.

## [0.3.0] - 2026-06-02

Stable release for the `0.3.0` release line.

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

[Unreleased]: https://github.com/kubedoio/rustshare/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/kubedoio/rustshare/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/kubedoio/rustshare/compare/v0.3.0-rc.1...v0.3.0
[0.3.0-rc.1]: https://github.com/kubedoio/rustshare/compare/v0.2.0...v0.3.0-rc.1
[0.2.0]: https://github.com/kubedoio/rustshare/compare/v0.1.1...v0.2.0
