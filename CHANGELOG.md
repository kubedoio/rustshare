<!--
This file follows the Keep a Changelog format.
See https://keepachangelog.com/en/1.1.0/ for details.
All notable changes to this project will be documented in this file.
-->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0] - 2026-06-29

### Added

- Added streaming download support for object storage via `ObjectStore::get_stream`, preserving Content-Type and preserving Content-Length when integrity verification does not require EOF validation. Authenticated file downloads, file previews, and public-share downloads now return a streaming body instead of buffering the entire object in memory.
- Added multipart upload streaming to temporary files, with automatic cleanup on success and error, and configurable size limits.
- Added unit and integration tests for large-object streaming upload/download, resumable-upload abort/cleanup, and low-memory `ObjectStore::get_stream` consumption.
- Added request-scoped correlation IDs. Every HTTP request receives an `X-Request-ID` (preserved from the client when valid, otherwise generated), propagated into tracing spans as `request_id`, and returned in response headers.
- Added `X-Tenant-ID` header support for unauthenticated public-share and public-chat-unfurl requests.
- Added tenant-scoped public share link resolution. `get_share_by_token`, `validate_and_create_session`, and `get_public_share_info` now require a `tenant_id` and reject cross-tenant share tokens with `ShareNotFoundByToken`.
- Added `tenant_id` to share-session JWT claims so share-session routes can scope share lookups to the issuing tenant.
- Added HMAC-SHA256 signature verification for incoming chat webhook events.
- Added operational runbooks for backup/restore and security incidents.
- Added production-readiness documentation (`docs/PRODUCTION_READINESS.md`) summarizing completed workstreams, residual risks, and operator checklists.
- Implemented a functional admin SMTP test action. `POST /api/v1/admin/config/smtp/test` now sends a real test email to the acting admin's address using the stored SMTP configuration, replacing the previous `501 Not Implemented` stub.
- Added ACL-aware indexing contract for OKF notes. Indexed note chunks now carry `NoteAclPayload` (tenant, workspace, note id, source file/folder ids, owner, read ACL, visibility, ACL hash/version, embedding policy). `ContentIndexer` supports `index_note`, `search_with_acl`, `update_note_acl`, and `remove_note_chunks`. `NoteService` emits indexing callbacks through an optional `NoteIndexSink`, wired to the shared `ContentIndexer` when AI is enabled.
- Wired real permission-resolver principals into the AI index. `NoteIndexSink` now resolves owner, direct-share, group-share, and public principals so indexed chunks carry accurate `read_acl` values instead of the owner placeholder.
- Persisted the AI vector index in PostgreSQL using pgvector. Added `note_index_chunks` table and `PgVectorStore` implementation; production builds use the database backend while in-memory storage remains available for tests.
- Added frontend conflict-resolution actions for OKF notes, allowing users to choose YAML, folder-name, or a custom title when the note frontmatter and bundle metadata disagree.
- Added purpose color tags for files and notes. Users can assign a color to a file from the file browser and to a note from the note editor; colors are persisted on the file record (for files) and in note sidecar metadata (for notes) and returned in folder/share listings.
- Added full item action menus to Decisions, Meetings, and Standups module lists, matching the Notes module: Show attachments, Rename, Move to folder, Duplicate, and Delete. Added backend `move` and `duplicate` endpoints for all three module types.

### Changed

- Aligned maximum upload size configuration with the existing `MAX_UPLOAD_SIZE_MB` environment variable (default 5000 MB) for authenticated file uploads and file updates.
- Restored distinct trust-boundary limits for public-share uploads (`MAX_PUBLIC_UPLOAD_SIZE`, 100 MB) and resumable chunk uploads (`MAX_CHUNK_SIZE`, 100 MB).
- Enforced HTTPS-only webhook registration; HTTP URLs are allowed only in debug builds or when `RUSTSHARE_ALLOW_HTTP_WEBHOOKS` is set to `"true"` or `"1"`.
- Switched session and CSRF cookie defaults to `Secure=true`. Opting out requires explicitly setting `RUSTSHARE_SESSION_COOKIE_SECURE=false` (or the legacy `SESSION_COOKIE_SECURE=false`).
- CI/CD workflows now generate per-run secrets via `openssl rand` instead of using hardcoded values.
- Updated `docs/security-model.md` and `docs/architecture.md` to document tenant isolation, secret rotation, webhook security, and request correlation IDs.
- Bumped the OpenAPI specification version from `1.0.0` to `2.0.0` to signal breaking contract changes (tenant-scoped share sessions, optional `X-Tenant-ID` on public endpoints, new admin/security response fields).

