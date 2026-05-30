# Gap Priority Backlog

## Launch Blocker

### LB-01: Source-of-truth drift in module/template contracts

- Problem: Specs/contracts, backend API shape, backend defaults, and frontend defaults disagree on module roots, fields, icons, dashboard widget types, and UI shape.
- Evidence from code/docs: `docs/specs/template-modules-system.md` and `docs/contracts/template-module-contract.md` use `/Notes` and camelCase fields; backend defaults use `/Workspace/Notes` and snake_case; frontend registry uses static `PREDEFINED_MODULES`.
- User impact: Modules may render inconsistently, disabled modules may leak into UX, and future agents will implement against the wrong shape.
- Technical risk: Migration bugs, duplicate roots, broken dashboard/sidebar, inconsistent API clients.
- Suggested fix direction: Declare current canonical module API/schema and update docs/tests first.
- Tests required: API schema contract tests, frontend normalization tests, disabled module route/sidebar/dashboard tests.
- Estimated complexity: M

### LB-02: Incomplete tenant/permission contract coverage across module surfaces

- Problem: File/folder core has permission checks, but module services and module routes are not proven against tenant and share boundaries.
- Evidence from code/docs: Contract G-01/G-02; module services filter by legacy and `/Workspace` paths; module handlers rely on enabled module visibility but object routes use specific services.
- User impact: Unauthorized access risk to notes, boards, decisions, standups, or attachments.
- Technical risk: Critical security regression.
- Suggested fix direction: Add contract tests for each module object route under different users/tenants/shares before refactoring.
- Tests required: Notes, meetings, standups, decisions, kanban, brainstorming tenant and permission denial tests.
- Estimated complexity: L

### LB-03: Sharing service split

- Problem: Intended unified `ShareService` coexists with deprecated `UserShareService`.
- Evidence from code/docs: `backend/crates/core/src/services/user_share_service.rs` says deprecated; `AppState` still wires `user_share_service`; share routes call both share and user share handlers.
- User impact: Revocation, notification, permission inheritance, and auditing may differ by share creation path.
- Technical risk: Security-sensitive inconsistent behavior.
- Suggested fix direction: Freeze behavior with tests, then migrate route handlers to unified service in a later implementation pass.
- Tests required: Same-resource user/group/public share lifecycle through every route, including revocation and notification.
- Estimated complexity: L

### LB-04: Auditability is not complete enough for contract G-03

- Problem: Security-sensitive actions are not uniformly logged as durable audit events.
- Evidence from code/docs: Contract G-03; admin audit exists; share access log exists; file/module actions rely on event store or local frontend activity; `create_from_template` logs to `admin_actions` for user object creation.
- User impact: Admins cannot reliably answer who did what.
- Technical risk: Compliance and incident-response gap.
- Suggested fix direction: Define audit taxonomy and map every sensitive route to durable audit emission.
- Tests required: Audit assertion tests for login, upload, download, share create/revoke, delete/restore, public access, module create/update/delete.
- Estimated complexity: L

## High Priority

### HP-01: Editor attachment backend contract unclear

- Problem: Attachment rules are documented, frontend validation exists, but backend document/attachment APIs are not clearly implemented.
- Evidence: `docs/contracts/editor-api-contract.md` specifies `/api/editor/documents`; routes do not define them; frontend uses file and module APIs.
- User impact: Rich document attachments may behave differently across notes, kanban cards, and public shares.
- Technical risk: Path traversal, hidden metadata exposure, broken portability.
- Suggested fix direction: Decide whether file APIs satisfy the editor contract or add document-level APIs; then test.
- Tests required: Upload/list/delete attachment, read/write permission, public rendering, hidden metadata exclusion.
- Estimated complexity: M/L

### HP-02: Dashboard not fully registry-driven

- Problem: Module registry exists, but dashboard quick actions and summaries use static frontend definitions in places.
- Evidence: `frontend/src/routes/(app)/dashboard/+page.svelte` imports `getEnabledModules` and hardcoded quick actions; module API exists.
- User impact: Admin module changes may not produce predictable UI.
- Technical risk: Contract drift and disabled module exposure.
- Suggested fix direction: Use backend `listEnabledModules` and `workspace-surface` consistently.
- Tests required: Disabled modules hidden; backend UI config changes reflected; unknown widget fallback.
- Estimated complexity: M

### HP-03: Realtime coverage uneven by feature

- Problem: WebSocket manager handles core files/folders/shares and some modules, but not all module-specific operations equally.
- Evidence: `manager.ts` handles brainstorming/meeting/decision/standup, but kanban operations appear to rely on query refresh/manual refetch.
- User impact: Multi-user collaboration feels stale or unpredictable.
- Technical risk: Data conflicts and stale UI.
- Suggested fix direction: Define event taxonomy per module and add invalidation handlers/tests.
- Tests required: Backend event emitted and frontend invalidates for kanban board/card/label/assignee/checklist/attachment changes.
- Estimated complexity: M

