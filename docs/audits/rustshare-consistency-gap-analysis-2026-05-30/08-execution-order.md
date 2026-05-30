# Execution Order

This plan turns the audit into a safest-first implementation sequence. It intentionally puts contracts, tests, data integrity, and permission behavior before UI polish.

Risk levels:

- Low: documentation or isolated tests; unlikely to change runtime behavior.
- Medium: scoped runtime changes behind existing APIs.
- High: security, storage, data migration, auth, sharing, or broad API behavior.
- Critical: changes that can expose data, break revocation, corrupt storage, or migrate durable state.

## Dependency Summary

Can run independently after this plan:

- Step 1: Contract/schema freeze.
- Step 2: Module tenant/permission test expansion.
- Step 3: Share revocation/session test expansion.
- Step 4: Editor API strategy decision.
- Step 5: Legacy module root policy tests.
- Step 10: Operational readiness endpoint design/tests, if kept to additive readiness only.

Should wait for prior fixes:

- Step 6 depends on Steps 1 and 5.
- Step 7 depends on Steps 2 and 3.
- Step 8 depends on Steps 3 and 7.
- Step 9 depends on Steps 1, 2, and 6.
- Step 11 depends on Steps 4 and 7.
- Step 12 depends on Step 11.
- Step 13 depends on Steps 2, 7, and 8.
- Step 14 depends on Steps 6 and 9.
- Step 15 depends on Steps 7, 8, 10, and 13.

## Recommended Safest Order

### Step 1: Freeze Canonical Module and Template Contracts

Why it matters:

The audit found conflicting roots, field names, UI shape, widget types, and icon registry rules. Any code change before this risks implementing the wrong contract.

Risk level: Low.

Dependencies: None.

Files likely affected:

- `docs/specs/template-modules-system.md`
- `docs/contracts/template-module-contract.md`
- `docs/contracts/module-ui-contract.md`
- `docs/specs/webui-dashboard-sidebar-integration.md`
- `frontend/src/lib/modules/workspaceSurface.test.ts`
- `frontend/src/lib/modules/modulePaths.test.ts`
- Backend module/template schema tests if present or newly added.

Tests required:

- Frontend normalization tests for canonical and legacy UI payloads.
- Backend/API schema snapshot or serialization tests for module/template payloads.

Acceptance criteria:

- Docs identify one canonical module root policy, likely `/Workspace/<Module>`.
- Docs identify canonical serialized field names and compatibility aliases.
- Tests fail if backend/frontend module shape drifts from the documented contract.

Codex pass size: One pass.

### Step 2: Add Module Tenant and Permission Contract Tests

Why it matters:

Permissions and tenant isolation are launch-blocking. The safest first implementation work is to add tests that reveal leaks before changing runtime behavior.

Risk level: Low for tests; Critical if failures expose bugs.

Dependencies: None, but Step 1 helps name canonical module roots.

Files likely affected:

- `backend/tests/contracts/tenant_isolation_contract.rs`
- `backend/tests/contracts/common.rs`
- `backend/tests/notes_test.rs`
- `backend/tests/decisions_test.rs`
- `backend/tests/kanban_test.rs`
- `backend/tests/brainstorming_test.rs`
- New module contract tests for meetings/standups if needed.

Tests required:

- Cross-tenant list/get/update/delete denial for notes, meetings, standups, decisions, kanban, brainstorming.
- Same-tenant unauthorized user denial where no share exists.
- Shared resource access only through intended share paths.
- Attachment/source/detail endpoint denial tests.

Acceptance criteria:

- Every module object route has at least one negative tenant/permission contract test.
- Tests can fail without requiring production code changes in the same pass.

Codex pass size: Split further. Do one module family per pass if fixtures are complex.

### Step 3: Add Share Revocation and Active Session Tests

Why it matters:

Share revocation correctness is a hard security contract. Active public share sessions and group membership changes must not retain stale access.

Risk level: Low for tests; Critical if failures expose bugs.

Dependencies: None.