### Deprecated

### Removed

- Removed the no-op PostgreSQL RLS context middleware. The middleware set `app.current_tenant_id` / `app.current_user_id` on a connection that was returned to the pool before handlers ran, so handler queries never saw the context. Repository-level tenant filtering remains the primary isolation mechanism.
- Removed the `rustshare migrate-notes-okf` CLI binary and the OKF notes migration helpers (`NoteMigrationReport`, `migrate_notes_to_okf`, migration integration tests). No production deployments have legacy notes to migrate, so the migration path is unnecessary.

### Fixed

- Fixed the back button in the Markdown editor to return to the previous `/files` location (including folder and filter state) instead of always going to `/notes`.
- Fixed the Markdown editor formatting toolbar so it remains visible (sticky) at the top of the editor while scrolling through long documents.
- Fixed the app layout height for non-files pages so the Markdown editor fills the available viewport and scrolls internally, keeping both the document header and formatting toolbar visible on long notes.
- Sanitized `Content-Disposition` filename parameters to strip control characters, backslashes, and quotes.
- Fixed resumable upload chunk integrity validation so `Content-MD5` is verified as MD5 instead of being compared to SHA-256 chunk hashes.
- Fixed resumable upload completion to assemble chunks through streaming temporary files instead of materializing full files in memory.
- Fixed concurrent resumable chunk uploads by using conditional chunk object writes and merging upload-session chunk state.
- Fixed ignored backend tests by re-enabling, replacing, or removing them with documented justifications.
- Resolved clippy warnings across all targets.
- Renamed the notes module primary action from "Create from Template" to "New note" and made the action label respect the module configuration instead of a hardcoded string.
- Addressed `cargo audit` advisories for `rustls-webpki` and RSA.
- Fixed permission resolver caching so source-aware lookups preserve owner, direct-share, group-share, inherited, and no-permission sources.
- Fixed inherited folder permission aggregation to select the highest active user share instead of an arbitrary share.
- Added object-store integrity checks for content-addressed `blobs/{sha256}` uploads and downloads.
- Fixed Markdown table preprocessing so backslashes no longer multiply on each save/load round-trip. Cell content is now rendered through markdown-it, preserving inline formatting and consuming escape sequences correctly.
- Fixed Markdown table serialization so cell content is treated as inline text. This prevents unnecessary escaping of block markers such as `1.`, `-`, and `#` at the start of a table cell.
- Fixed the Notes module list so clicking a note opens the note editor (`note.md`) instead of the folder bundle.

### Security

- Added pre-commit/CI secret-scan gate to block hardcoded secrets in CI/CD, config, and shell files.
- Hardened multi-tenant isolation for share links: cross-tenant share tokens are no longer resolved.
- Enforced admin authentication on all `/api/v1/admin/*` routes, including chat integration and replication admin endpoints.
- Removed hardcoded credentials from GitHub Actions workflows.
- Documented required production secrets and rotation guidance in `docs/DEPLOYMENT.md`, `docs/CI_SECRETS.md`, and `.env.example`.
- Hardened chat webhook URLs against SSRF: registration and dispatch now reject loopback, private IPv4, link-local, multicast, CGNAT, localhost, and IPv4-mapped IPv6 addresses, with a 5-second DNS timeout and re-validation at dispatch time to mitigate DNS rebinding.
- Added replay-age checks for incoming chat webhook events: timestamps outside `RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS` (default 300) are rejected without revealing that the failure was a replay.
- Re-verify current folder write permission when completing resumable uploads, and use current public-share permissions instead of stale JWT permission claims for public folder uploads.
- Prevented password-protected public share info from exposing filename, size, MIME type, or folder name before password-backed session creation.
- Restricted private user-share chat unfurls to the share recipient in the requesting tenant.
- Added explicit wrong-tenant regression coverage for infrastructure file and folder repository lookups.
- Disabled implicit object-store bucket creation by default; local/dev deployments can opt in with `RUSTSHARE_OBJECT_STORE_AUTO_CREATE_BUCKET=true`.

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

[Unreleased]: https://github.com/kubedoio/rustshare/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/kubedoio/rustshare/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/kubedoio/rustshare/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/kubedoio/rustshare/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/kubedoio/rustshare/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/kubedoio/rustshare/compare/v0.3.0-rc.1...v0.3.0
[0.3.0-rc.1]: https://github.com/kubedoio/rustshare/compare/v0.2.0...v0.3.0-rc.1
[0.2.0]: https://github.com/kubedoio/rustshare/compare/v0.1.1...v0.2.0
