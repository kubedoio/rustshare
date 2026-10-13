# Phase 4.0 Negative-Test Run Report — Tenant Isolation Cases (a)–(j)

**Milestone:** Elembra — Phase 4.0: Institutional Authorization Decision Gate
**Baseline:** `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e` (branch `codex/phase4.0-institutional-authz`)
**Date:** 2026-10-13
**Status:** Executed evidence. No production authorization code, migration, existing test or credential was changed. No production or FWS system was accessed. Change set is additive: `scripts/phase40-tenant-isolation-negative-tests.sql` and this report.

## 1. What this report adds

The sibling audits are source-derived; the consolidated matrix `docs/audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md` records that **no row carried an `API reachability test` or `Service test` tag** and that the reproduction harness "was not executed". This report is that run report:

1. the harness `scripts/phase40-tenant-isolation-negative-tests.sql` was **executed** (36 probes, all `AS-ASSERTED`) against a migrated disposable PostgreSQL 16;
2. the repository's existing end-to-end suites that exercise the real `add_member` and real credential paths were **executed** against the same disposable stack;
3. a live `rustshare-server` debug build was **run** against the disposable database and the load-bearing claims were reproduced over HTTP (group creation, cross-tenant `add_member`, roster read, group-share creation, content denial, membership removal, admin content denial, disabled-credential rejection).

Per §7 of the matrix, its rows `#287-R13`, `R14`, `R16`, `R18`, `R19`, `P4-30`, `P4-22`, `P4-31`, `P4-09`, `P4-10`, `P4-12`, `P4-18`, `P4-38` and `#287-R02`, `R03`, `R11`, `R20` are now backed by executed results rather than source inspection.

## 2. Environment and artefacts

| Item | Value |
|---|---|
| Database | `pgvector/pgvector:pg16` (the tag `docker-compose.yml:7` pins as `@sha256:ccc6e83d6e35e931dc7c5def2022729d5a6c370318d099181995567ff1fb4d6b`), container `phase40-negtest-pg`, host port `127.0.0.1:55440`, `POSTGRES_USER=rustshare` (the shipped superuser shape). **Destroyed at the end of the run.** |
| Migrations | All 112 files under `backend/migrations` applied with `sqlx migrate run`; two databases created in the container: `rustshare` (harness probes) and `rustshare_test` (the repository's integration tests, which refuse any other name). |
| Object storage | The pre-existing disposable test container `rustshare-test-rustfs-1` (`rustfs/rustfs@sha256:60f4f2f41ce95216f8cac676e69f9d90c0bfec458a3bc7fd7fb9b7c2452ac57a`) was reused because host ports 9000 and 19000 were already bound by the repository's own test stack. It was neither created nor destroyed by this run; only new buckets prefixed `rustshare-test-phase40` were written (the harness's own allow-list). |
| Test binary | `SQLX_OFFLINE=true cargo test -p rustshare-server` (existing suites, `--ignored --test-threads=1`). |
| Live server | `target/debug/rustshare-server` on `127.0.0.1:18080` against the disposable database, with object GC, orphan reconciliation (Phase 3.4B), retention, trash cleanup and replication workers explicitly disabled. |
| `.env` handling | `dotenvy::dotenv()` searches upward and would have loaded the main checkout's `backend/../.env`. To remove that confound, `.env` in this worktree was copied from `.env.example` (it is gitignored) and every database/object-store/authz variable was passed explicitly on the command line; the file is not part of the change set. |

### Reproduce