Files likely affected:

- `backend/tests/contracts/share_link_contract.rs`
- `backend/tests/contracts/public_upload_only_contract.rs`
- `backend/tests/group_sharing_test.rs`
- `backend/tests/contracts/common.rs`

Tests required:

- Public read link session works before revoke and fails after revoke.
- Upload-only session cannot browse existing unrelated content.
- Group share access fails after share revoke.
- Group share access fails after membership removal.
- Denied post-revocation access is auditable if current contract requires it.

Acceptance criteria:

- Revocation tests cover public, internal, and group share paths.
- Tests explicitly cover already-issued session tokens.

Codex pass size: One pass for tests only; split if fixtures are unstable.

### Step 4: Decide Editor Document API Strategy

Why it matters:

The editor contract allows using existing file APIs only if sufficient. The current ambiguity blocks safe attachment and rich-document work.

Risk level: Low.

Dependencies: None.

Files likely affected:

- `docs/contracts/editor-api-contract.md`
- `docs/contracts/rich-document-contract.md`
- `docs/specs/attachments-and-assets.md`
- Possibly a short ADR under `docs/adr/`.
- Test files documenting route mapping.

Tests required:

- If existing APIs are canonical: mapping tests for get/save/upload/list/delete behavior.
- If dedicated APIs are selected: route contract tests can be stubbed as pending/ignored until implementation.

Acceptance criteria:

- The repo has one explicit editor API strategy.
- The strategy says what not to implement yet.
- Attachment and revision/conflict behavior expectations are testable.

Codex pass size: One pass.

### Step 5: Formalize Legacy Module Root Policy

Why it matters:

Services currently recognize both legacy roots and `/Workspace` roots. Without a policy, later changes may duplicate or hide user data.

Risk level: Low for docs/tests; High if runtime migration is attempted.

Dependencies: Step 1 preferred.

Files likely affected:

- `docs/specs/template-modules-system.md`
- `docs/contracts/template-module-contract.md`
- `frontend/src/lib/modules/modulePaths.ts`
- `frontend/src/lib/modules/modulePaths.test.ts`
- Backend module service tests.

Tests required:

- Legacy `/Notes`, `/Meetings`, `/Standups`, `/Decisions`, `/Kanban`, `/Brainstorming` data remains visible.
- New creates write only to `/Workspace/<Module>`.
- No duplicate root is created when canonical root exists.

Acceptance criteria:

- Compatibility is read-only unless explicitly documented.
- Tests protect old data visibility and new-write canonical behavior.

Codex pass size: One pass if tests can use existing helpers; split if backend fixtures are needed.

### Step 6: Fix Module Registry and Template Schema Drift

Why it matters:

After contracts and tests are frozen, backend/frontend schema mismatches can be corrected safely.

Risk level: Medium.

Dependencies: Steps 1 and 5.

Files likely affected:

- `backend/server/src/services/module_service.rs`
- `backend/server/src/services/template_service.rs`
- `backend/crates/core/src/domain/module.rs`
- `backend/crates/core/src/domain/template.rs`
- `frontend/src/lib/modules/workspaceSurface.ts`
- `frontend/src/lib/modules/registry.ts`
- `frontend/src/lib/api/types.ts`

Tests required:

- Backend module/template schema tests.
- Frontend normalization and fallback tests.
- Disabled module hidden from API response where intended.

Acceptance criteria:

- Backend defaults match canonical docs.
- Frontend accepts canonical payloads without relying on stale static definitions.
- Legacy payload compatibility remains covered where promised.

Codex pass size: Split further: backend schema/defaults first, frontend normalization second.

### Step 7: Fix Permission and Tenant Bugs Exposed by Module Tests

Why it matters:

Tests from Step 2 may expose actual leaks. These must be fixed before UI or feature work.

Risk level: High.

Dependencies: Step 2; Step 5 if failures involve root path behavior.

Files likely affected:

