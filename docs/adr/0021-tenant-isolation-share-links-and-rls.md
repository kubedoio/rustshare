# ADR 0021: Tenant Isolation for Share Links and Removal of No-Op RLS Middleware

## Status

Accepted — implemented in Workstream B of the production-readiness gap closure and updated by follow-up remediation tasks on 2026-06-18.

## Context

The production-readiness audit found two related issues in the multi-tenant isolation workstream:

1. **Cross-tenant share links were not rejected.** `get_share_by_token` looked up a share only by `share_token`, with no `tenant_id` filter. Both `validate_and_create_session` and `get_public_share_info` called it unconditionally, so a tenant-B request could resolve a tenant-A share.
2. **The RLS context middleware was a no-op.** It acquired a connection, ran `SET app.current_tenant_id` / `SET app.current_user_id`, returned the connection to the pool, and then ran the inner handler. Handler queries execute on different connections, so the set context was never visible.

## Decision

### Share links

- Added `tenant_id` to `get_share_by_token` in all `MetadataStoreOps` / `ShareMetadataStoreOps` implementations and updated the SQL to `WHERE share_token = $1 AND tenant_id = $2`.
- Added `tenant_id` parameters to `ShareService::validate_and_create_session`, `get_public_share_info`, and `list_public_folder_contents`.
- For unauthenticated public-share HTTP routes, `X-Tenant-ID` is optional for backward compatibility. If supplied, it must match the share's tenant; if omitted, the backend derives the tenant from the globally unique share token before continuing with tenant-scoped lookups.
- For share-session routes (which use a JWT issued by `validate_and_create_session`), `tenant_id` was added to `ShareSessionClaims` so the session carries the tenant context and each route can scope the share lookup.
- The chat-integration unfurl endpoints were updated analogously: the authenticated endpoint uses the authenticated user's tenant, and the public endpoint requires `X-Tenant-ID`.

### RLS middleware

- Removed the no-op `tenant_context` middleware, its module, its layer in `main.rs`, and the empty integration test.
- Documented that repository-level tenant filtering is the active and primary tenant boundary.

### Login tenant scoping

- Password login accepts an optional `tenant_id`. When provided, credential validation uses a tenant-scoped, case-insensitive email lookup. When omitted for backward compatibility, unscoped login rejects ambiguous emails that exist in more than one tenant.
- The users table enforces per-tenant, case-insensitive email uniqueness.

## Consequences

- Cross-tenant share link access is now rejected at the repository and service layers.
- Public share links remain self-contained for existing clients: omitting `X-Tenant-ID` derives tenant from the globally unique share token, while providing the header adds an explicit tenant consistency check.
- No ineffective security control remains in production.
- Password login no longer relies on global email uniqueness when clients provide `tenant_id`, and ambiguous unscoped login is rejected.
