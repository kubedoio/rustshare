# Mattermost-Level Maturity Benchmark

Mattermost is used here only as a maturity benchmark for stable collaboration behavior. RustShare does not need to become Mattermost. The relevant patterns are predictable permissions, stable workspace boundaries, durable history, realtime consistency, operational controls, and clear user mental models.

| Collaboration stability pattern | RustShare evidence | Maturity assessment | Relevant gap |
|---|---|---|---|
| Workspace/team-like organization | Tenant IDs, `/Workspace/...` module roots, groups, admin UI. | Partial | Workspace is mostly a tenant/path convention, not a clear user-facing/team governance model. |
| User roles | `is_admin`, disabled users, admin extractor. Product spec calls owner/admin/member/guest. | Partial | No complete role model beyond admin/member-like behavior. Guest/public share behavior is separate. |
| Permission model | `PermissionResolver` owner/direct/group/inherited shares; public sessions. | Good foundation, partial maturity | Needs exhaustive contract tests, cache invalidation guarantees, and unified share service. |
| Sharing boundaries | Public/internal/group shares; public sessions; upload-only support. | Partial | Dual sharing paths and incomplete governance enforcement create risk. |
| Realtime updates | EventBroadcaster, `/api/ws`, frontend invalidation, WebSocket tests. | Partial | Module-specific realtime uneven; no durable unread/activity model for all events. |
| Activity history | Event store, admin audit, share access log, local frontend activity store. | Partial | User-facing activity is not clearly durable, server-sourced, permission-filtered history. |
| Notifications/unread | Notifications API, unread count, frontend notification store/query invalidation. | Partial | Mostly share/permission notification scope; no broad workspace unread model. |
| Attachments | File-backed attachment specs, editor adapters, notes/kanban attachment types. | Partial | No unified backend attachment contract/API; public rendering exposure needs tests. |
| Search | Search handler/repo/API and global search UI. | Partial | Needs broader authorization, deletion/revocation, module artifact, and indexing SLA tests. |
| Pinned/saved/recent items | Starred files/folders, note pinned metadata, recent notes/dashboard artifacts. | Partial | Concepts are fragmented and not generalized across workspace artifacts. |
| Auditability | Admin audit, security events, share access logs, event store. | Partial | Contract requires every security-sensitive action; module/file audit coverage is incomplete. |
| Admin controls | Admin users/groups/OIDC/SMTP/webhooks/workflows/modules/templates/security pages. | Stronger area | Tenant sharing policy and operational readiness controls need enforcement/tests. |
| Predictable UX | Broad Svelte app with dashboard, files, modules, settings, admin. | Partial | Dashboard/sidebar registry drift and hardcoded actions reduce predictability. |
| Integration readiness | Webhooks, chat integration handlers/tests, OIDC/SCIM docs. | Partial | Integration boundaries exist, but event taxonomy and permission-checked unfurl/attachment behavior need more coverage. |
| Operational stability | Health route, tracing, scripts, backup/restore docs, verification tooling. | Partial/weak | Need readiness, metrics, backup status, audit export, queue/index lag instrumentation. |

## Relevant Patterns To Adopt

- **Single permission truth:** every UI, API, realtime event, search result, AI answer, and integration must use the same effective permission model.
- **Durable activity feed:** user-visible recent activity should come from server-side events/audit projections, not only local browser state.
- **Clear unread semantics:** notifications should distinguish read/unread, delivery target, resource, actor, and action URL consistently.
- **Predictable share lifecycle:** create, access, expiry, revocation, and policy-denied outcomes should be testable and visible.
- **Admin audit and export:** administrators should be able to review and export sensitive events without reading raw logs.
- **Stable registry-driven UX:** modules should appear, disappear, and route based on registry/permissions, not mixed static definitions.
- **Operational readiness surface:** health must include dependencies and lag, not only process aliveness.
- **Integration boundaries:** chat/webhooks/AI should receive permission-filtered references and never bypass RustShare authorization.

## Non-Goals From This Benchmark

- Do not copy Mattermost channels, messaging, or product shape.
- Do not add enterprise breadth before RustShare file/workspace contracts are stable.
- Do not create new collaboration concepts unless they support file/workspace stability.