```bash
# 1. disposable database
docker run -d --name phase40-negtest-pg \
  -e POSTGRES_USER=rustshare -e POSTGRES_PASSWORD=rustshare \
  -e POSTGRES_DB=rustshare -p 127.0.0.1:55440:5432 pgvector/pgvector:pg16
(cd backend && SQLX_OFFLINE=true sqlx migrate run \
   --database-url postgres://rustshare:rustshare@127.0.0.1:55440/rustshare)

# 2. harness cases (a)-(j): 36 probes, one deliberate ERROR at the end
docker exec -i phase40-negtest-pg psql -U rustshare -d rustshare -X -q -v ON_ERROR_STOP=1 \
  < scripts/phase40-tenant-isolation-negative-tests.sql

# 3. existing end-to-end suites (need the disposable DB + a loopback S3 on port 9000/19000)
docker exec phase40-negtest-pg psql -U rustshare -d rustshare -c \
  'CREATE DATABASE rustshare_test OWNER rustshare'
(cd backend && SQLX_OFFLINE=true sqlx migrate run \
   --database-url postgres://rustshare:rustshare@127.0.0.1:55440/rustshare_test)
env DATABASE_URL=postgres://rustshare:rustshare@127.0.0.1:55440/rustshare_test \
    RUSTSHARE_TEST_DISPOSABLE_DB=1 RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 \
    S3_ENDPOINT=http://127.0.0.1:19000 RUSTFS_ENDPOINT=http://127.0.0.1:19000 \
    S3_REGION=us-east-1 S3_BUCKET=rustshare-test-phase40 \
    RUSTFS_BUCKET=rustshare-test-phase40 \
    RUSTSHARE_OBJECT_STORE_AUTO_CREATE_BUCKET=true \
    JWT_SECRET=... RUSTSHARE_SECRET_ENCRYPTION_KEY=... \
    AWS_ACCESS_KEY_ID=... AWS_SECRET_ACCESS_KEY=... \
  SQLX_OFFLINE=true cargo test -p rustshare-server \
    --test user_disable_credential_revocation_test -- --ignored --test-threads=1
  # and: --test admin_user_disable_transaction_test --exact \
  #      group_membership_changes_and_audits_commit_or_roll_back_together

# 4. live API run: identical env plus SERVER_PORT=18080, RUSTSHARE_ADMIN_*,
#    RUSTSHARE_CHAT_PROVISIONING=manual, RUSTSHARE_DEMO_VIEWER_*= (empty)
./target/debug/rustshare-server
```

## 3. Posture re-verification (executed on the migrated database)

| Fact | Observed |
|---|---|
| Shipped app role | `rolsuper=t`, `rolbypassrls=t`, `rolcreatedb=t`, `rolcreaterole=t` for `rustshare` |
| `relforcerowsecurity` | `f` on all 10 inspected relations (`files`, `folders`, `file_versions`, `shares`, `user_groups`, `group_members`, `note_index_chunks`, `users`, `vaults`, `vault_files`); `relrowsecurity=t` only on `files`, `folders`, `file_versions`, `vaults`, `vault_files`, `vault_devices` |
| Policies | 6 policies, all `PERMISSIVE`, all `roles={public}`, all `FOR ALL`, every predicate `current_setting('app.current_user_id', true) IS NULL OR = '00000000-…' OR <owner match>` |
| Tenant context in policies | 0 policies reference `app.current_tenant_id` (harness probe 36) |
| `is_admin` in policies | 0 (harness probe 29) |

This confirms matrix facts E1–E4 and `#287-R01`, `R02`, `R04`, `R05`.

## 4. Case-by-case results

All SQL below is the harness file; the `observed` column is the value the harness itself printed and compared to its own assertion (`AS-ASSERTED` when they agree). Fixture UUIDs are fixed and the transaction is rolled back, so the run is repeatable and leaves nothing behind.

### (a) Same-organization membership allowed — baseline sanity

- **Setup:** tenant A (`a0000000-…-a1`) with owner A, member A and outsider A; file `f0000000-…-a1` owned by owner A in tenant A; group `c0000000-…-a1` (tenant A); membership (group A, member A); group share `e0000000-…-a1` (file A, group A, `tenant_id = A`, `View`).
- **Command:** probes 1–3 of the harness; plus the real path: `POST /api/v1/admin/groups/{id}/members` (existing test `admin_user_disable_transaction_test::group_membership_changes_and_audits_commit_or_roll_back_together`, executed here) and the live run.
- **Observed:** probe 2 `GroupShare:View`; probe 3 `1` (membership tenant-consistent); existing suite `1 passed`; live `add_member=204` for a same-tenant user and `add_member_C=204` for a second same-tenant user, with `GET /api/v1/files/{id}` returning `200` for that member and `403` after removal.
- **Reachability:** DIRECT API. **Classification: Intended and tested behavior.**

### (b) Cross-organization membership

