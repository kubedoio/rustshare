# Documents Inventory

This inventory lists source-of-truth, adjacent, and relevant docs found during the audit. Status values are document-currency judgments, not implementation status.

## Primary Source-of-Truth Documents

| Document | Defines | Status |
|---|---|---|
| `docs/adr/02-contract.md` | Behavioral contracts for tenancy, permissions, files, sharing, sync, search, AI, chat, security, storage, operations, and required executable suites. | Current; highest-priority enforcement layer. |
| `docs/adr/01-product-spec.md` | Product thesis, Phase 1 scope, hard requirements, priority order, and success criteria. | Current direction; some scope exceeds implementation maturity. |
| `docs/adr/03-design.md` | Architecture/design guidance for storage, namespace, sharing, sync, chat, AI, security, deployment, and migration. | Current architectural intent; overlaps ADR-0001. |
| `docs/adr/0001-ADR.md` | Accepted hybrid object-store-centered service architecture, explicit metadata/permission authority, tenant boundary, bounded RustChat integration, optional AI. | Current and accepted. |
| `docs/STATUS.md` | Honest maturity label, zero-PostgreSQL/metadata-v2 notes, implemented/partial/hardening status, priorities. | Current but conflicts with active SQL-heavy code paths and older TODO maturity wording. |
| `docs/PRODUCTION_READINESS.md` | Runtime, auth/session, sharing, storage, security, recovery, observability, deployment checklists and remaining risks. | Current readiness checklist; many items partial. |

## Module, Editor, and Attachment Specs/Contracts

| Document | Defines | Status |
|---|---|---|
| `docs/specs/template-modules-system.md` | Module/template registry concepts, predefined modules, registry persistence, startup behavior, create-from-template flow, audit events. | Partly outdated: root paths and shape differ from code. |
| `docs/contracts/template-module-contract.md` | Required module/template fields, key/path safety rules, file-backed object invariants. | Partly outdated: API/domain naming differs; root paths differ. |
| `docs/contracts/module-ui-contract.md` | Sidebar/dashboard/page UI fields, primary actions, summary modes, layouts, approved icon registry, rendering invariants. | Partly outdated: frontend/backend use nested `widget` and extra icons/types. |
| `docs/specs/module-renderers-and-file-layouts.md` | Renderer resolution and layouts for notes, meetings, standups, kanban, decisions, shares, generic view. | Current direction but requires validation against implementation. |
| `docs/specs/webui-dashboard-sidebar-integration.md` | Sidebar and dashboard rendering rules, responsive behavior, failure states. | Partly current; dashboard still has hardcoded pieces. |
| `docs/specs/module-integration.md` | Shared editor rule and module-level use of rich markdown editor across notes, meetings, decisions, standups, kanban, brainstorming, file browser. | Current direction; implementation uneven. |
| `docs/specs/rich-markdown-editor-system.md` | Shared rich markdown editor architecture and behavior. | Current direction; mostly frontend-side. |
| `docs/contracts/rich-document-contract.md` | Rich markdown metadata sidecar shape and invariants. | Current, but backend schemas vary by module. |
| `docs/contracts/editor-api-contract.md` | Optional document-level editor APIs for get/save/upload/list/delete attachments and error taxonomy. | Unclear/partial: no `/api/editor/documents` routes found. |
| `docs/contracts/editor-renderer-contract.md` | Editor renderer rules. | Relevant, limited detail in heading inventory. |
| `docs/specs/attachments-and-assets.md` | Attachment folder layout, upload rules, sanitization, insert syntax, public rendering. | Current concept; backend implementation is fragmented. |
| `docs/contracts/attachment-contract.md` | Attachment metadata fields and invariants. | Current concept; code uses multiple attachment shapes. |
| `docs/specs/security-and-permissions.md` | Permission rules, sanitization, attachment/link/extension security. | Current but terse; needs executable coverage. |
| `docs/specs/markdown-storage-and-serialization.md` | Markdown storage and serialization expectations. | Relevant; not deeply reviewed. |
| `docs/specs/printable-pdf-export.md` | PDF/printable export behavior. | Relevant for editor maturity, lower priority to core stability. |
| `docs/specs/docmost-like-editor-ux.md` | Rich editor UX direction. | Product/UX guidance, not a backend contract. |

## Older Phase Specs and Plans

