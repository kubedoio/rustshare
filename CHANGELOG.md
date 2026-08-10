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

### Added

- Durable Integration Events (ADR-0031, v1alpha1) in the new
  `rustshare-integration-events` crate with a transactional PostgreSQL outbox
  (`rustshare-storage::OutboxStore`), a CloudEvents-compatible `IntegrationEvent`
  envelope, and a durable consumer contract. File uploads/updates now publish
  `io.elembra.files.file.created.v1` / `file.updated.v1` events atomically
  with their metadata transaction; events carry the acting principal
  (`elembraActor = principal:<id>`) — authenticated uploads are attributed to
  the acting user, public-share uploads carry no actor, and the file owner is
  never used as a fallback actor. Consumers register durably
  (`integration_consumers` / `integration_consumer_subscriptions`); every
  publish eagerly fans out a pending delivery obligation to each registered
  consumer whose subscription patterns match, so an offline or
  operator-disabled consumer never loses events (retention compacts only
  events whose obligations are all processed). An asynchronous outbox
  dispatcher delivers at-least-once with lease fencing, retry backoff,
  dead-lettering, and a self-healing registration sync each tick. Operators
  get `RUSTSHARE_OUTBOX_*` configuration, an `outbox` readiness component,
  and `outbox_*` metrics (#212). The reference "memory projection" consumer
  used by the integration tests ships in test support only
  (`backend/tests/contracts/reference_consumer.rs`); production runs a
  zero-consumer dispatcher until real consumers land.

- Cross-Application identity/resource contracts (ADR-0032, v1alpha1) in the
  new `rustshare-resource-auth` crate: `PrincipalContext` (user/service/agent
  principals with explicit bounded delegation), opaque `ResourceRef` with
  canonical `elembra://` URIs, the `ResourceOwner` source-authorization
  contract, bounded batch authorization, and a Search/RAG reauthorization
  proof contract. Elembra Files is the first owner adapter, delegating to the
  existing Files permission semantics; the `SourceAuthorizer` is wired into
  `AppState` (#211). Owner registration is validated against the canonical
  `ApplicationRegistry` (unknown Applications or undeclared resource/action
  surfaces fail startup), and contexts whose workspace does not correspond to
  their tenant fail closed before any owner is consulted (1:1
  tenant/workspace invariant).

### Changed

- Replaced the pre-release Module product boundary with Elembra Applications,
  including declarative manifests, tenant/workspace enablement persistence,
  `/apps/...` UI routes, and `/api/v1/applications/...` registry APIs (#210).

### Fixed

- **Fresh installs** — a new migration renames `templates.module_config` to
  `templates.application_config` (the cutover renamed the code but never the
  column), restoring first-boot default template seeding; the pilot release
  job now verifies real backend readiness via the proxied `/health/ready`
  endpoint instead of nginx's static `/health` (#220).
- **Authorization** — creating a group share now requires the caller to hold at
  least the permission being granted on the resource (previously any group
  member could grant a group, including themselves, arbitrary access to any
  resource); moving a folder now requires Edit on the target parent; shared
  recipients can create/rename/move folders and move files inside shared
  folders, and shared Admin recipients can delete mixed-ownership trees
  (#218, #221, #222).
- **Invites** — the emailed invite link is now built from the server's
  configured public URL instead of client-supplied input; invitees are placed
  in the inviter's tenant (not a hardcoded nil tenant); concurrent accepts no
  longer surface as 500s; passwords are length-bounded (#218).
- **Realtime** — WebSocket connections now reject disabled/deleted accounts,
  matching the HTTP layer; the OIDC post-login redirect rejects backslash and
  control-character bypasses; client IP extraction trusts the proxy-appended
  `X-Forwarded-For` entry (nginx now overwrites the header with the real
  remote address), so per-IP rate limits can no longer be evaded by spoofing
  (#218).
- **Uploads** — the resumable-upload completion now verifies the assembled
  blob size against the declared size, and replication jobs reference the
  persisted file-version row (previously a dangling id could fail replication
  on overwrite edits) (#223).
- **Sync** — a failed remote fetch or an unreadable local subtree now aborts
  the sync cycle instead of deleting local/server content; delete planning
  preserves edited files (re-upload/download) instead of destroying them;
  unparseable server timestamps no longer cause infinite resync loops; an
  unknown event cursor returns an explicit error instead of a silent
  "caught up" page (#219, #224).
- **Object GC** — re-enqueuing a released blob resets its attempt history so a
  previously-retried candidate is not re-held after a single transient failure
  (#225).
- **Frontend security** — markdown file previews are sanitized (stored-XSS via
  malicious file contents); external links open with `noopener,noreferrer`;
  OIDC, file download/preview, folder download, and avatar URLs honor the
  configured API base URL; the root page waits for the session bootstrap
  before redirecting (#226).
- **Ops** — the sqlx offline query cache is regenerated to match the current
  schema, removing orphaned Module-era metadata entries.

## [0.7.0] - 2026-08-08

### Added

- Deterministic ascending/descending date sorting for mail lists (#182): both the imported ("Saved to RustShare") list and the remote IMAP folder list accept `sort=date_desc` (default, newest first) or `sort=date_asc` (oldest first), reject unknown values with a 400, and order deterministically by message date with an id/UID tiebreak. The Mail UI gains a sort toggle whose preference is persisted globally in `localStorage`.
- Added safe WebUI editing for eligible vault files. Vaults now have a `write_policy` (`read_only`, `web_editing_enabled`, or `sync_client_only`) defaulting to `read_only`. The WebUI can load and save Markdown/text files through `GET/PUT /api/vault-sync/v1/vaults/{id}/content/{*path}` when `web_editing_enabled` is set, using optimistic revision locking to prevent silent overwrites. Added `PATCH /api/vault-sync/v1/vaults/{id}/write-policy` for policy updates, a vault detail page policy selector, and a `VaultFileEditor` component with dirty state, conflict handling, and save shortcuts.
- Added RustShare Mail Phase 3: IMAP selected import. Users can connect IMAP accounts with encrypted credentials, browse folders and messages, and create import jobs that copy selected messages into RustShare as durable mail artifacts. Includes account management, an import-job worker, audit events, and REST endpoints for accounts, folders, messages, and jobs.
- Added RustShare Mail Phase 4: archive jobs. Users can create folder/date-range IMAP archive jobs that incrementally copy messages into RustShare, resume from the last imported UID, apply optional retention soft-deletion, and retry failed runs with exponential backoff. Includes audit events and REST endpoints under `/api/v1/mail/accounts/{id}/archive-jobs` and `/api/v1/mail/archive-jobs/{id}`. Refs #147.
- Added RustShare Mail Phase 5 WebUI client. Users can manage IMAP accounts, browse folders and message summaries, queue selected imports, view archive/import status, read imported mail with sanitized HTML, inspect attachments, and manage links to RustShare objects.
- Added reliable RustShare Mail move and archive actions. Single and bulk archive buttons file messages into the account's `\Archive`-role folder (with a clear error when none exists), IMAP servers without UID MOVE now fall back to a UIDPLUS-gated COPY + delete sequence instead of failing outright, bulk actions report per-item failures and keep failed messages selected for retry, and toolbar/bulk buttons are disabled while a request is in flight to prevent duplicate submissions. Refs #184.

### Changed

- `rustshare-desktop status` now derives its output from the sync manager state instead of a hardcoded value; the standalone CLI still reports Idle until daemon-state reporting is wired up.
- Releases are now triggered by tag push or manual dispatch only; the release workflow no longer waits for CI to pass on the tagged commit (both the workflow_run auto-trigger and the CI-completion gate were removed). Run the full validation suite before tagging, as documented in `docs/release-process.md`.
- Consolidated the root and backend Cargo workspaces into one unified workspace with a single `Cargo.lock`, removing the nested `backend/Cargo.toml` workspace and eliminating ambiguous dependency resolution.
- Reduced development and test build overhead by setting `debug = 1` in the `dev` and `test` Cargo profiles; this preserves line-level debugging while shrinking the target directory and improving test-linking times.
- Updated CI so documentation-only and frontend-only changes no longer trigger the full Rust workflow; the DCO check now runs in its own always-on workflow.
- Hardened RustShare Mail daily-use workflows with idempotent outbound sends, account-bound sender identities, SMTP timeouts, durable import status, bounded classic search, IMAP special-use folders, modified UTF-7 labels, inline-image rendering, explicit partial-send warnings, safer draft replacement/cleanup, and reference-aware object cleanup queueing.
- Fixed RustShare Mail mailbox action gating so read/unread, archive, and trash buttons each key on their own folder availability, blocked scheme-relative and uppercase-scheme remote images in sanitized HTML, and kept deferred content-addressed blob keys from starving the object cleanup queue.
- Polished RustShare Mail release readiness: the local copy of sent mail now retains its Bcc recipients, reply headers emit RFC 5322 angle-bracketed message ids, mail view events no longer fail reads, import-job listings are bounded, the message-page composer tracks drafts end-to-end (update on re-save, discard after send, SMTP guard, forward attachment race), saving a draft reflects the real save result, the dead folder-mapping settings section and unreachable IMAP TLS options were removed, and the published OpenAPI contract was refreshed with a CI freshness check.
- Hardened RustShare Mail outbound safety: plaintext SMTP modes persisted before the ban are now rejected at send and test time, and send idempotency claims left pending by a crashed send are reclaimed after ten minutes so retries with the same key are never blocked forever. Fixed the integration-test environment-variable toggle so concurrent plaintext-SMTP tests no longer race and unset the override mid-send.
- Fixed a rustls 0.23 startup panic that blocked Gmail/real-provider IMAP and SMTP tests: the server now explicitly installs the `aws-lc-rs` CryptoProvider before any TLS handshake.
- Added an operator opt-in (`RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS=true`) for self-hosted or air-gapped deployments to use internal/private IMAP and SMTP servers; `localhost` remains rejected for SSRF safety.
- Expanded the RustShare-style Mail workspace into a responsive three-pane webmail client with mailbox counts, remote message reading, bulk actions, search, move/star/delete controls, reply/reply-all/forward threading, drafts, attachments, RustShare imports, and synchronization activity. Settings → Mail was redesigned as account list plus selected-account details with collapsible sections, one unified save workflow, saved-vs-replace password states, and a Gmail provider preset.
- IMAP and SMTP connections to internal/private mail servers (opted in via `RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS=true`) now accept invalid or hostname-mismatched TLS certificates by default, since internal servers are commonly self-signed or reached by IP address. Public destinations always keep full verification, and `RUSTSHARE_MAIL_TLS_ACCEPT_INVALID_CERTS=never` restores strict verification for internal servers.
- Fixed IMAP message listing against servers that return raw UTF-8 in ENVELOPE responses (e.g. Stalwart with UTF8=ACCEPT): message summaries are now fetched via `BODY.PEEK[HEADER]` and parsed with mailparse, which handles both RFC 2047 encoded-words and raw UTF-8.
- The compose dialog now uses compact labeled To/Cc/Bcc/Subject rows, an inline attachment strip, and a compact action footer. Forwards no longer emit `In-Reply-To`/`References` headers, and forward drafts no longer persist the original message as a reply target, so forwarded mail starts a fresh thread in recipients' clients.
- Compose now has a rich-text body editor (the same TipTap stack as the note editor) with a compact formatting toolbar; Cc/Bcc fields reveal on demand. Outbound mail and drafts carry a real `text/html` alternative part rendered from sanitized Markdown, so sent mail renders as formatted HTML in recipients' clients instead of wrapped plain text.
- Fixed imported mail and draft listings returning 500: the `mail_messages.reference_ids` column aliasing broke the runtime row mapping in the paginated message and draft queries after a struct rename, which also made imported mail invisible in the workspace.

### Removed

- Desktop real-time WebSocket sync notifications were removed; they were never wired up (the daemon always ran without a WebSocket URL). Periodic 30s full sync is unchanged.
- Removed the unused collaboration (Yjs) editor dependencies from the frontend.

### Fixed

- Reworked Vault Markdown editing into a visible two-pane workspace with the standard RustShare rich-text toolbar, a full-height scrollable editor, and automatic editor focus after selecting a manifest file; plain `.txt` files retain a simple notepad-style editor.
- Quoted the space-containing `RUSTSHARE_DEMO_VIEWER_DISPLAY_NAME` value in `.env.example` and made `scripts/final-launch-smoke.sh` extract only the variables it needs from `.env` via grep/sed instead of sourcing the file, so `.env` values can no longer break or execute inside the smoke script. Refs #132.
- Fixed `scripts/final-launch-smoke.sh` CSRF handling to match the backend's double-submit protection: the smoke script now reads the `rustshare_csrf_token` cookie from its login cookie jar and sends it as the `X-Rustshare-Csrf` header on every mutating authenticated request instead of a static `1`, which previously 403'd all mutations. Refs #132.
- Documented that the auto-generated admin bootstrap password is written to container-local storage once, does not survive container recreation, and must be recorded immediately; `RUSTSHARE_ADMIN_PASSWORD` in `.env` before first start is now documented as the durable alternative across README.md, docs/DEPLOYMENT.md, `.env.example`, and `scripts/pre-flight.sh` output. Refs #132.
- Distinguished a confirmed missing object-storage bucket from unreachable endpoints or rejected credentials at startup: only a real NotFound/NoSuchBucket (HTTP 404) falls into the bucket-creation path, while connection and 403 errors now fail fast with an actionable endpoint/credentials message. Refs #154, #132.
- Documented secret generation (`scripts/pre-flight.sh` or manual values) as a required quickstart step in README.md and docs/DEPLOYMENT.md, added expected startup duration and a first-start troubleshooting subsection (secret validation errors, unreachable RustFS, bootstrap admin password retrieval). Refs #154, #132.
- Extended `scripts/final-launch-smoke.sh` with nginx health and proxied backend readiness assertions and with share revocation coverage (public link returns 404/410 after revocation; internal share disappears from the recipient's list). Refs #154, #132.
- Added a backend liveness healthcheck (`/health` via wget) and `restart: unless-stopped` policies for postgres, rustfs, backend, and nginx to the base `docker-compose.yml`; nginx now proxies `/health/ready` to the backend. Refs #154, #132.
- Made WebUI vault Markdown editing reliable and conflict-safe (#185): structured 409 conflict bodies (`current_rev`, `server_sha256`) now propagate through the frontend API client; a dirty editor warns on refresh/tab close (`beforeunload`), in-app navigation (`beforeNavigate`), and file switching; conflicts show a recovery panel (copy changes to clipboard, download local version, reload server version after confirmation) instead of reload-and-lose; conflicts whose server content is identical to the editor content (SHA-256 match) silently adopt the server revision instead of alarming the user; added HTTP integration coverage for the `/content/*` vault-sync endpoints.
- Fixed mail attachment filenames, metadata, and downloads (#183): imported mail attachments can now be downloaded via `GET /api/v1/mail/messages/{id}/attachments/{attachment_id}` serving the exact stored bytes (object-store blob with linked-file fallback; missing blobs and cross-tenant access return 404), and remote IMAP messages now expose a raw `.eml` download via `GET /api/v1/mail/accounts/{id}/messages/{uid}/source`. The message detail page offers a per-attachment Download action, the remote viewer gains a Download .eml toolbar action, and duplicate attachment filenames are distinguished by an index badge.
- WebUI bug-bash fixes for issue #186 (WB-001 through WB-012): the login page shows "Invalid email or password" instead of a raw "Unauthorized"; dashboard recent activity uses grammatical "You created …" copy and neutral "A file"/"A folder" labels instead of "Unknown"; the files toolbar's icon-only New folder/Upload buttons have accessible names; the note editor header no longer overlaps or clips its actions at 390px; imported mail stays reachable in "Saved to RustShare" without an IMAP account; the mail module restores its list context (mailbox/account/folder/search) after a detail round-trip; the mail bulk-action bar and remote attachment chips wrap/truncate at narrow widths; the shares page fits 390px viewports; the admin sidebar collapses behind a hamburger below md; and date/time display is unified through the shared `format.ts` policy (relative for lists/feeds, one absolute format for details).
- Fixed `scripts/backup-stack.sh` failing during the configuration snapshot: the archive referenced `PRODUCTION_READINESS.md` at the repo root, but the file lives at `docs/PRODUCTION_READINESS.md`, so `tar` errored and the backup aborted before writing `manifest.env`/`SHA256SUMS`. The snapshot now includes `docs/PRODUCTION_READINESS.md` and the backup completes (exit 0, manifest + checksums written).
- Fixed `docker-compose.restore-drill.yml` using `postgres:16-alpine`, which lacks the `vector` extension the v0.7.0 schema requires — the restore drill now uses `pgvector/pgvector:pg16` (matching the base compose image) so the documented restore procedure works against a v0.7.0 backup.

### Security

- Sanitized response filenames on every mail download path (remote/imported attachment, imported/remote `.eml` source) with one shared Content-Disposition builder: an ASCII-only, injection-proof `filename=` fallback (control characters stripped, quotes/backslashes/slashes neutralized, `..` traversal collapsed, length capped, Windows reserved device names prefixed) plus an RFC 5987 `filename*` carrying the safe Unicode original. Storage blob keys are never exposed in responses (#183).
- Added safe asynchronous garbage collection for orphaned global `blobs/<sha256>` objects, with durable coalesced candidates, a 24-hour default grace period, cross-process writer/collector locking, global reference checks, leased workers, conservative retry, metrics, and disabled-by-default operator controls.
- Separated the Notes filename from the first Markdown H1: the note name is now independently editable, changing the H1 does not rename the note, and renaming the note does not rewrite the H1.
- Corrected Kanban comment actor attribution so the comment author matches the authenticated actor.
- Made the internal-mail-server validation tests deterministic by serializing environment-variable mutations with an async mutex and restoring values after each test.
- Restored skipped `FileThumbnail` lifecycle tests and added coverage for prop-change replacement and object-URL cleanup.
- Hardened the production Compose contract by requiring same-host external TLS termination on a dedicated loopback port, preserving the validated upstream HTTPS scheme, probing dependency readiness, and pinning RustFS to an immutable image digest.
- Audited and hardened permission-aware AI indexing. All indexed note chunks now carry a canonical `IndexAclProjection` resolved from the authoritative permission model; retrieval pre-filters by tenant, caller principals, visibility, and embedding policy; missing, malformed, stale, and cross-tenant ACL data fail closed; share revocation and note lifecycle events propagate to the index without requiring a full rebuild. Added backend-agnostic contract tests against both `InMemoryVectorStore` and `PgVectorStore`.
- Mail module: remote images in message previews and imported message bodies are now blocked by default so opening a message never triggers external image requests, with a privacy notice and an explicit per-message "Load remote images" action in both the IMAP preview and the imported message page. `cid:` embedded images now resolve in the IMAP preview via the attachment download endpoint, and remote `srcset` candidates are stripped alongside `src`. The imported-parts endpoint gained an opt-in `load_remote_images=true` query parameter and reports blocked images via the `X-Mail-Blocked-Remote-Images` response header. Refs #181.
- Fixed high-severity `nanoid` advisories (GHSA-28wg-ghj8-5hjv / GHSA-2v37-7h3g-55p8) in the frontend dependency tree: the npm `overrides` now pin `nanoid` to patched releases (`^3.3.17` / `^5.1.16`), clearing the high-severity findings from `npm audit` (two moderate advisories remain, tracked upstream).

### Documentation

- Aligned metadata-backend support and launch-readiness claims around PostgreSQL production deployments and target-environment release gates.

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

[Unreleased]: https://github.com/kubedoio/rustshare/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/kubedoio/rustshare/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/kubedoio/rustshare/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/kubedoio/rustshare/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/kubedoio/rustshare/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/kubedoio/rustshare/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/kubedoio/rustshare/compare/v0.3.0-rc.1...v0.3.0
[0.3.0-rc.1]: https://github.com/kubedoio/rustshare/compare/v0.2.0...v0.3.0-rc.1
[0.2.0]: https://github.com/kubedoio/rustshare/compare/v0.1.1...v0.2.0