- **Setup:** same as (a) plus member B in tenant B (`b0000000-…-b1`); the cross-tenant row is inserted with the exact statement `admin/groups.rs:474-484` runs.
- **Observed:**
  - probe 4 `true` — the mirror of `add_member`'s group lookup (`admin/groups.rs:445-456`) finds the group and a non-null tenant (the `Group missing tenant_id` branch at `:454-456` is unreachable: the column is `NOT NULL DEFAULT nil`);
  - probe 5 `true` — the mirror of its user-existence check (`admin/groups.rs:460-469`) accepts a tenant-B user;
  - probe 6 `1` — the membership row exists; probe 7 `1` — `membership_group_tenant_mismatches` = 1;
  - probe 8 `{}` — `get_user_group_ids(member B, tenant A)` returns no groups (the join is `users.tenant_id = $2`, `backend/crates/infrastructure/src/repositories/permission_resolver.rs:160-175`);
  - probe 9 `0` — the group-share arm therefore matches no share row for that member on the tenant-A file.
- **Real path (live):** `POST /api/v1/admin/groups` → `201`, and the stored group row has `tenant_id = 00000000-0000-0000-0000-000000000000` (the INSERT at `admin/groups.rs:183` omits `tenant_id`); `POST /api/v1/admin/groups/{id}/members {"user_id": <tenant-B user>}` → **`204`**, and the stored row shows `member_tenant = bb000000-…`, `group_tenant = 00000000-…`, `tenant_mismatch = t`.
- **Containment:** the live run also created a group share (tenant A) to that nil-tenant group and requested the file as the tenant-B member → **`403`**, and `GET /api/v1/files?limit=50` as that member → `200 []`.
- **Reachability:** DIRECT API for the membership insert and for the group-tenant defect; the content plane stays contained. **Classification: Correctness defect** (membership is not tenant-validated; cross-tenant *content* reach is blocked downstream by the share-tenant and user-tenant filters, not by any membership check). The group-tenant part is `Legacy compatibility concern`.

### (c) Group share resolution scoped correctly

- **Setup:** as (a); plus a cross-tenant requester (member B) and a non-member in tenant A (outsider A).
- **Observed:** probe 10 `NULL` (non-member resolves nothing); probe 11 `NULL` (cross-tenant requester keyed on its own tenant cannot even see the tenant-A file — `find_file_by_id` is tenant-scoped); probe 12 `1|0` (the group-share row is visible only under its own `tenant_id`).
- **Real path (live):** with the nil-tenant group share in place and the tenant-B user a member of that group, `GET /api/v1/files/{tenant-A file}` → **`403`** for the tenant-B member, `200` for the tenant-A owner, `403` for the installation admin, and `[]` for the tenant-B member's file list.
- **Reachability:** DIRECT API. **Classification: Intended and tested behavior.** The mechanism is two independent filters, both keyed on the *file's* tenant (`get_file` passes `file.tenant_id`, `backend/crates/core/src/services/file_service.rs:872`): share rows must carry the lookup tenant, and the membership join requires the requester's `users.tenant_id` to equal it.

### (d) Membership removal invalidates effective access

- **Observed:** probe 33 `GroupShare:View` before the `DELETE FROM group_members`, probe 34 `NULL` after it — resolution is a live join with no materialized file/folder ACL.
- **Real path (live):** add a same-tenant non-owner to the group → `GET /api/v1/files/{id}` `200`; `DELETE /api/v1/admin/groups/{id}/members/{uid}` → `204`; same request → `403`.
- **Reachability:** DIRECT API. **Classification: Intended and tested behavior.**
- **Separately:** the AI-index refresh (`note_service.rs:897-946`, spawned best-effort at `admin/groups.rs:506,567`) is a different, weaker path — see (g).

### (e) Legacy nil-tenant group behaviour (C2)

The reported chain is: *nil-tenant user in a nil-tenant group reaches a tenant-A file through a nil-tenant share row.*

- **Setup:** nil-tenant users `nil owner` and `nil reader`, nil-tenant groups `g_nil` and `g_nil_c4`, nil-tenant file `f_nil`, tenant-A file `f_a2`, share `e0000000-…-f2` (file `f_a2`, group `g_nil`, `tenant_id = nil`), share `e0000000-…-f3` (file `f_nil`, recipient = tenant-A member, `tenant_id = nil`), share `e0000000-…-f4` (file `f_nil`, group `g_nil`, `tenant_id = nil`).
- **Observed (executed) — the chain as stated is REFUTED:**
  - probe 13 `NULL` — with the lookup tenant = the file's tenant A, `get_user_group_ids(nil user, A)` returns `{}` because the join requires `users.tenant_id = A` and the user's tenant is nil. The group membership never resolves.
  - probe 14 `NULL` — keyed on the nil tenant instead, `find_file_by_id(f_a2, nil)` finds nothing, so the tenant-A file is still invisible.
