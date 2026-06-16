# ADR 0021: Tenant Isolation for Share Links and Removal of No-Op RLS Middleware

## Status

Accepted — implemented in Workstream B of the production-readiness gap closure.

## Context

The production-readiness audit found two related issues in the multi-tenant isolation workstream:

1. **Cross-tenant share links were not rejected.** `get_share_by_token` looked up a share only by `share_token`, with no `tenant_id` filter. Both `validate_and_create_session` and `get_public_share_info` called it unconditionally, so a tenant-B request could resolve a tenant-A share.
2. **The RLS context middleware was a no-op.** It acquired a connection, ran `SET app.current_tenant_id` / `SET app.current_user_id`, returned the connection to the pool, and then ran the inner handler. Handler queries execute on different connections, so the set context was never visible.

## Decision

### Share links

- Added `tenant_id` to `get_share_by_token` in all `MetadataStoreOps` / `ShareMetadataStoreOps` implementations and updated the SQL to `WHERE share_token = $1 AND tenant_id = $2`.
- Added `tenant_id` parameters to `ShareService::validate_and_create_session`, `get_public_share_info`, and `list_public_folder_contents`.
- For unauthenticated public-share HTTP routes, the caller must supply the tenant in the `X-Tenant-ID` request header. Requests for the wrong tenant return `ShareNotFoundByToken`.
- For share-session routes (which use a JWT issued by `validate_and_create_session`), `tenant_id` was added to `ShareSessionClaims` so the session carries the tenant context and each route can scope the share lookup.
- The chat-integration unfurl endpoints were updated analogously: the authenticated endpoint uses the authenticated user's tenant, and the public endpoint requires `X-Tenant-ID`.

### RLS middleware

- Removed the no-op `tenant_context` middleware, its module, its layer in `main.rs`, and the empty integration test.
- Documented that repository-level tenant filtering is the active and primary tenant boundary.

### Login tenant scoping

- Password login still uses `find_user_by_email` without a tenant filter. Adding tenant to the login request was deemed too invasive for this workstream. The residual risk is documented with TODO comments in the login handler and repository, and this ADR records the decision.

## Consequences

- Cross-tenant share link access is now rejected at the repository and service layers.
- Public share links require an `X-Tenant-ID` header. This is a breaking change for anonymous public-share clients; future work may encode tenant in the token or route path to make links self-contained again.
- No ineffective security control remains in production.
- Email uniqueness across tenants remains an implicit assumption for password login until the residual risk is addressed.