### HP-04: Operational readiness incomplete

- Problem: Health, metrics, backup status, audit export, and queue/index lag are not complete.
- Evidence: Contract O-01/O-04; server exposes `/health`; scripts exist; no comprehensive readiness surface observed.
- User impact: Operators cannot trust production state.
- Technical risk: Poor incident recovery and hidden dependency failures.
- Suggested fix direction: Add readiness endpoints and operator status data after tests are defined.
- Tests required: Dependency readiness tests, backup status tests, metrics endpoint smoke tests.
- Estimated complexity: L

### HP-05: Workspace/team model underdefined

- Problem: Product spec requires organization/workspace/tenant model and owner/admin/member/guest roles; implementation mostly has tenant ID, groups, admin boolean, and workspace paths.
- Evidence: `docs/adr/01-product-spec.md`; `users` has `is_admin`; groups exist; no full role model surfaced.
- User impact: Admins and users may not understand boundaries or responsibilities.
- Technical risk: Authorization model grows ad hoc.
- Suggested fix direction: Document current model honestly and add role model only if required by contract.
- Tests required: Role matrix tests once defined.
- Estimated complexity: M/L

## Medium Priority

### MP-01: Search maturity gaps

- Problem: Search exists but authorization, module artifacts, deleted/revoked/expired propagation need broader guarantees.
- Evidence: Contract Q-01/Q-04; `search_authorization_contract.rs`; global search UI.
- User impact: Missing or stale search results, potential leakage if filters fail.
- Technical risk: Security and trust issue.
- Suggested fix direction: Expand search fixtures and define indexing SLA.
- Tests required: Shared folder, module artifact, deleted/revoked/expired, tenant isolation search tests.
- Estimated complexity: M

### MP-02: Recent/activity model fragmented

- Problem: Durable event store exists but frontend recent activity uses local store and file/module summaries.
- Evidence: `activityStore` in dashboard; admin audit and event store separate.
- User impact: Activity history disappears or differs per device.
- Technical risk: UX inconsistency and audit confusion.
- Suggested fix direction: Add server activity projection filtered by permissions.
- Tests required: Activity feed permission/filter/order tests.
- Estimated complexity: M

### MP-03: Attachment and metadata shapes vary by module

- Problem: Notes, rich documents, and kanban use different attachment/metadata fields.
- Evidence: `NoteAttachment`, `KanbanCardAttachment`, `RichMarkdownAttachment`.
- User impact: Attachments do not behave consistently.
- Technical risk: Migration complexity and public rendering gaps.
- Suggested fix direction: Define shared attachment metadata adapter and migration rules.
- Tests required: Cross-module attachment serialization tests.
- Estimated complexity: M

### MP-04: Public note sharing is separate from public file/share service

- Problem: Notes have `public_share_id` side index separate from public share links.
- Evidence: `NoteService::public_share_key`, `/api/v1/public/notes/{share_id}`.
- User impact: Different sharing controls for notes vs files/folders.
- Technical risk: Revocation/audit/policy mismatch.
- Suggested fix direction: Decide whether public notes are a special renderer for normal share links.
- Tests required: Public note expiry/revocation/audit/policy tests.
- Estimated complexity: M

### MP-05: Legacy root path compatibility needs migration policy

- Problem: Services list both legacy and `/Workspace` roots.
- Evidence: meeting/standup/decision/brainstorming filters accept both; docs and tests differ.
- User impact: Duplicate module roots or hidden content after migration.
- Technical risk: Data migration and listing inconsistency.
- Suggested fix direction: Formalize legacy read compatibility and new-write policy.
- Tests required: Legacy content visible, new content writes only canonical root, migration idempotent.
- Estimated complexity: M

## Later Improvement

### LI-01: Pinned/saved/recent across artifact types

- Problem: Starred files/folders, note pinned, recent artifacts are fragmented.
- Evidence: file star endpoints, note metadata `pinned`, dashboard recent artifacts.
- User impact: Inconsistent saved/recent UX.
- Technical risk: Low if documented, higher as modules grow.
- Suggested fix direction: Add generalized artifact preference model later.
- Tests required: Preference visibility/order tests.
- Estimated complexity: M

### LI-02: Integration event taxonomy

- Problem: Webhooks/chat integrations exist but event naming and payload guarantees need hardening.
- Evidence: admin webhooks support events, chat handlers, chat contract tests.
- User impact: Integrations are harder to build reliably.
- Technical risk: Medium.
- Suggested fix direction: Publish versioned integration event contract.
- Tests required: Payload schema and permission-filtered unfurl tests.
- Estimated complexity: M

### LI-03: Printable/PDF exports

- Problem: Printable rendering exists; export maturity is lower priority.
- Evidence: ADR-0022, `PrintableDocumentView` tests.
- User impact: Export polish gap.
- Technical risk: Low.
- Suggested fix direction: Keep browser print first; add E2E only when needed.
- Tests required: Printable view and route tests.
- Estimated complexity: S/M