- **Observed (executed) — what the collapse actually is:**
  - probe 15 `GroupShare:View` — within the nil tenant the principals collapse among themselves: nil reader + nil group + nil share + nil file. In a default deployment (`RUSTSHARE_DEFAULT_TENANT_ID` unset ⇒ nil, `bootstrap.rs:715-718`) these rows are manufactured by the API, so this arm is DIRECT, not synthetic.
  - probe 16 `DirectShare:View` — the reverse (and genuinely cross-tenant) direction: a **tenant-A** user reaches a **nil-tenant** file through a nil-tenant share row. Same root cause as (f): the recipient's own tenant is never compared to the share's tenant. API-reachability is SYNTHETIC here (user-share creation resolves the recipient inside the creator's tenant, `user_share_service.rs`), but the equivalent nil-tenant *public-link* path (ADR-0031) is live for legacy data.
  - probe 17 `1` — the nil-tenant group's share row on the tenant-A file exists, which is why the inventory reports it as a mismatch that resolution then ignores.
- **Reachability:** the reachable part is DIRECT (nil tenant), the refuted part was SYNTHETIC. **Classification: Correctness defect** (`Legacy compatibility concern` for the nil-tenant collapse itself).

### (f) Cross-tenant resource/share mismatch (C3)

- **Setup:** share `e0000000-…-f1` with `tenant_id = A` (the file's tenant), `recipient_user_id` = the tenant-B user, on the tenant-A file.
- **Observed:** probe 18 `DirectShare:View` — `get_file`'s keying (lookup tenant = the file's tenant, `file_service.rs:872`) grants View; probe 19 `NULL` — the ADR-0032 source authorizer's keying (requester tenant, `backend/server/src/authz/files_owner.rs:237-246`) denies / returns not-found; probe 20 `1` — the mismatched row exists exactly as described.
- **Code confirmation that it is not API-reachable:** user-share creation resolves the recipient within the creator's tenant, and group-share creation rejects `resource_tenant != requester tenant` (`backend/crates/core/src/services/share_service.rs:953-956`) and stamps the share with the caller's tenant (`:1032`).
- **Reachability:** SYNTHETIC database-state. **Classification: Correctness defect** (two evaluators implement different tenant keys for the same database state).

### (g) Admin-created group AI-index refresh (C4)

- **Setup:** admin-created group `g_nil_c4` (`tenant_id = nil`) with an active share row whose `tenant_id = A` (the shape the API produces — see the live run below). Plus the nil-tenant group's share for (e).
- **Observed:** probe 21 `0` — the shipped filter (`recipient_group_id = $1 AND tenant_id = $2` with `$2` = the group's own tenant, `note_service.rs:897-905` called from `admin/groups.rs:506,567`) matches nothing; probe 22 `1` — the corrected filter (`recipient_group_id` + `revoked_at IS NULL` only) matches the share row; probe 23 `a0000000-…-a1` — it also yields the share's own tenant, which is what the per-resource refresh needs.
- **Real path (live) — this state is DIRECT API-producible, not synthetic:** `POST /api/v1/admin/groups` → group stored with `tenant_id = nil`; tenant-A owner `POST /api/v1/files/{id}/share/group` → `201`; the resulting row shows `share_tenant = aa000000-…`, `group_tenant = 00000000-…`, `c4_mismatch = t`, and against those real rows the shipped filter returns **0** while the corrected filter returns **1**. (The same call also confirms that group-share creation skips the membership check for owners — the owner was not yet a member of the group and the share was created anyway; `share_service.rs:959-993`.)
- **Reachability:** DIRECT API (the no-op runs on every member add/remove), no access consequence today because membership is resolved live at retrieval and every search candidate is reauthorized. **Classification: Correctness defect.**

### (h) Disabled principal cannot regain access through existing credentials

- **Credential paths inspected:** `resolve_bearer_token` re-reads `users.disabled_at` per request for JWTs (`handlers/extractors.rs:66-76`) and for device tokens (`:113-121`, `AuthError::AccountDisabled` → `401`, `:35`); cookie sessions are re-checked and lazily deleted (`web_session.rs:48-61`); WebSocket resolves the user again (`handlers/ws_auth.rs:74-84`); admin routes re-check (`extractors.rs:286-303`).
- **Database-level guarantee:** migration `20261003090000_revoke_credentials_on_user_disable.sql` installs `users_disable_revokes_credentials`, an `AFTER UPDATE OF disabled_at` trigger that deletes `user_sessions` and sets `device_tokens.revoked_at` for **every** writer, "including SCIM and future administrative paths".
- **Observed (harness probes 24–28), fixture caveat:** the harness seeds the disabled account with `disabled_at` already set by `INSERT`, which does not fire the `AFTER UPDATE` trigger, so those probes isolate the *per-request* predicate rather than the trigger. 24 `true` (the predicate the extractor evaluates is true); 25 `true` (in that fixture the device token is still unrevoked, so the per-request check alone is what blocks it); 26 `true|false` (the session row exists and is gated by `disabled_at IS NULL`); 27 `false` (after re-enable the account is enabled); 28 `true|false` (device token still unrevoked, account enabled — the naive state the trigger prevents in reality; the live run below is authoritative).
- **Observed (live, authoritative):** a *bare* `UPDATE users SET disabled_at = NOW()` for a user with a live device token and a live web session produced `c_sessions_remaining = 0`, `c_device_token_revoked = t`, and `GET /api/v1/me` with the pre-existing device token → **`401 {"error":"Invalid token"}`** (the trigger revoked the token, so the disabled branch is not even reached); after re-enable the same token still → **`401`** (revocation is durable).
- **Observed (existing end-to-end suite, executed here):** `user_disable_credential_revocation_test` — `2 passed`. It asserts: JWT issued before the disable → `401`; device token issued before the disable → `401`; `device_tokens.revoked_at IS NOT NULL`; device token still `401` after re-enable; and (as a deliberately labeled change detector) a pre-disable **JWT** is honoured again after re-enable because there is no server-side JWT denylist (issue #215).
- **Reachability:** DIRECT API. **Classification: Intended and tested behavior** for the disable/revocation guarantee; the stateless-JWT resurrection after re-enable is a documented, tracked **Defense-in-depth weakness**.

### (i) Organization administrator has no implicit content access

- **Admin model:** the only admin primitive is `users.is_admin` (installation-wide), enforced by the `AdminUser` extractor (`handlers/extractors.rs:277-308`); it is not a database role, and no policy references it (probe 29 `0`).
- **Observed:** probe 30 `0` (as the admin identity under a NOSUPERUSER NOBYPASSRLS NOINHERIT role with `row_security = on`, none of the 4 fixture files is visible), probe 31 `2` (as the owner identity the owner's 2 files are visible — the probe is not vacuous), probe 32 `0` (the tenant-B file stays invisible).
- **Real path (live):** `GET /api/v1/files/{tenant-A file}` with the bootstrap admin's JWT → **`403 {"error":"Permission denied: user … cannot access file …"}`**.
- **Handler inspection:** no admin handler reads file or note bodies. `handlers/admin/users.rs` reads only metadata (`:439-441` storage-size aggregation; `:727-762` collects `file_versions.storage_key` before deleting a user, then destroys those objects) — destructive authority over the target's data, not read authority.
- **Reachability:** DIRECT API. **Classification: Intended and tested behavior** (with the caveat that the global admin remains installation-wide by design, which is `Legacy compatibility concern` for a multi-tenant reading).

### (j) Connection-pool reuse cannot leak security context

- **Hook:** `bootstrap.rs:124-138` issues session-scoped `SET app.current_tenant_id = '00000000-…'` and `SET app.current_user_id = '00000000-…'` on **every** `before_acquire`. Repository-wide, the only other writer of either variable is the synthetic probe in `backend/tests/pilot_group_scope_inventory_test.rs:322`.
- **Observed:** probe 35 `00000000-0000-0000-0000-000000000000` — the hook's reset statement leaves the setting at the nil UUID; probe 36 `0` — no policy references `app.current_tenant_id`.
- **Part 2 (same connection, session-scoped `SET` semantics):** `after_write_inside_transaction = a1000000-…-0002`; `still_set_after_commit_same_connection = a1000000-…-0002` (a plain `SET` survives the transaction and the commit — the latent leak channel is real); `after_before_acquire_reset = 00000000-…` (the hook's statement scrubs it); `fresh_connection_default = <unset>`.
- **Reachability:** the reset is DIRECT and effective; the leak channel itself is currently inert because no production path writes a real identity. **Classification: Intended and tested behavior** (reset is a correct anti-leak baseline) with a **Defense-in-depth weakness**: the reset happens on checkout, not checkin, so the guarantee depends on the hook surviving every future refactor.

### Harness result

```
### Matrix summary (expected: AS-ASSERTED | 36)
|   verdict   | probes |
| AS-ASSERTED |     36 |

### (i) part 2 — row_security = off fails closed (one deliberate ERROR follows)
ERROR:  42501: query would be affected by row-level security policy for table "files"
LOCATION:  check_enable_rls, rls.c:125
```

36/36 probes agreed with their assertions; no `CONTRADICTION` row; the one deliberate error is the required fail-closed behaviour (SQLSTATE 42501), matching `#287-R11`/`R20` and `P4-48`.

## 5. Corrections and contradictions versus the prior evidence

1. **C2 as stated is refuted (`#287-R17`, and the harness case text).** "A nil-tenant user in a nil-tenant group reaches a tenant-A file through a nil-tenant share row" does **not** happen: on the Files path the lookup tenant is the *file's* tenant, `get_user_group_ids` requires `users.tenant_id = <lookup tenant>`, and a nil-tenant user fails that. Probes 13 and 14. What is real (probes 15–17) is (i) collapse *within* the nil tenant — which in a default deployment is DIRECT-API-produced, and (ii) the reverse direction: a real-tenant user reaching a **nil-tenant** resource through a nil-tenant share row, the same class as C3. Severity is unchanged; the described mechanism is not the one that exists, and the correction matters for the ADR-0043 nil-tenant plan because it changes which direction is dangerous.
2. **C4's triggering state is DIRECT-API-producible, not synthetic (`#287-R19`, `P4-31`).** Both the nil-tenant group and the real-tenant share row are created by ordinary authenticated calls (`POST /api/v1/admin/groups`, `POST /api/v1/files/{id}/share/group`), verified live. The classification in the matrix ("DIRECT API (no-op runs on every add/remove), no access consequence today") is right; the reachability of the *state* should also read DIRECT.
3. **Tenant-identity audit F4 is refuted (independently corroborated by matrix `P4-10`).** The `users_disable_revokes_credentials` trigger (migration `20261003090000`) revokes sessions and device tokens for every writer, so SCIM disable does revoke and a re-enable does **not** resurrect device tokens. Reproduced live (§4h) and asserted green by `user_disable_credential_revocation_test`. ADR-0040's D8 rationale needs re-anchoring (the matrix already records this).
4. **Confirmed without change:** shipped superuser/BYPASSRLS role, `relforcerowsecurity = false` everywhere, decoratively-flat nil-permissive owner-only policies, no tenant predicate anywhere in SQL (policies or triggers), the nil pool-context reset, C1 (group INSERT without tenant), C3 (the `get_file` vs `FilesResourceOwner` tenant-key asymmetry), immediate membership revocation for the content plane, and no implicit admin content access.
5. **Newly observed, not previously recorded:** group-share creation accepts an owner who is **not** a group member (live `201`), which is the documented `share_service.rs:959-993` skip — worth stating explicitly because the handler maps `GroupNotFound`/`NotGroupMember` errors that the owner path can never produce.

## 6. Limits

- The cross-tenant `add_member` case is proved at the SQL level and over the live HTTP API, **not** by a new committed regression test: `backend/server/Cargo.toml` sets `autotests = false` and registers every suite explicitly, so a new test file would require editing that manifest, which the change-set constraint for this task ("additive files only") forbids. Recommended follow-up (also matrix gap G2): register a `phase40_tenant_isolation_negative_test` suite and assert that `add_member` rejects a member whose tenant differs from the group's, and that `GET /api/v1/groups/my/{id}` does not return other tenants' members.
- Case (j) proves the hook's reset statement and the session-scoped `SET` semantics on a real connection; the checkout-time reset itself is established from `bootstrap.rs:124-138` rather than by driving an `sqlx` pool, because the pool hook is not reachable without an `AppState` (and therefore an object store).
- The live run used a synthetic device-token credential (SHA-256 hash computed locally and inserted into the disposable database) instead of password login for the two tenant users, because password hashing is deliberately not reproducible from a shell; the bearer/device-token extractor is a production path (`extractors.rs:81-122`).
- Nil-tenant reachability of the *legacy public-link* path is inferred from ADR-0031 and the audit reports; it was not re-exercised here.
- Everything in this report is disposable-stack evidence at baseline `1ec2ea7a`; nothing was run against production or FWS, no reconciliation/GC path was enabled, and the container created for this run was destroyed.