- `backend/server/src/handlers/notes.rs`
- `backend/server/src/handlers/meetings.rs`
- `backend/server/src/handlers/standups.rs`
- `backend/server/src/handlers/decisions.rs`
- `backend/server/src/handlers/kanban.rs`
- `backend/server/src/handlers/brainstorming.rs`
- `backend/server/src/services/*_service.rs`
- `backend/crates/core/src/services/permission_resolver.rs`

Tests required:

- All tests added in Step 2 pass.
- Existing module tests still pass.
- Any changed service adds focused regression tests.

Acceptance criteria:

- No module route returns another tenant's data.
- No module write succeeds without required permission.
- Fixes use existing permission resolver/file/folder service patterns where possible.

Codex pass size: Split further. One module or permission class per pass.

### Step 8: Fix Share Revocation Bugs and Freeze Unified Share Behavior

Why it matters:

The audit found two sharing generations. Revocation must be correct before migrating or polishing share UX.

Risk level: High.

Dependencies: Step 3; Step 7 if module shares are affected.

Files likely affected:

- `backend/crates/core/src/services/share_service.rs`
- `backend/crates/core/src/services/user_share_service.rs`
- `backend/server/src/handlers/shares.rs`
- `backend/server/src/handlers/public_shares.rs`
- `backend/server/src/handlers/groups.rs`
- `backend/crates/core/src/services/permission_resolver.rs`

Tests required:

- All Step 3 revocation/session tests pass.
- Existing share/group/public upload tests pass.
- Regression test for permission cache invalidation or no stale grants.

Acceptance criteria:

- Active public share sessions cannot bypass revocation.
- Group membership/share revocation is effective on next access.
- Behavior is identical or explicitly mapped across old and unified share paths.

Codex pass size: Split further. Do public share sessions first, then group/internal sharing.

### Step 9: Make Dashboard and Sidebar Fully Registry-Driven

Why it matters:

This is frontend/product behavior, but it depends on trusted registry and permission data. It should come after schema and permission work.

Risk level: Medium.

Dependencies: Steps 1, 2, and 6.

Files likely affected:

- `frontend/src/routes/(app)/dashboard/+page.svelte`
- `frontend/src/lib/layout/LeftRail.svelte`
- `frontend/src/lib/components/dashboard/*`
- `frontend/src/lib/modules/workspaceSurface.ts`
- `frontend/src/lib/api/modules.ts`
- `frontend/src/lib/modules/registry.ts`

Tests required:

- Disabled module hidden from dashboard, sidebar, and quick actions.
- Unknown widget/layout/action falls back safely.
- Backend UI config changes are reflected by frontend.
- Static registry is no longer the source of availability for authenticated dashboard behavior.

Acceptance criteria:

- Dashboard and sidebar render from backend enabled modules and user-visible config.
- Hardcoded quick actions do not expose disabled/unauthorized modules.

Codex pass size: Split further. Dashboard first, sidebar/route states second.

### Step 10: Add Operational Readiness Endpoint

Why it matters:

Readiness is operational safety, not UI polish. This can proceed independently if it stays additive.

Risk level: Medium.

Dependencies: None for additive endpoint; later metrics/backup status depend on storage and audit work.

Files likely affected:

- `backend/server/src/routes.rs`
- `backend/server/src/main.rs`
- `backend/server/src/handlers/health.rs` or equivalent new file.
- `backend/server/src/state.rs`
- `docs/PRODUCTION_READINESS.md`

Tests required:

- Healthy readiness response includes metadata/database, object storage, event delivery, auth/session, optional AI/index status.
- Optional disabled components do not fail readiness.
- Simulated dependency failure returns degraded/not-ready status.

Acceptance criteria:

- Operators can distinguish process liveness from dependency readiness.
- Endpoint is machine-readable and stable.

Codex pass size: One pass if dependency checks are simple; split if storage checks need new abstractions.

### Step 11: Add Backend Attachment Security and Portability Tests

Why it matters:

Attachments are a security boundary: path traversal, hidden metadata exposure, and public rendering leaks are high-impact.

