# Phase 4.0 Audit — Tenant Identifiers, User Identity, Session Revocation, Disabled Principals

Baseline: commit `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e` (main, PR #359 merged).
Scope: actual semantics from source and migrations, not intended architecture.
Method: read-only source audit in an isolated worktree; prior-verified facts from the issue #287 evidence matrix treated as given and re-confirmed where cited.
Status: this is the Phase 1 source-derived snapshot. Findings were subsequently verified against executed disposable-PostgreSQL and live-server evidence in `docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` (cases a-j) and `docs/audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md` (rows `P4-*`). One finding, F4, was **withdrawn** by that evidence; corrections are marked inline.

## Actual semantics (baseline)

**Tenant identifiers.** `tenant_id UUID NOT NULL DEFAULT '00000000-…'` on users, folders, files, shares, file_versions, notifications, user_groups, file_thumbnails (`backend/migrations/20260329170001_add_tenant_id_to_tables.sql:4-33`); `user_sessions` gained it in `20260329200001`; `invite_tokens` has it with nil default (`20260402000002_create_invite_tokens.sql:3`). Request tenant context is **not** taken from headers or JWT claims on authenticated routes: `resolve_bearer_token` reads `tenant_id` from the `users` row (`handlers/extractors.rs:65-78`), cookie sessions read it from the `user_sessions` row (`web_session.rs:24-70`, used at `extractors.rs:152-164`). The JWT `tenant_id` claim (`crates/auth/src/jwt.rs:24`) is **ignored by HTTP extractors**; only WebSocket auth uses the claim (`handlers/ws_auth.rs:46`). `X-Tenant-ID` exists only on the public share and public chat-unfurl routes (`handlers/public_shares.rs:37-57`; `handlers/chat_integration.rs:26`, read at `:164`). `RUSTSHARE_DEFAULT_TENANT_ID` defaults to **nil UUID** (`bootstrap.rs:715-718`), so nil is not an edge case — it is the default home tenant for seed, OIDC, SCIM-v2, and admin-created users. Nil-as-wildcard/default appears in: RLS fallback (`20260507220652_fix_rls_to_allow_nil_default.sql:11-35`, `bootstrap.rs:124-138` sets nil `app.current_user_id`/`current_tenant_id` on every checkout), public-share JWT claims (`public_shares.rs:273-285`), and the optional tenant header (`public_shares.rs:43-57`). Real isolation is application-level `owner_id`+`tenant_id` predicates (e.g. `handlers/files.rs:1040-1041,1209,1251-1252`).

**User identity.** `users`: global-unique `username`, per-tenant case-insensitive unique `LOWER(email), tenant_id` (global constraint dropped in `20260617160001_add_users_email_lower_index.sql:10-14`), `is_admin`, `disabled_at` (`20260322000001`), globally-unique `external_id` (`20260328000001`). Identity source of truth is the local table (Argon2 hash; OIDC users get a random hash). OIDC binds by `(email, default_tenant_id)` only (`oidc.rs:591-623`) — OIDC users always land in the default tenant; `username` global uniqueness forces cross-tenant-unique usernames (`oidc.rs:634-661`). Invites: any authenticated user invites; invitee is inserted into the **inviter's** tenant (`invites.rs:69-72,288-298`).

**Sessions/tokens.** Three credential forms: stateless JWT (HS256, iss/aud/exp, 24 h default `config.rs:155-157`, no session linkage, no revocation list); server-side `user_sessions` (SHA-256-hashed, 7-day TTL); `device_tokens` (SHA-256 hash, `revoked_at`). Every authenticated path checks `disabled_at` per request: JWT and device tokens at `extractors.rs:65-76` and `:111-122`; web sessions at `web_session.rs:47-61` (lazily deleted); admin re-check at `extractors.rs:286-303`; WebSocket at `ws_auth.rs:69-84,104,115,133`; password login `auth.rs:166-168`; OIDC `oidc.rs:597-599`. Admin disable revokes everything (`admin/users.rs:626-649`). There is a dedicated test proving out-of-band disable kills live credentials: `backend/tests/user_disable_credential_revocation_test.rs`.

**Admin model.** `is_admin` is a single installation-wide flag (`extractors.rs:277-308`); there is no org/tenant-scoped role. All `/api/v1/admin/*` handlers take `AdminUser` (`handlers/admin/mod.rs:1`), but none scope queries to a tenant.

## Findings

**F1 — SCIM bearer token is a cross-tenant management bypass** — *Confirmed exploit / access bypass; DIRECT API-reachable.*
- `handlers/scim_v2.rs:141-142` `list_users`: `FROM users WHERE 1=1` — no tenant filter; `:209` get-by-id, `:361-398` PATCH active / DELETE: `UPDATE users … WHERE id = $1` with no tenant predicate.
- `handlers/scim.rs:321-341,431-456` v1: lookup/disable by global `external_id`, no tenant scope.
- `crates/core/src/services/scim_service.rs:56-57,236,283` v1 `ScimUser` accepts client-supplied `tenantId` and creates the user in **any** tenant.
- Actual behavior: possession of `RUSTSHARE_SCIM_BEARER_TOKEN` allows listing all users' PII across tenants, reading any user by ID, disabling any user by ID (cross-tenant DoS), and v1-creating users in arbitrary tenants.
- ADR impact: violates ADR-0038 decision 1 ("tenant is the institutional security boundary") on the SCIM surface today; `0031-tenant-isolation-share-links-and-rls.md`'s repository-level boundary is not applied to SCIM repositories.

**F2 — Admin user management is installation-wide, and admin-created users land in the nil tenant** — *Defense-in-depth weakness (by current single-tenant design), but a hard boundary violation for ADR-0038; DIRECT API-reachable with any admin JWT.*
- `admin/users.rs:175,181` list counts/selects across all tenants; get/update/delete by bare user id (`:430,489,738`); create omits `tenant_id` from the INSERT (`:371-374`) → nil-tenant user; uniqueness checks are global (`:329-350`), blocking emails that are legal per the per-tenant DB index.
- ADR impact: ADR-0038 decision 3 anticipates installation admin ≠ org admin; the actual code has only the installation-wide role and no tenant scoping to build on. Creating users without a tenant contradicts the "no inferred/ambiguous tenant" constraint if nil is meant as a legacy marker.

**F3 — Nil-tenant groups and cross-tenant group membership via admin API** — *Correctness defect; DIRECT API-reachable (admin); consistent with the pre-verified #287 facts.*
- `admin/groups.rs:181-187` INSERT omits `tenant_id` → nil-tenant group; `:445-460` `add_member` verifies the user exists but never compares `users.tenant_id` to the group's; group names are globally unique (`20260322000002_create_user_groups.sql:3`).
- ADR impact: exactly the legacy-group inventory ADR-0038 decision 4 refuses to auto-map; the API keeps producing new ambiguous (nil) groups.

**F4 — WITHDRAWN after executed evidence: disable already revokes credentials for every writer** — *Intended and tested behavior.*
- Originally reported as: `scim.rs:431-456` (v1 `set_user_disabled`) and `scim_v2.rs:360-398` (v2 PATCH/DELETE) only `UPDATE users.disabled_at`, unlike `admin/users.rs:637-649`, so a re-enable resurrects previously issued device tokens.
- The handler-level observation is accurate; the effect claim is refuted. `backend/migrations/20261003090000_revoke_credentials_on_user_disable.sql` installs an `AFTER UPDATE OF disabled_at` trigger (`users_disable_revokes_credentials`) that deletes `user_sessions` rows and sets `device_tokens.revoked_at` for **every** writer, SCIM included. Executed end-to-end against a disposable database: after the exact SCIM-shaped `UPDATE users SET disabled_at = NOW()`, web sessions = 0 and live device tokens = 0; a later re-enable does not resurrect them (see `docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` case (h) and evidence-matrix row `P4-10`).
- Residual, genuinely open weakness (distinct from F4): a stateless JWT issued before disable becomes usable again after re-enable until its `exp` (default 24 h, `config.rs:155-157`), because `resolve_bearer_token` (`extractors.rs:60-79`) has no token-version check. Classification: Defense-in-depth weakness — the same mechanism as F5.
- ADR impact: the identity-lifecycle decision (ADR-0040 D8) must be re-anchored on the trigger plus this residual JWT-revival gap, not on F4.

**F5 — Admin password reset leaves stateless JWTs valid up to expiry** — *Defense-in-depth weakness; DIRECT API-reachable scenario.*
- `admin/users.rs:540-554` deletes web sessions and revokes device tokens on password change, but there is no token-version check in `resolve_bearer_token` (`extractors.rs:60-79`); JWTs live until `exp` (default 24 h, `config.rs:155-157`). Logout similarly only deletes the web session (`auth.rs:284-327`).
- ADR impact: matters for ADR-0038's session-claims/revocation review item — revocation is partial by design.

**F6 — Disabled principals are blocked on every authenticated path** — *Intended and tested behavior.*
- Per-request `disabled_at` checks at `extractors.rs:65-76,111-122`, `web_session.rs:47-61`, `extractors.rs:286-303` (admin), `ws_auth.rs:69-133`, plus login/OIDC refusal (`auth.rs:166-168`, `oidc.rs:597-599`). Covered by `backend/tests/user_disable_credential_revocation_test.rs` and `backend/tests/chat_app_read_test.rs`. A disabled user cannot use any existing token (JWT, cookie, device, WS).

**F7 — Invite acceptance duplicates identities across tenants for case variants; exact-match check is globally blocking** — *Correctness defect; DIRECT API-reachable.*
- `invites.rs:269-272` pre-check is `WHERE email = $1` (case-sensitive, no tenant) → an exact email match in *another* tenant 409s even though the DB index (`LOWER(email), tenant_id`) allows it; a case-variant (`Foo@x` vs `foo@x`) in another tenant passes the pre-check and the index → two case-insensitively identical identities in different tenants, which then makes unscoped password login reject that email as ambiguous (`auth.rs:93-113`).
- ADR impact: identity binding is per-tenant by design (ADR-0031); invite flow partially contradicts it.

**F8 — Nil tenant conflates "default home" with "RLS permissive sentinel"** — *Legacy compatibility concern.*
- `bootstrap.rs:715-718` (default tenant = nil), `admin/users.rs:371-374` + `admin/groups.rs:183` (rows created into nil), `20260507220652` (nil ⇒ permissive RLS). Combined with the shipped superuser/BYPASSRLS role (given), nil-tenant rows are first-class citizens, not a compat shim; ADR-0038's "no migration may infer an institutional tenant from a zero UUID" constraint will face a large nil-tenant population in default deployments.

## Net assessment for the ADR gate

- Org/tenant identity: tenant is a real application-level predicate everywhere *except* the SCIM surface (F1) and admin surfaces (F2/F3), which are installation-global by construction. If ADR-0038 is accepted as written, SCIM v1/v2 and all `admin/*` handlers need tenant scoping before the pilot's "one tenant = one organization" claim holds under a leaked SCIM token or a non-default `RUSTSHARE_DEFAULT_TENANT_ID`.
- Nil-tenant compat: nil is the runtime default, not legacy-only (F8); treat it as a first-class tenant in migration inventories.
- User lifecycle/revocation: disable is robust and tested (F6), and credential revocation on disable is guaranteed at the database level for every writer — the `users_disable_revokes_credentials` trigger in `backend/migrations/20261003090000_revoke_credentials_on_user_disable.sql` (executed: case (h), matrix `P4-10`; the original F4 claim is withdrawn). The remaining hole is the stateless JWT: a token issued before disable or password reset becomes usable again after re-enable until its `exp` (default 24 h), because `resolve_bearer_token` performs no token-version check (F5).
