# Implementation Prompt Plan

These prompts are intentionally small. Each one should be executable independently by an LLM coding agent. Do not run them as one giant prompt.

## Prompt 1: Freeze Canonical Module Schema

Goal: Reconcile module/template source-of-truth docs with current canonical API behavior.

Files likely involved:

- `docs/specs/template-modules-system.md`
- `docs/contracts/template-module-contract.md`
- `docs/contracts/module-ui-contract.md`
- `frontend/src/lib/api/types.ts`
- `backend/crates/core/src/domain/module.rs`
- `backend/crates/core/src/domain/template.rs`

Exact behavior to implement:

- Decide and document canonical root path policy, likely `/Workspace/<Module>`.
- Decide and document serialized API field names, including snake_case backend payloads and frontend normalization if retained.
- Document accepted legacy UI fields (`modulePage`) and current nested `dashboard.widget` support.
- Document approved icon registry based on implementation or reduce implementation to contract in a later prompt.

What not to change:

- Do not change runtime behavior.
- Do not migrate data.
- Do not refactor module services.

Tests to add/update:

- Add or update schema snapshot tests for backend module/template JSON if available.
- Update frontend normalization tests to assert canonical and legacy payload handling.

Acceptance criteria:

- Docs no longer conflict on module roots, field names, widget structure, and icon registry.
- Tests assert the documented serialized shape.

## Prompt 2: Add Module Tenant/Permission Contract Tests

Goal: Prove module object routes do not cross tenant or permission boundaries.

Files likely involved:

- `backend/tests/contracts/tenant_isolation_contract.rs`
- `backend/tests/notes_test.rs`
- `backend/tests/decisions_test.rs`
- `backend/tests/kanban_test.rs`
- `backend/tests/brainstorming_test.rs`
- New helper(s) under `backend/tests/contracts/common.rs`

Exact behavior to implement:

- Create fixtures for two tenants/users.
- For notes, meetings, standups, decisions, kanban, and brainstorming, assert user/tenant B cannot get/list/update/delete tenant A objects unless a valid share path exists.
- Include attachment/source/detail endpoints where applicable.

What not to change:

- Do not alter production code unless a test exposes a confirmed bug and the prompt is explicitly extended.
- Do not broaden sharing semantics.

Tests to add/update:

- Backend contract tests only.

Acceptance criteria:

- Tests fail if any module route leaks cross-tenant or unauthorized data.

## Prompt 3: Add Share Revocation Session Tests

Goal: Verify revoked public shares and internal/group shares deny subsequent access promptly.

Files likely involved:

- `backend/tests/contracts/share_link_contract.rs`
- `backend/tests/contracts/public_upload_only_contract.rs`
- `backend/tests/group_sharing_test.rs`
- `backend/crates/core/src/services/share_service.rs`

Exact behavior to implement:

- Create public share, create session token, access successfully, revoke share, assert same session token is denied.
- Create group share, access as member, remove member or revoke share, assert denied.
- Assert audit/access log captures denied post-revocation attempt if contract requires.

What not to change:

- Do not migrate deprecated `UserShareService` yet.
- Do not change token TTL unless required by existing contract.

Tests to add/update:

- Backend contract tests.

Acceptance criteria:

- Active sessions do not bypass revocation.
- Group membership/share revocation takes effect on next access.

## Prompt 4: Make Dashboard Registry-Driven

Goal: Remove hardcoded module availability from the authenticated dashboard.

Files likely involved:

- `frontend/src/routes/(app)/dashboard/+page.svelte`
- `frontend/src/lib/api/modules.ts`
- `frontend/src/lib/modules/workspaceSurface.ts`
- `frontend/src/lib/components/dashboard/*`
- `frontend/src/lib/modules/registry.ts`

Exact behavior to implement:

- Fetch enabled modules from backend via `listEnabledModules`.
- Render module summaries and quick actions from module UI config.
- Preserve current UX where backend supplies equivalent config.
- Fall back safely for unknown widget/layout/action.

What not to change:

- Do not remove existing module views.
- Do not change backend APIs in this prompt.

Tests to add/update:

- Frontend unit tests for disabled module hidden from dashboard.
- Frontend tests for unknown widget fallback.
- Route/dashboard test asserting API-provided module config drives visible actions.

Acceptance criteria:

- Disabling a module in backend payload hides dashboard widget and quick action.
- Static `PREDEFINED_MODULES` is not the dashboard source of availability.

## Prompt 5: Define Audit Event Taxonomy and Add File Audit Tests

Goal: Convert contract G-03 into executable coverage for core file actions.

Files likely involved:

- `docs/security-model.md`
- `docs/adr/02-contract.md` if taxonomy reference is needed
- `backend/tests/admin_audit_test.rs`
- `backend/tests/file_operations.rs`
- `backend/crates/storage/src/event_store.rs`
- `backend/server/src/handlers/admin/audit.rs`

Exact behavior to implement:

- Document audit event names for upload, download, replace, rename, move, delete, restore, permanent delete.
- Add tests asserting durable audit/event records are created.

What not to change:

- Do not redesign audit storage in this prompt.
- Do not add module audit events yet.

Tests to add/update:

- Backend tests asserting audit/event records for core file operations.

Acceptance criteria:

- Security-sensitive file operations have durable audit evidence.

## Prompt 6: Add Backend Attachment Security Tests

Goal: Enforce attachment path and hidden metadata invariants server-side.

Files likely involved:

- `backend/server/src/services/note_service.rs`
- `backend/server/src/services/kanban_service.rs`
- `backend/server/src/handlers/files.rs`
- `backend/tests/notes_test.rs`
- `backend/tests/kanban_test.rs`
- `docs/contracts/attachment-contract.md`

Exact behavior to implement:

- Add tests for traversal, absolute path, `.rustshare*`, `index.editor.json`, duplicate filenames, write permission, read permission.
- Cover public rendering/listing so hidden files are not exposed.

What not to change:

- Do not introduce a new attachment API unless tests show current APIs cannot express the contract.

Tests to add/update:

- Backend tests for note and kanban attachments, plus public share rendering if supported.

Acceptance criteria:

- Server rejects unsafe attachment names/paths and never lists hidden metadata as attachments.

## Prompt 7: Normalize Kanban Realtime Events

Goal: Bring kanban realtime behavior to the same maturity as other module events.

Files likely involved:

- `backend/server/src/services/kanban_service.rs`
- `backend/crates/core/src/events/types.rs`
- `frontend/src/lib/websocket/events.ts`
- `frontend/src/lib/websocket/manager.ts`
- `backend/tests/kanban_test.rs`
- Frontend WebSocket manager tests if present or new tests.

Exact behavior to implement:

- Define kanban event payloads for board/card/label/assignee/checklist/attachment changes.
- Emit events from kanban service mutations.
- Frontend invalidates kanban board/list/card queries on those events.

What not to change:

- Do not alter kanban storage layout.
- Do not add collaborative editing/locking.

Tests to add/update:

- Backend event emission tests.
- Frontend event handler invalidation tests.

Acceptance criteria:

- Two clients see kanban query invalidation after board/card changes.

## Prompt 8: Decide Editor Document API Strategy

Goal: Resolve whether `docs/contracts/editor-api-contract.md` is implemented by file APIs or needs dedicated routes.

Files likely involved:

- `docs/contracts/editor-api-contract.md`
- `backend/server/src/routes.rs`
- `backend/server/src/handlers/files.rs`
- `backend/server/src/handlers/notes.rs`
- `frontend/src/lib/editor/*`
- `frontend/src/lib/api/notes.ts`
- `frontend/src/lib/api/files.ts`

Exact behavior to implement:

- Write a short ADR or update the contract with one of two decisions:
  - file/note/module APIs are the editor API for MVP, with exact route mapping; or
  - add `/api/editor/documents/*` routes in a later prompt.
- Add tests for whichever mapping is chosen.

What not to change:

- Do not implement a new editor API in this decision prompt unless explicitly selected and scoped.

Tests to add/update:

- Contract mapping tests or route tests.

Acceptance criteria:

- No ambiguity remains about how editor get/save/upload/list/delete behavior is fulfilled.

## Prompt 9: Add Operational Readiness Endpoint

Goal: Start implementing contract O-01 without a broad observability rewrite.

Files likely involved:

- `backend/server/src/routes.rs`
- `backend/server/src/main.rs`
- `backend/server/src/handlers/*` or new `health.rs`
- `backend/crates/storage/src/*`
- `docs/PRODUCTION_READINESS.md`

Exact behavior to implement:

- Add `/ready` or `/api/v1/health/ready` with checks for database/metadata, object storage, event store/broadcaster, auth/session capability, optional index/AI status.
- Return machine-readable component statuses.

What not to change:

- Do not add metrics or backup status in this prompt.
- Do not fail readiness for disabled optional AI.

Tests to add/update:

- Backend readiness route tests for healthy and simulated dependency-failure cases.

Acceptance criteria:

- Operators can distinguish process alive from dependencies ready.

## Prompt 10: Formalize Legacy Module Root Migration

Goal: Prevent duplicate or hidden module data during `/Notes` to `/Workspace/Notes` transition.

Files likely involved:

- `docs/specs/template-modules-system.md`
- `docs/contracts/template-module-contract.md`
- `backend/server/src/services/note_service.rs`
- `backend/server/src/services/meeting_service.rs`
- `backend/server/src/services/standup_service.rs`
- `backend/server/src/services/decision_service.rs`
- `backend/server/src/services/brainstorming_service.rs`
- `backend/server/src/services/kanban_service.rs`
- `frontend/src/lib/modules/modulePaths.ts`

Exact behavior to implement:

- Document: new writes go to `/Workspace/<Module>`; legacy roots are read-compatible only.
- Add tests that legacy content remains visible and new content does not create legacy roots.

What not to change:

- Do not migrate existing data in this prompt.
- Do not delete legacy compatibility.

Tests to add/update:

- Backend module listing/create tests and frontend module path tests.

Acceptance criteria:

- Root path behavior is deterministic and covered by tests.