Risk level: Low for tests; High if failures expose bugs.

Dependencies: Step 4; Step 7 preferred.

Files likely affected:

- `backend/tests/notes_test.rs`
- `backend/tests/kanban_test.rs`
- `backend/tests/contracts/public_upload_only_contract.rs`
- `backend/server/src/services/note_service.rs`
- `backend/server/src/services/kanban_service.rs`
- `backend/server/src/handlers/files.rs`

Tests required:

- Reject `..`, absolute paths, path separators in filenames, `.rustshare*`, `index.editor.json`.
- Enforce write permission on upload and read permission on view/list.
- Public rendering/listing excludes hidden metadata and editor cache.
- Duplicate filenames are handled according to contract.

Acceptance criteria:

- Current backend behavior is pinned by tests before attachment fixes.
- Unsafe attachment names and metadata exposure are covered.

Codex pass size: Split further. Notes/public rendering first, then kanban.

### Step 12: Fix Attachment Backend Bugs

Why it matters:

This applies fixes found by Step 11 and should not be mixed with unrelated editor UI work.

Risk level: High.

Dependencies: Step 11.

Files likely affected:

- `backend/server/src/services/note_service.rs`
- `backend/server/src/services/kanban_service.rs`
- `backend/server/src/handlers/files.rs`
- Shared validation helpers if introduced.

Tests required:

- All Step 11 tests pass.
- Existing editor/frontend attachment tests remain valid.
- Public share/note tests pass.

Acceptance criteria:

- Backend enforces attachment path and metadata invariants.
- Module-specific attachment schemas remain backward-compatible unless a migration is explicitly planned.

Codex pass size: Split further by module/service.

### Step 13: Add Audit Taxonomy and Core Audit Tests

Why it matters:

Auditability is a hard contract, but changing audit behavior can touch many flows. Start with taxonomy and core files/shares before modules.

Risk level: Medium for taxonomy/tests; High for runtime audit emission fixes.

Dependencies: Steps 2, 7, and 8 preferred.

Files likely affected:

- `docs/security-model.md`
- `docs/adr/02-contract.md`
- `backend/tests/admin_audit_test.rs`
- `backend/tests/file_operations.rs`
- `backend/tests/contracts/share_link_contract.rs`
- `backend/server/src/handlers/admin/audit.rs`
- `backend/crates/storage/src/event_store.rs`

Tests required:

- Audit for login/security event, upload, download, replace, rename, move, delete, restore, share create, share revoke, public-link access.
- Denied sensitive public-link access logged if contract says so.

Acceptance criteria:

- Audit taxonomy is documented.
- Core file/share security-sensitive operations have durable audit evidence.

Codex pass size: Split further. Taxonomy/tests first, runtime emission fixes second.

### Step 14: Add Module Audit and Realtime Coverage

Why it matters:

Module services are uneven. After core permissions and registry behavior are stable, bring module audit/realtime behavior up to the same level.

Risk level: Medium/High.

Dependencies: Steps 6, 9, and 13.

Files likely affected:

- `backend/server/src/services/kanban_service.rs`
- `backend/server/src/services/note_service.rs`
- `backend/server/src/services/meeting_service.rs`
- `backend/server/src/services/standup_service.rs`
- `backend/server/src/services/decision_service.rs`
- `backend/server/src/services/brainstorming_service.rs`
- `backend/crates/core/src/events/types.rs`
- `frontend/src/lib/websocket/events.ts`
- `frontend/src/lib/websocket/manager.ts`

Tests required:

- Backend event emission tests for module create/update/delete where relevant.
- Frontend invalidation tests for kanban board/card/label/assignee/checklist/attachment changes.
- Audit tests for module create/update/delete.

Acceptance criteria:

- Module mutations emit durable audit where security-sensitive.
- Frontend receives or invalidates relevant module data after realtime events.

Codex pass size: Split further. Kanban realtime alone is one pass; module audit is separate.