| Document | Defines | Status |
|---|---|---|
| `docs/superpowers/specs/2026-03-17-rustshare-design.md` | Original broad design: data model, realtime, file versioning, sharing, previews, auth, frontend, errors, testing, deployment. | Historical; useful but duplicated by ADRs/contracts. |
| `docs/superpowers/specs/2026-03-17-rustshare-phase2-file-operations.md` | File/folder service APIs, events, errors, tests. | Historical but implementation-relevant. |
| `docs/superpowers/specs/2026-03-18-rustshare-phase3a-realtime-sync.md` | EventBroadcaster, WebSocket, catch-up, event store integration. | Historical/current-adjacent; compare with WebSocket tests. |
| `docs/superpowers/specs/2026-03-18-rustshare-phase3a-user-sharing.md` | User sharing, notifications, WebSocket events, migrations, security. | Historical; unified sharing work supersedes parts. |
| `docs/superpowers/specs/2026-03-18-rustshare-phase3b-sharing.md` | Public share links, sessions, access logging, upload-only, rate limiting, tests. | Historical but important for public share contract. |
| `docs/superpowers/specs/2026-03-18-rustshare-phase4-frontend-design.md` | Frontend architecture, API reference, WebSocket layer, UX, testing. | Historical; some API aliases removed. |
| `docs/superpowers/specs/2026-03-22-admin-panel-design.md` | Admin users/groups/OIDC/SMTP/webhooks/audit frontend/backend/API/tests. | Mostly current. |
| `docs/superpowers/specs/2026-04-02-production-workflow-system-design.md` | Workflows, invites, email service, admin/invite routes, tests. | Mostly current for invite/workflow surface. |
| `docs/superpowers/specs/2026-04-13-file-listing-sort-pagination-design.md` | File listing sort/pagination design. | Current-adjacent; verify UI/backend behavior separately. |
| `docs/plans/*.md` and `docs/superpowers/plans/*.md` | Implementation plans for sync, sharing fixes, desktop auth, directory upload, template modules, tenant isolation, UI audit. | Planning/history; not source of truth unless echoed in contracts/ADRs. |

## Architecture, Operations, Security, and Testing Docs

| Document | Defines | Status |
|---|---|---|
| `docs/architecture.md` | System overview, backend/frontend architecture, data flows, schema, object layout, request lifecycle, future metadata migration. | Useful current overview; may lag code. |
| `docs/security-model.md` | Threat model, auth, authorization, encryption, secrets, network, uploads, rate limiting, audit logging. | Current; needs test mapping. |
| `docs/backup-restore.md` | Backup and restore processes. | Relevant; needs contract evidence against restore tests/tooling. |
| `docs/configuration.md` | Configuration behavior. | Relevant operations doc. |
| `docs/TESTING.md`, `backend/TESTING.md`, `frontend/src/TEST_README.md` | Testing instructions and conventions. | Current process docs. |
| `backend/docs/SYNC_SEMANTICS.md` | Cursor format, delta API, deltas, conflict strategy, retry/idempotency, client journal. | Current sync contract-adjacent. |
| `backend/docs/SCIM_INTEGRATION.md` | SCIM operations, auth, user/group provisioning, limitations, security. | Current provisioning reference. |
| `frontend/WEBSOCKET_IMPLEMENTATION.md`, `frontend/WEBSOCKET_ARCHITECTURE_DIAGRAM.md`, `frontend/WEBSOCKET_TESTING_CHECKLIST.md`, `frontend/src/lib/websocket/EVENT_REFERENCE.md` | Frontend realtime implementation, event payloads, testing checklist. | Current-adjacent; may drift from backend event payloads. |

## Root and Project Docs

| Document | Defines | Status |
|---|---|---|
| `README.md` | Project status, quick start, product direction, target users, architecture, docs map, priority. | Current onboarding/status summary. |
| `ROADMAP.md` | Product roadmap. | Relevant planning. |
| `DESIGN.md`, `docs/DESIGN.md` | Visual/product design guidance. | Duplicated; clarify precedence. |
| `OPEN_SOURCE_READINESS_AUDIT.md` | Open source readiness gaps and maturity warnings. | Current audit context; explicitly flags TODO/status drift. |
| `docs/TODOS.md` | Agentic task list. | Potentially misleading: marks many TODOs complete while status docs remain more cautious. |
| `docs/TEMPLATE_MODULES_AUDIT_REPORT.md` | Prior module audit. | Relevant prior analysis; should be cross-reconciled. |
| `docs/qa/public-preview-final-qa-report.md` | Public preview QA findings. | Relevant quality evidence. |
| `rustshare_public_preview_handover/*.md` | Public preview implementation/checklist/UI simplification handover. | Historical handover. |

## Desktop Docs

