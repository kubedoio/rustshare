# Test Coverage Gaps

## Existing Test Coverage

Backend coverage includes:

- Admin users, groups, audit, config, webhooks, workflows.
- Notes, decisions, kanban, brainstorming.
- Group sharing and compatibility layer integration.
- Invites, file operations, conflict detection, folder cascade, version restore.
- Contract tests for tenant isolation, share links, public upload-only, versioning, restore, storage verification, search authorization, chat integration, AI permission, device pairing.
- WebSocket sync, multi-device, catch-up tests.
- Core duplicate file/upload service tests.

Frontend coverage includes:

- API module unit tests for shares, notifications, modules, brainstorming.
- Editor metadata/path/validation/markdown/security/attachments tests.
- Module registry, paths, workspace surface, page routing, icon registry tests.
- Dashboard widgets, recent activity, module cards, layout/topbar/left rail tests.
- Admin component tests.
- Login and device route tests.
- Admin Playwright E2E.

## Missing Contract Tests

- Contract-level module registry schema serialization from backend to frontend.
- Disabled module hidden from sidebar, dashboard, routes, and create actions.
- Tenant isolation for all module object routes.
- Permission monotonicity for notes, meetings, standups, decisions, kanban, brainstorming, attachments, search, AI, public shares.
- Permission-visible module dashboard summaries, including directly shared child objects when the module root is not shared.
- Audit event emission for every security-sensitive file/share/module/admin operation.
- Revocation correctness for active public share sessions and group membership changes.
- Share governance enforcement for tenant/workspace public/external share policy.
- Metadata/blob drift detection for module sidecars, attachments, previews, and thumbnails.
- Health/readiness contract for auth/session, storage, metadata, indexing, event delivery.
- Audit export contract.

## Missing Backend Tests

- Backend editor/document API decision coverage: either route tests or explicit file-API contract tests.
- Attachment backend tests: filename sanitization, traversal rejection, hidden metadata exclusion, duplicate suffixing, write/read permissions, and no side effects on denied delete.
- Module services: create/update/delete audit events and WebSocket events.
- Kanban realtime event tests for board/card/label/assignee/checklist/attachment changes.
- Notes public share policy/expiry/revocation/audit tests.
- Search deleted/revoked/expired/module artifact tests.
- Upload-only public folder upload retry/idempotency tests.
- Device revocation blocking sync/WebSocket/device token access.
- Disabled user cannot use browser session, JWT, device token, WebSocket, invite, share creation.
- Template path safety tests for every `default_files` and `folder_structure` input.
- Legacy-to-`/Workspace` module path compatibility tests.
- Metrics/readiness/backup status endpoint tests once implemented.

## Missing Frontend Tests

- Dashboard consumes backend registry and hides disabled/unauthorized modules.
- Quick actions follow registry primary actions instead of static availability.
- Unknown widget/layout/renderer fallback behavior from real API payloads.
- Module route denial/disabled states.
- Public share UI for upload-only boundaries.
- Attachment panel integration with backend failures and path validation.
- WebSocket frontend invalidation for kanban events.
- Stale notification/unread count after realtime events.
- Global search result authorization and empty/error states.
- Admin security policy changes hiding/disabling sharing controls.

## Missing Integration Tests

- End-to-end share lifecycle: create internal/group/public share, access through UI/API, revoke, verify denial, verify notification/audit.
- End-to-end module lifecycle: admin disables module, user cannot see/use/create, data remains accessible via files if authorized.
- End-to-end rich document: create note from template, add attachment, save markdown, public share render, revoke share.
- End-to-end backup/restore drill with module artifacts and attachments.
- End-to-end multi-device realtime: file/module edit on one browser updates another.
- End-to-end OIDC/SCIM lifecycle: provision user/group, share to group, deprovision, access denied.

## Missing Regression Tests

- Old `/Notes` content remains discoverable while new writes go to `/Workspace/Notes`.
- Module UI payload using legacy `modulePage` still normalizes to `page`.
- Kanban attachment delete for a non-attachment file does not delete the file and does not create an `attachments` folder.
- Kanban dashboard summary includes a directly shared board even when `/Workspace/Kanban` is not shared.
- API route compatibility for remaining `/api/users/me` legacy aliases is intentional and bounded.
- Public link token/session not logged or exposed in frontend analytics/errors.
- Deleted or hidden metadata files do not appear in public share or file browser listings.
- Permission resolver cache does not keep stale grants after share revocation or group membership change.

## Recommended Test Order

1. Tenant/permission contract tests for module routes and attachments.
2. Share lifecycle and revocation tests across public, internal, group, and active sessions.
3. Module registry schema and disabled-module visibility tests.
4. Audit emission tests for file/share/module operations.
5. Attachment security tests.
6. Search authorization/deleted/revoked/module artifact tests.
7. Realtime module event tests, especially kanban.
8. Operational readiness/backup visibility tests.
9. Frontend E2E flows for registry-driven dashboard/sidebar and share lifecycle.