### Step 15: Expand Search and AI Permission Coverage

Why it matters:

Search and AI must not become alternate access paths. This should follow permission and revocation fixes.

Risk level: High.

Dependencies: Steps 7, 8, 10, and 13.

Files likely affected:

- `backend/server/src/handlers/search.rs`
- `backend/crates/core/src/services/search_service.rs`
- `backend/crates/storage/src/repos/search/*`
- `backend/crates/core/src/services/ai_service.rs`
- `backend/tests/contracts/search_authorization_contract.rs`
- `backend/tests/contracts/ai_permission_contract.rs`
- `frontend/src/lib/layout/topbar/GlobalSearch.svelte`

Tests required:

- Search excludes other tenants, unauthorized shared content, deleted files, revoked shares, expired shares, hidden metadata, and module sidecars.
- AI disabled mode works.
- AI/search excludes revoked/deleted content according to documented propagation behavior.

Acceptance criteria:

- Search and AI use normal effective permissions.
- Stale/deleted/revoked content behavior is documented and tested.

Codex pass size: Split further. Search first, AI second.

### Step 16: Add Durable Activity and Notification Maturity

Why it matters:

This improves collaboration maturity but should wait until permissions, audit, and realtime behavior are trustworthy.

Risk level: Medium.

Dependencies: Steps 13, 14, and 15.

Files likely affected:

- `backend/crates/core/src/events/*`
- `backend/server/src/handlers/notifications.rs`
- New or existing activity projection handlers.
- `frontend/src/lib/stores/activity.ts`
- `frontend/src/lib/components/dashboard/RecentActivity.svelte`
- `frontend/src/lib/components/activity/ActivityFeed.svelte`

Tests required:

- Activity feed is server-sourced, permission-filtered, deterministic, and paginated.
- Unread notification count updates after realtime notifications.
- Revoked/deleted resources no longer produce openable activity links for unauthorized users.

Acceptance criteria:

- Recent activity no longer depends only on local browser state.
- Notifications and activity respect permission changes.

Codex pass size: Split further. Backend projection first, frontend consumption second.

### Step 17: UI Polish and Predictability Pass

Why it matters:

Visual and UX polish should happen after data, permission, registry, and realtime behavior are stable.

Risk level: Low/Medium.

Dependencies: Steps 9, 14, and 16.

Files likely affected:

- `frontend/src/routes/(app)/*`
- `frontend/src/lib/components/dashboard/*`
- `frontend/src/lib/components/modules/*`
- `frontend/src/lib/components/common/*`
- `frontend/src/lib/layout/*`

Tests required:

- Loading, empty, error, disabled, unauthorized, and offline states.
- Mobile/desktop layout tests where available.
- E2E smoke for dashboard, files, modules, shares, notifications.

Acceptance criteria:

- UI state is predictable for disabled modules, denied access, empty data, stale realtime, and failed network calls.
- No visual change weakens data/permission behavior.

Codex pass size: Split further by surface: dashboard, modules, files/shares, notifications.

## Risky Changes To Mark Explicitly In Future Prompts

- Any migration of legacy module roots or durable metadata.
- Removing or rewiring `UserShareService`.
- Changing share session token validation.
- Changing permission resolver cache behavior.
- Adding or changing public share rendering.
- Adding document-level editor APIs.
- Changing object storage keys or metadata verification logic.
- Making dashboard/sidebar authorization decisions client-side instead of server-side.
- Adding search/AI indexing over new content types.

## Grouped Workstreams

Backend:

- Steps 2, 3, 7, 8, 11, 12, 13, 14, 15, 16.

Frontend:

- Steps 6 frontend normalization, 9, 14 frontend realtime, 15 global search behavior, 16 activity/notifications, 17.

Storage/data integrity:

- Steps 5, 10, 11, 12, 13, 15.

Tests/contracts:

- Steps 1, 2, 3, 4, 5, 11, 13, 15.

Operations:

- Step 10, with later backup status, metrics, and audit export work after core readiness.