| Document | Defines | Status |
|---|---|---|
| `apps/desktop/docs/specs/desktop-phase1-spec.md` | Desktop phase 1 behavior. | Current for desktop prototype, not web core. |
| `apps/desktop/docs/contracts/desktop-phase1-contracts.md` | Desktop contract behaviors. | Current for desktop scope. |
| `apps/desktop/docs/adr/*.md` | Desktop sync, state, conflict, pairing, delete/tombstone ADRs. | Current desktop decisions. |
| `apps/desktop/docs/testing/*.md` | Desktop test plan/matrix. | Current desktop testing reference. |
| `apps/desktop/docs/architecture/*.md` | Desktop architecture and flows. | Current desktop reference. |

## Complete Relevant Markdown File List Found

Root/project:

- `README.md`: primary project overview and documentation map; current.
- `backend/README.md`: backend setup/overview; current-adjacent.
- `frontend/README.md`: frontend setup/overview; current-adjacent.
- `docs-site/README.md`: docs site instructions; relevant to docs publishing.
- `CLAUDE.md`: agent/project guidance; relevant but not product contract.
- `CHANGELOG.md`: release/change history; current historical record.
- `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `GOVERNANCE.md`, `SECURITY.md`, `SUPPORT.md`: open-source/project governance docs; current process docs.
- `DESIGN.md`, `ROADMAP.md`, `OPEN_SOURCE_READINESS_AUDIT.md`: design, roadmap, and prior audit context.

Core docs:

- `docs/SPEC.md`: old notes MVP spec; outdated by later module/editor ADRs.
- `docs/STATUS.md`, `docs/PRODUCTION_READINESS.md`, `docs/architecture.md`, `docs/ARCHITECTURE_NOTES.md`, `docs/DESIGN.md`: current/high-level status and architecture docs with some duplication.
- `docs/security-model.md`, `docs/configuration.md`, `docs/backup-restore.md`, `docs/DEPLOYMENT.md`, `docs/troubleshooting.md`, `docs/TESTING.md`, `docs/development.md`, `docs/getting-started.md`, `docs/upgrading.md`, `docs/release-process.md`, `docs/DEPENDENCY_MANAGEMENT.md`: operational/security/testing/reference docs.
- `docs/FRONTEND_STATUS.md`, `docs/TODOS.md`, `docs/TEMPLATE_MODULES_AUDIT_REPORT.md`: status/audit/task docs; useful but not source-of-truth over ADR contracts.
- `docs/qa/public-preview-final-qa-report.md`: QA evidence.

Specs:

- `docs/specs/admin-modules-and-templates.md`
- `docs/specs/attachments-and-assets.md`
- `docs/specs/docmost-like-editor-ux.md`
- `docs/specs/markdown-storage-and-serialization.md`
- `docs/specs/module-integration.md`
- `docs/specs/module-renderers-and-file-layouts.md`
- `docs/specs/printable-pdf-export.md`
- `docs/specs/rich-markdown-editor-system.md`
- `docs/specs/security-and-permissions.md`
- `docs/specs/template-modules-system.md`
- `docs/specs/webui-dashboard-sidebar-integration.md`

Contracts:

- `docs/contracts/attachment-contract.md`
- `docs/contracts/editor-api-contract.md`
- `docs/contracts/editor-renderer-contract.md`
- `docs/contracts/module-ui-contract.md`
- `docs/contracts/rich-document-contract.md`
- `docs/contracts/template-module-contract.md`
- `apps/desktop/docs/contracts/desktop-phase1-contracts.md`

ADRs:

- `docs/adr/0001-ADR.md`
- `docs/adr/01-product-spec.md`
- `docs/adr/02-contract.md`
- `docs/adr/03-design.md`
- `docs/adr/0016-file-backed-template-modules.md`
- `docs/adr/0017-template-registry-and-admin-governance.md`
- `docs/adr/0018-webui-module-navigation-and-dashboard-integration.md`
- `docs/adr/0019-shared-rich-markdown-editor.md`
- `docs/adr/0020-canonical-markdown-with-editor-cache.md`
- `docs/adr/0021-file-backed-attachments-and-portability.md`
- `docs/adr/0022-pdf-export-printable-rendering.md`
- `apps/desktop/docs/adr/001-desktop-monorepo-boundary.md`
- `apps/desktop/docs/adr/002-shared-rust-sync-core.md`
- `apps/desktop/docs/adr/003-phase1-no-native-filesystem-provider.md`
- `apps/desktop/docs/adr/004-local-state-store-choice.md`
- `apps/desktop/docs/adr/005-conflict-handling-policy.md`
- `apps/desktop/docs/adr/006-device-registration-and-sync-session.md`
- `apps/desktop/docs/adr/007-sync-root-scoped-directory-mirroring.md`
- `apps/desktop/docs/adr/008-delete-tombstones-and-idempotent-deletes.md`

Plans and historical implementation specs:

- `docs/plans/PUBLIC_PREVIEW_UI_AUDIT.md`
- `docs/plans/TENANT_ISOLATION_PLAN.md`
- `docs/plans/2026-04-03-directory-upload-design.md`
- `docs/plans/2026-04-03-directory-upload-plan.md`
- `docs/plans/2026-04-04-fix-compat-layer-inconsistencies.md`
- `docs/plans/2026-04-04-fix-group-share-inconsistencies.md`
- `docs/plans/2026-04-04-fix-shared-section-display.md`
- `docs/plans/2026-04-04-unified-share-service-design.md`
- `docs/plans/2026-04-04-unified-share-service-implementation.md`
- `docs/plans/2026-04-06-extended-file-formats-design.md`
- `docs/plans/2026-04-06-extended-file-formats-plan.md`
- `docs/plans/2026-04-06-new-button-enhancement-design.md`
- `docs/plans/2026-04-06-new-button-enhancement.md`
- `docs/plans/2026-04-07-desktop-device-pairing-default-auth-design.md`
- `docs/plans/2026-04-07-desktop-device-pairing-default-auth.md`
- `docs/plans/2026-04-08-desktop-cli-improvements-design.md`
- `docs/plans/2026-04-08-desktop-cli-improvements.md`
- `docs/plans/2026-04-08-sync-engine-design.md`
- `docs/plans/2026-04-08-sync-engine-implementation.md`
- `docs/plans/2026-04-09-delete-tombstones-implementation.md`
- `docs/plans/2026-04-30-template-modules-phase2.md`
- `docs/superpowers/specs/2026-03-17-rustshare-design.md`
- `docs/superpowers/specs/2026-03-17-rustshare-phase2-file-operations.md`
- `docs/superpowers/specs/2026-03-18-rustshare-phase3a-realtime-sync.md`
- `docs/superpowers/specs/2026-03-18-rustshare-phase3a-user-sharing.md`
- `docs/superpowers/specs/2026-03-18-rustshare-phase3b-sharing.md`
- `docs/superpowers/specs/2026-03-18-rustshare-phase4-frontend-design.md`
- `docs/superpowers/specs/2026-03-22-admin-panel-design.md`
- `docs/superpowers/specs/2026-04-02-production-workflow-system-design.md`
- `docs/superpowers/specs/2026-04-13-file-listing-sort-pagination-design.md`
- `docs/superpowers/plans/*.md`: historical execution plans.

Backend/frontend/desktop support docs:

- `backend/TESTING.md`
- `backend/docs/SCIM_INTEGRATION.md`
- `backend/docs/SYNC_SEMANTICS.md`
- `frontend/DEPLOYMENT.md`
- `frontend/WEBSOCKET_ARCHITECTURE_DIAGRAM.md`
- `frontend/WEBSOCKET_IMPLEMENTATION.md`
- `frontend/WEBSOCKET_QUICK_START.md`
- `frontend/WEBSOCKET_TESTING_CHECKLIST.md`
- `frontend/src/TEST_README.md`
- `frontend/src/lib/websocket/README.md`
- `frontend/src/lib/websocket/EVENT_REFERENCE.md`
- `apps/desktop/CHANGELOG.md`
- `apps/desktop/docs/CLI_USAGE.md`
- `apps/desktop/docs/specs/desktop-phase1-spec.md`
- `apps/desktop/docs/architecture/desktop-phase1-architecture.md`
- `apps/desktop/docs/architecture/desktop-phase1-runtime-view.md`
- `apps/desktop/docs/architecture/desktop-phase1-sequence-flows.md`
- `apps/desktop/docs/testing/desktop-phase1-test-matrix.md`
- `apps/desktop/docs/testing/desktop-phase1-test-plan.md`
- `apps/desktop/docs/distribution/build-and-package.md`
- `apps/desktop/docs/distribution/macos-client-installation.md`

Public preview handover:

- `rustshare_public_preview_handover/rustshare-public-preview-implementation-checklist.md`
- `rustshare_public_preview_handover/rustshare-public-preview-ui-simplification-report.md`
- `rustshare_public_preview_handover/source-review-resource.md`
