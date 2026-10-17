-- Phase 4.0 institutional authorization gate — tenant isolation negative tests.
--
-- Reproducible, self-verifying SQL evidence for cases (a)–(j) of the Phase 4.0
-- authorization decision gate. Every probe mirrors a *specific* production SQL
-- predicate, cited by `file:line`, so the result can be re-derived from source
-- without trusting this file. Nothing here touches production or FWS: the whole
-- probe set runs inside one transaction that is rolled back, and the fixtures
-- are synthetic with fixed UUIDs.
--
-- Reproduce (disposable database only):
--
--   docker run -d --name phase40-negtest-pg \
--     -e POSTGRES_USER=rustshare -e POSTGRES_PASSWORD=rustshare \
--     -e POSTGRES_DB=rustshare -p 127.0.0.1:55440:5432 pgvector/pgvector:pg16
--   (cd backend && sqlx migrate run \
--      --database-url postgres://rustshare:rustshare@127.0.0.1:55440/rustshare)
--   docker exec -i phase40-negtest-pg \
--     psql -U rustshare -d rustshare -X -q -v ON_ERROR_STOP=1 \
--     < scripts/phase40-tenant-isolation-negative-tests.sql
--
-- Expected output: a section per case, a `PROBE MATRIX` where every row reads
-- `AS-ASSERTED`, a summary line reading `AS-ASSERTED | N`, and exactly one
-- deliberate error in the final section (case i part 2: `row_security = off`
-- must fail closed with SQLSTATE 42501). That final section disables
-- ON_ERROR_STOP for itself only.
--
-- No secrets or PII: the fixture users are synthetic, and the only non-fixture
-- reads are catalogue metadata (pg_policies, pg_class, pg_roles).

BEGIN;

-- =====================================================================
-- Synthetic fixtures (transaction-local, rolled back at the end)
-- =====================================================================
\echo ''
\echo '### FIXTURES'

INSERT INTO tenants (id, name) VALUES
    ('a0000000-0000-4000-8000-0000000000a1', 'phase40 negtest tenant A'),
    ('b0000000-0000-4000-8000-0000000000b1', 'phase40 negtest tenant B');

INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota, tenant_id, disabled_at) VALUES
    ('a1000000-0000-4000-8000-000000000001', 'negtest_owner_a',    'negtest_owner_a@example.test',    'test-only-hash', 'owner A',    false, 10737418240, 'a0000000-0000-4000-8000-0000000000a1', NULL),
    ('a1000000-0000-4000-8000-000000000002', 'negtest_member_a',   'negtest_member_a@example.test',   'test-only-hash', 'member A',   false, 10737418240, 'a0000000-0000-4000-8000-0000000000a1', NULL),
    ('a1000000-0000-4000-8000-000000000003', 'negtest_outsider_a', 'negtest_outsider_a@example.test', 'test-only-hash', 'outsider A', false, 10737418240, 'a0000000-0000-4000-8000-0000000000a1', NULL),
    ('a1000000-0000-4000-8000-000000000004', 'negtest_member_b',   'negtest_member_b@example.test',   'test-only-hash', 'member B',   false, 10737418240, 'b0000000-0000-4000-8000-0000000000b1', NULL),
    ('a1000000-0000-4000-8000-000000000005', 'negtest_owner_b',    'negtest_owner_b@example.test',    'test-only-hash', 'owner B',    false, 10737418240, 'b0000000-0000-4000-8000-0000000000b1', NULL),
    ('a1000000-0000-4000-8000-000000000006', 'negtest_admin',      'negtest_admin@example.test',      'test-only-hash', 'admin',      true,  10737418240, 'a0000000-0000-4000-8000-0000000000a1', NULL),
    ('a1000000-0000-4000-8000-000000000007', 'negtest_nil_user',   'negtest_nil_user@example.test',   'test-only-hash', 'nil owner',  false, 10737418240, '00000000-0000-0000-0000-000000000000', NULL),
    ('a1000000-0000-4000-8000-000000000008', 'negtest_disabled',   'negtest_disabled@example.test',   'test-only-hash', 'disabled',   false, 10737418240, 'a0000000-0000-4000-8000-0000000000a1', NOW()),
    ('a1000000-0000-4000-8000-000000000009', 'negtest_nil_reader', 'negtest_nil_reader@example.test', 'test-only-hash', 'nil reader', false, 10737418240, '00000000-0000-0000-0000-000000000000', NULL);

-- Admin-created groups take the nil tenant: the INSERT at
-- backend/server/src/handlers/admin/groups.rs:183 omits `tenant_id`, and
-- backend/migrations/20260329170001_add_tenant_id_to_tables.sql:28 supplies the
-- zero-UUID default. G_NIL mirrors a legacy group, G_NIL_C4 the shape case (g)
-- needs (an admin-created group that later receives a share).
INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES
    ('c0000000-0000-4000-8000-0000000000a1', 'negtest group A',      'a0000000-0000-4000-8000-0000000000a1', 'a1000000-0000-4000-8000-000000000001'),
    ('c0000000-0000-4000-8000-0000000000f0', 'negtest nil group',    '00000000-0000-0000-0000-000000000000', 'a1000000-0000-4000-8000-000000000001'),
    ('c0000000-0000-4000-8000-0000000000f1', 'negtest nil group c4', '00000000-0000-0000-0000-000000000000', 'a1000000-0000-4000-8000-000000000001');

-- G_A holds a same-org member (a) and a cross-tenant member (b). The
-- cross-tenant row uses the exact statement admin/groups.rs:472-484 runs; it is
-- never rejected because `add_member` compares neither the member's tenant nor
-- the group's. G_NIL holds two nil-tenant users for (e).
INSERT INTO group_members (group_id, user_id, added_by) VALUES
    ('c0000000-0000-4000-8000-0000000000a1', 'a1000000-0000-4000-8000-000000000002', 'a1000000-0000-4000-8000-000000000006'),
    ('c0000000-0000-4000-8000-0000000000a1', 'a1000000-0000-4000-8000-000000000004', 'a1000000-0000-4000-8000-000000000006'),
    ('c0000000-0000-4000-8000-0000000000f0', 'a1000000-0000-4000-8000-000000000007', 'a1000000-0000-4000-8000-000000000006'),
    ('c0000000-0000-4000-8000-0000000000f0', 'a1000000-0000-4000-8000-000000000009', 'a1000000-0000-4000-8000-000000000006');

INSERT INTO files (id, name, path, content_hash, size, mime_type, owner_id, tenant_id, current_version, storage_key) VALUES
    ('f0000000-0000-4000-8000-0000000000a1', 'negtest-f-a.txt',   '/negtest/f-a.txt',   'negtest-hash-fa',   0, 'text/plain', 'a1000000-0000-4000-8000-000000000001', 'a0000000-0000-4000-8000-0000000000a1', 1, 'a/negtest-f-a.txt'),
    ('f0000000-0000-4000-8000-0000000000a2', 'negtest-f-a2.txt',  '/negtest/f-a2.txt',  'negtest-hash-fa2',  0, 'text/plain', 'a1000000-0000-4000-8000-000000000001', 'a0000000-0000-4000-8000-0000000000a1', 1, 'a/negtest-f-a2.txt'),
    ('f0000000-0000-4000-8000-0000000000f0', 'negtest-f-nil.txt', '/negtest/f-nil.txt', 'negtest-hash-fnil', 0, 'text/plain', 'a1000000-0000-4000-8000-000000000007', '00000000-0000-0000-0000-000000000000', 1, 'nil/negtest-f-nil.txt'),
    ('f0000000-0000-4000-8000-0000000000b1', 'negtest-f-b.txt',   '/negtest/f-b.txt',   'negtest-hash-fb',   0, 'text/plain', 'a1000000-0000-4000-8000-000000000005', 'b0000000-0000-4000-8000-0000000000b1', 1, 'b/negtest-f-b.txt');

INSERT INTO shares (id, file_id, folder_id, share_token, created_by, permissions, recipient_user_id, recipient_group_id, tenant_id, revoked_at, expires_at) VALUES
    -- (a)/(c)/(d): well-formed same-tenant group share.
    ('e0000000-0000-4000-8000-0000000000a1', 'f0000000-0000-4000-8000-0000000000a1', NULL, NULL, 'a1000000-0000-4000-8000-000000000001', 'View', NULL, 'c0000000-0000-4000-8000-0000000000a1', 'a0000000-0000-4000-8000-0000000000a1', NULL, NULL);
    -- (f) C3: share row tenant = the FILE's tenant, recipient belongs to another tenant.
INSERT INTO shares (id, file_id, folder_id, share_token, created_by, permissions, recipient_user_id, recipient_group_id, tenant_id, revoked_at, expires_at) VALUES
    ('e0000000-0000-4000-8000-0000000000f1', 'f0000000-0000-4000-8000-0000000000a1', NULL, NULL, 'a1000000-0000-4000-8000-000000000001', 'View', 'a1000000-0000-4000-8000-000000000004', NULL, 'a0000000-0000-4000-8000-0000000000a1', NULL, NULL);
    -- (e) the reported C2 chain: nil-tenant share row + nil-tenant group, real-tenant file.
INSERT INTO shares (id, file_id, folder_id, share_token, created_by, permissions, recipient_user_id, recipient_group_id, tenant_id, revoked_at, expires_at) VALUES
    ('e0000000-0000-4000-8000-0000000000f2', 'f0000000-0000-4000-8000-0000000000a2', NULL, NULL, 'a1000000-0000-4000-8000-000000000007', 'View', NULL, 'c0000000-0000-4000-8000-0000000000f0', '00000000-0000-0000-0000-000000000000', NULL, NULL);
    -- (e) reverse direction: nil-tenant file + nil-tenant share row, recipient in tenant A.
INSERT INTO shares (id, file_id, folder_id, share_token, created_by, permissions, recipient_user_id, recipient_group_id, tenant_id, revoked_at, expires_at) VALUES
    ('e0000000-0000-4000-8000-0000000000f3', 'f0000000-0000-4000-8000-0000000000f0', NULL, NULL, 'a1000000-0000-4000-8000-000000000007', 'View', 'a1000000-0000-4000-8000-000000000002', NULL, '00000000-0000-0000-0000-000000000000', NULL, NULL);
    -- (e) nil-tenant group share on a nil-tenant file: the collapse among nil-tenant principals.
INSERT INTO shares (id, file_id, folder_id, share_token, created_by, permissions, recipient_user_id, recipient_group_id, tenant_id, revoked_at, expires_at) VALUES
    ('e0000000-0000-4000-8000-0000000000f4', 'f0000000-0000-4000-8000-0000000000f0', NULL, NULL, 'a1000000-0000-4000-8000-000000000007', 'View', NULL, 'c0000000-0000-4000-8000-0000000000f0', '00000000-0000-0000-0000-000000000000', NULL, NULL);
    -- (g) C4: share to an admin-created (nil-tenant) group carries the creator's real tenant.
INSERT INTO shares (id, file_id, folder_id, share_token, created_by, permissions, recipient_user_id, recipient_group_id, tenant_id, revoked_at, expires_at) VALUES
    ('e0000000-0000-4000-8000-0000000000f5', 'f0000000-0000-4000-8000-0000000000a1', NULL, NULL, 'a1000000-0000-4000-8000-000000000001', 'View', NULL, 'c0000000-0000-4000-8000-0000000000f1', 'a0000000-0000-4000-8000-0000000000a1', NULL, NULL);

-- Device token + web session minted while the account was enabled, then
-- `disabled_at` is set out of band (case h).
INSERT INTO device_tokens (id, user_id, token_hash, device_name) VALUES
    ('d0000000-0000-4000-8000-0000000000f1', 'a1000000-0000-4000-8000-000000000008', 'negtest-disabled-device-token-hash', 'phase40 negtest');
INSERT INTO user_sessions (id, user_id, session_token_hash, tenant_id, expires_at) VALUES
    ('d0000000-0000-4000-8000-0000000000f2', 'a1000000-0000-4000-8000-000000000008', 'negtest-disabled-session-token-hash', 'a0000000-0000-4000-8000-0000000000a1', NOW() + INTERVAL '7 days');

-- =====================================================================
-- Mirrors of the production predicates, so every probe names its source
-- =====================================================================

-- Mirror of PermissionResolverRepository::get_user_group_ids
-- backend/crates/infrastructure/src/repositories/permission_resolver.rs:160-175
CREATE FUNCTION pg_temp.rs_user_group_ids(p_user uuid, p_tenant uuid)
RETURNS uuid[] LANGUAGE sql STABLE AS $$
    SELECT COALESCE(array_agg(gm.group_id ORDER BY gm.group_id), ARRAY[]::uuid[])
    FROM group_members gm
    JOIN users u ON u.id = gm.user_id
    WHERE gm.user_id = p_user AND u.tenant_id = p_tenant
$$;

-- Mirror of PermissionResolver::check_file_permission steps 1-3
-- backend/crates/core/src/services/permission_resolver.rs:193-284
-- (folder-ancestry step 4 is omitted: no fixture file has a parent folder).
-- The tenant argument is supplied by the caller, which is the whole point of
-- case (f): FileService::get_file passes file.tenant_id
-- (backend/crates/core/src/services/file_service.rs:865,872) while
-- FilesResourceOwner passes ctx.tenant_id.0
-- (backend/server/src/authz/files_owner.rs:237-246).
CREATE FUNCTION pg_temp.rs_check_file_view(p_user uuid, p_tenant uuid, p_file uuid)
RETURNS text LANGUAGE plpgsql STABLE AS $$
DECLARE
    v_owner uuid;
    v_perm  text;
    v_groups uuid[];
BEGIN
    -- step 0: find_file_by_id(p_file, p_tenant) — no row means NotFound / deny.
    SELECT owner_id INTO v_owner
    FROM files
    WHERE id = p_file AND tenant_id = p_tenant;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;

    -- step 1: owner holds Admin.
    IF v_owner = p_user THEN
        RETURN 'Owner:Admin';
    END IF;

    -- step 2: direct share on the file, tenant-scoped to the lookup tenant.
    SELECT s.permissions INTO v_perm
    FROM shares s
    WHERE s.file_id = p_file
      AND s.folder_id IS NULL
      AND s.recipient_user_id = p_user
      AND s.tenant_id = p_tenant
      AND s.revoked_at IS NULL
      AND (s.expires_at IS NULL OR s.expires_at > NOW())
    LIMIT 1;
    IF FOUND THEN
        RETURN 'DirectShare:' || v_perm;
    END IF;

    -- step 3: group shares on the file, highest permission wins.
    v_groups := pg_temp.rs_user_group_ids(p_user, p_tenant);
    IF array_length(v_groups, 1) IS NOT NULL THEN
        SELECT s.permissions INTO v_perm
        FROM shares s
        WHERE s.file_id = p_file
          AND s.folder_id IS NULL
          AND s.recipient_group_id = ANY(v_groups)
          AND s.tenant_id = p_tenant
          AND s.revoked_at IS NULL
          AND (s.expires_at IS NULL OR s.expires_at > NOW())
        ORDER BY array_position(ARRAY['View', 'Edit', 'Admin'], s.permissions) DESC
        LIMIT 1;
        IF FOUND THEN
            RETURN 'GroupShare:' || v_perm;
        END IF;
    END IF;

    RETURN NULL;
END
$$;

CREATE TEMP TABLE phase40_probe_results (
    ordinal              integer,
    case_id              text,
    probe                text,
    asserted             text,
    observed             text,
    boundary_requirement text,
    classification       text
);

-- Short aliases, so every probe stays readable.
CREATE TEMP TABLE phase40_ids AS SELECT
    'a0000000-0000-4000-8000-0000000000a1'::uuid AS t_a,
    'b0000000-0000-4000-8000-0000000000b1'::uuid AS t_b,
    '00000000-0000-0000-0000-000000000000'::uuid AS nil,
    'a1000000-0000-4000-8000-000000000001'::uuid AS u_owner_a,
    'a1000000-0000-4000-8000-000000000002'::uuid AS u_member_a,
    'a1000000-0000-4000-8000-000000000003'::uuid AS u_outsider_a,
    'a1000000-0000-4000-8000-000000000004'::uuid AS u_member_b,
    'a1000000-0000-4000-8000-000000000006'::uuid AS u_admin,
    'a1000000-0000-4000-8000-000000000007'::uuid AS u_nil,
    'a1000000-0000-4000-8000-000000000008'::uuid AS u_disabled,
    'a1000000-0000-4000-8000-000000000009'::uuid AS u_nil_reader,
    'c0000000-0000-4000-8000-0000000000a1'::uuid AS g_a,
    'c0000000-0000-4000-8000-0000000000f0'::uuid AS g_nil,
    'c0000000-0000-4000-8000-0000000000f1'::uuid AS g_nil_c4,
    'f0000000-0000-4000-8000-0000000000a1'::uuid AS f_a,
    'f0000000-0000-4000-8000-0000000000a2'::uuid AS f_a2,
    'f0000000-0000-4000-8000-0000000000f0'::uuid AS f_nil,
    'f0000000-0000-4000-8000-0000000000b1'::uuid AS f_b;

-- =====================================================================
-- (a) Same-organization membership is allowed — baseline sanity
-- =====================================================================
\echo ''
\echo '### (a) same-organization membership allowed'

INSERT INTO phase40_probe_results
SELECT 1, 'a', 'owner of the tenant-A file resolves as Owner', 'Owner:Admin',
       pg_temp.rs_check_file_view(i.u_owner_a, i.t_a, i.f_a),
       'Owner keeps Admin on own resource', 'intended-tested'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 2, 'a', 'same-tenant member of G_A resolves the G_A group share', 'GroupShare:View',
       pg_temp.rs_check_file_view(i.u_member_a, i.t_a, i.f_a),
       'Same-org membership grants the shared permission', 'intended-tested'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 3, 'a', 'same-org membership row is tenant-consistent (member tenant = group tenant)', '1',
       (SELECT count(*)::text FROM group_members gm
        JOIN user_groups g ON g.id = gm.group_id
        JOIN users u ON u.id = gm.user_id
        WHERE gm.group_id = i.g_a AND gm.user_id = i.u_member_a
          AND u.tenant_id = g.tenant_id)
FROM phase40_ids i;

-- =====================================================================
-- (b) Cross-organization membership — SQL level
-- =====================================================================
-- `add_member` (backend/server/src/handlers/admin/groups.rs:437-511) performs
-- exactly three checks: the group exists (:444-456), `tenant_id` is not NULL
-- (:454-456 — unreachable, the column is NOT NULL), and the user exists
-- (:459-469). Nothing compares the member's tenant to the group's.
\echo ''
\echo '### (b) cross-organization membership is NOT tenant-validated'

INSERT INTO phase40_probe_results
SELECT 4, 'b', 'mirror of add_member''s group-existence + tenant read: group found, tenant non-null', 'true',
       (SELECT (EXISTS(SELECT 1 FROM user_groups WHERE id = i.g_a)
                AND (SELECT tenant_id FROM user_groups WHERE id = i.g_a) IS NOT NULL)::text
        FROM phase40_ids i),
       'Group lookup must not be the only gate', 'correctness-defect';

INSERT INTO phase40_probe_results
SELECT 5, 'b', 'mirror of add_member''s user-existence check accepts a user from another tenant', 'true',
       (SELECT EXISTS(SELECT 1 FROM users WHERE id = i.u_member_b)::text FROM phase40_ids i),
       'Membership requires users.tenant_id = user_groups.tenant_id', 'correctness-defect';

INSERT INTO phase40_probe_results
SELECT 6, 'b', 'cross-tenant membership row is accepted by add_member''s INSERT (no tenant predicate rejects it)', '1',
       (SELECT count(*)::text FROM group_members WHERE group_id = i.g_a AND user_id = i.u_member_b)
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 7, 'b', 'inventory counter membership_group_tenant_mismatches for G_A', '1',
       (SELECT count(*)::text FROM group_members gm
        JOIN user_groups g ON g.id = gm.group_id
        JOIN users u ON u.id = gm.user_id
        WHERE gm.group_id = i.g_a AND u.tenant_id IS DISTINCT FROM g.tenant_id)
FROM phase40_ids i;

-- Downstream containment: get_user_group_ids keys the membership join off the
-- *lookup* tenant, so the foreign member's membership never resolves when the
-- lookup tenant is the group's / the file's tenant.
INSERT INTO phase40_probe_results
SELECT 8, 'b', 'get_user_group_ids(foreign member, group tenant) returns no groups', '{}',
       pg_temp.rs_user_group_ids(i.u_member_b, i.t_a)::text
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 9, 'b', 'group-share arm for the foreign member on the tenant-A file matches 0 share rows', '0',
       (SELECT count(*)::text FROM shares s
        WHERE s.file_id = i.f_a AND s.folder_id IS NULL
          AND s.recipient_group_id = ANY(pg_temp.rs_user_group_ids(i.u_member_b, i.t_a))
          AND s.tenant_id = i.t_a AND s.revoked_at IS NULL)
FROM phase40_ids i;

-- =====================================================================
-- (c) Group share resolution is scoped: non-member and cross-tenant get nothing
-- =====================================================================
\echo ''
\echo '### (c) group share resolution scoping'

INSERT INTO phase40_probe_results
SELECT 10, 'c', 'same-tenant non-member resolves no group share', 'NULL',
       COALESCE(pg_temp.rs_check_file_view(i.u_outsider_a, i.t_a, i.f_a), 'NULL'),
       'Membership is required', 'intended-tested'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 11, 'c', 'cross-tenant requester keyed on its own tenant cannot see the tenant-A file at all', 'NULL',
       COALESCE(pg_temp.rs_check_file_view(i.u_member_b, i.t_b, i.f_a), 'NULL'),
       'Tenant is the visibility boundary', 'intended-tested'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 12, 'c', 'the G_A group share is visible only under the share''s own tenant', '1|0',
       (SELECT (count(*) FILTER (WHERE s.tenant_id = i.t_a))::text || '|' ||
               (count(*) FILTER (WHERE s.tenant_id = i.t_b))::text
        FROM shares s
        WHERE s.file_id = i.f_a AND s.recipient_group_id = i.g_a AND s.revoked_at IS NULL),
       'Share rows must be tenant-scoped', 'intended-tested'
FROM phase40_ids i;

-- =====================================================================
-- (e) Legacy nil-tenant groups (C2)
-- =====================================================================
-- The reported C2 chain is "nil-tenant user in a nil-tenant group reaches a
-- tenant-A file through a nil-tenant share row". On the Files HTTP path the
-- lookup tenant is the FILE's tenant, and get_user_group_ids filters on the
-- *user's* tenant, so the nil-tenant membership does not resolve. Probes 13-14
-- refute the chain as stated; 15-17 show what the collapse actually is.
\echo ''
\echo '### (e) legacy nil-tenant group behaviour (C2)'

INSERT INTO phase40_probe_results
SELECT 13, 'e', 'nil-tenant user + nil-tenant group + nil-tenant share row does NOT reach the tenant-A file', 'NULL',
       COALESCE(pg_temp.rs_check_file_view(i.u_nil, i.t_a, i.f_a2), 'NULL'),
       'A nil-tenant membership must not resolve across tenants', 'intended-tested'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 14, 'e', 'the same chain keyed on the nil tenant still finds no file (find_file_by_id is tenant-scoped)', 'NULL',
       COALESCE(pg_temp.rs_check_file_view(i.u_nil, i.nil, i.f_a2), 'NULL'),
       'File lookup is tenant-scoped', 'intended-tested'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 15, 'e', 'nil tenant collapses among its own principals: nil reader + nil group + nil share + nil file = View', 'GroupShare:View',
       COALESCE(pg_temp.rs_check_file_view(i.u_nil_reader, i.nil, i.f_nil), 'NULL'),
       'The nil tenant has no boundary of its own', 'legacy-compat'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 16, 'e', 'reverse direction: a tenant-A user reaches a nil-tenant file through a nil-tenant share row', 'DirectShare:View',
       COALESCE(pg_temp.rs_check_file_view(i.u_member_a, i.nil, i.f_nil), 'NULL'),
       'Recipient tenant must match the share tenant', 'correctness-defect'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 17, 'e', 'the nil-tenant group''s share row on the tenant-A file exists (inventory says why it is unreachable)', '1',
       (SELECT count(*)::text FROM shares s
        WHERE s.recipient_group_id = i.g_nil AND s.tenant_id = i.nil
          AND s.file_id = i.f_a2 AND s.revoked_at IS NULL)
FROM phase40_ids i;

-- =====================================================================
-- (f) Cross-tenant resource/share mismatch (C3) — the decision asymmetry
-- =====================================================================
\echo ''
\echo '### (f) C3 — FileService::get_file vs FilesResourceOwner keying'

INSERT INTO phase40_probe_results
SELECT 18, 'f', 'get_file keying (lookup tenant = file tenant): the foreign recipient gets View', 'DirectShare:View',
       COALESCE(pg_temp.rs_check_file_view(i.u_member_b, i.t_a, i.f_a), 'NULL'),
       'No cross-tenant grant may exist', 'correctness-defect'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 19, 'f', 'FilesResourceOwner keying (lookup tenant = requester tenant): the same rows deny / not-found', 'NULL',
       COALESCE(pg_temp.rs_check_file_view(i.u_member_b, i.t_b, i.f_a), 'NULL'),
       'One keying must be authoritative', 'correctness-defect'
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 20, 'f', 'the mismatched share row exists exactly as C3 describes (share tenant = file tenant, recipient elsewhere)', '1',
       (SELECT count(*)::text FROM shares s
        JOIN users r ON r.id = s.recipient_user_id
        JOIN files f ON f.id = s.file_id
        WHERE s.id = 'e0000000-0000-4000-8000-0000000000f1'
          AND s.tenant_id = f.tenant_id AND r.tenant_id <> s.tenant_id)
FROM phase40_ids i;

-- =====================================================================
-- (g) Admin-created (nil-tenant) group AI-index refresh (C4)
-- =====================================================================
-- Mirror of NoteService::refresh_index_acl_for_group
-- backend/server/src/services/note_service.rs:897-905, which is called with the
-- group's own tenant_id (backend/server/src/handlers/admin/groups.rs:444-456,
-- :506, :567).
\echo ''
\echo '### (g) C4 — group-scoped index refresh filters on the wrong tenant'

INSERT INTO phase40_probe_results
SELECT 21, 'g', 'shipped filter (recipient_group_id = G AND tenant_id = group.tenant_id) matches 0 rows', '0',
       (SELECT count(*)::text FROM shares s
        JOIN user_groups g ON g.id = i.g_nil_c4
        WHERE s.recipient_group_id = i.g_nil_c4
          AND s.tenant_id = g.tenant_id
          AND s.revoked_at IS NULL)
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 22, 'g', 'corrected filter (recipient_group_id = G AND revoked_at IS NULL) matches the share row', '1',
       (SELECT count(*)::text FROM shares s
        WHERE s.recipient_group_id = i.g_nil_c4 AND s.revoked_at IS NULL)
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 23, 'g', 'corrected filter exposes the share''s own tenant for the per-resource refresh', 'a0000000-0000-4000-8000-0000000000a1',
       (SELECT DISTINCT s.tenant_id::text FROM shares s
        WHERE s.recipient_group_id = i.g_nil_c4 AND s.revoked_at IS NULL)
FROM phase40_ids i;

-- =====================================================================
-- (h) Disabled user cannot regain access through existing credentials
-- =====================================================================
-- Mirror of the per-request disabled check in resolve_bearer_token
-- backend/server/src/handlers/extractors.rs:65-76 (JWT) and :111-122 (device
-- token); web sessions in backend/server/src/web_session.rs:47-61; WebSocket in
-- backend/server/src/handlers/ws_auth.rs:69-84.
\echo ''
\echo '### (h) disabled principal and credential lifecycle'

INSERT INTO phase40_probe_results
SELECT 24, 'h', 'the extractor''s per-request predicate is true for the disabled account (JWT path)', 'true',
       (SELECT (disabled_at IS NOT NULL)::text FROM users WHERE id = i.u_disabled)
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 25, 'h', 'the disabled account''s device token is still unrevoked, so only the disabled check blocks it', 'true',
       (SELECT (revoked_at IS NULL)::text FROM device_tokens
        WHERE user_id = i.u_disabled AND token_hash = 'negtest-disabled-device-token-hash')
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 26, 'h', 'the disabled account''s web session row still exists and is gated only by `disabled_at IS NULL`', 'true|false',
       (SELECT EXISTS(SELECT 1 FROM user_sessions
                      WHERE user_id = i.u_disabled
                        AND session_token_hash = 'negtest-disabled-session-token-hash')::text
               || '|' ||
               (SELECT (disabled_at IS NULL)::text FROM users WHERE id = i.u_disabled))
FROM phase40_ids i;

-- Out-of-band re-enable in the SCIM shape (backend/server/src/handlers/scim.rs:431-456
-- and scim_v2.rs:360-398 only touch users.disabled_at): the untouched device
-- token becomes usable again because nothing revoked it.
UPDATE users SET disabled_at = NULL WHERE id = 'a1000000-0000-4000-8000-000000000008';

INSERT INTO phase40_probe_results
SELECT 27, 'h', 'after a bare re-enable the account is enabled again', 'false',
       (SELECT (disabled_at IS NOT NULL)::text FROM users WHERE id = i.u_disabled)
FROM phase40_ids i;

INSERT INTO phase40_probe_results
SELECT 28, 'h', 'the pre-existing device token was never revoked, so it resurrects on re-enable (SCIM path)',
       'true|false',
       (SELECT (revoked_at IS NULL)::text FROM device_tokens
        WHERE user_id = i.u_disabled AND token_hash = 'negtest-disabled-device-token-hash')
       || '|' ||
       (SELECT (disabled_at IS NOT NULL)::text FROM users WHERE id = i.u_disabled)
FROM phase40_ids i;

UPDATE users SET disabled_at = NOW() WHERE id = 'a1000000-0000-4000-8000-000000000008';

-- =====================================================================
-- (i) Organization administrator has no implicit content access
-- =====================================================================
-- Admin-ness is `users.is_admin` (backend/server/src/handlers/extractors.rs:277-308)
-- and appears in no policy (probe 29) and no resolver predicate. Probes 30-32
-- read the tables as a NOSUPERUSER NOBYPASSRLS NOINHERIT role with
-- row_security = on.
\echo ''
\echo '### (i) admin role confers no content access'

INSERT INTO phase40_probe_results
SELECT 29, 'i', 'no RLS policy mentions is_admin (admin is not a database role)', '0',
       (SELECT count(*)::text FROM pg_policies WHERE COALESCE(qual, '') ILIKE '%is_admin%'),
       'Admin must not be a content role', 'intended-tested';

SAVEPOINT before_rls_probes;
CREATE ROLE phase40_negtest_probe NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
GRANT SELECT ON users, files, folders, shares, user_groups, group_members TO phase40_negtest_probe;

SET LOCAL ROLE phase40_negtest_probe;
SET LOCAL row_security = on;

SET LOCAL app.current_user_id = 'a1000000-0000-4000-8000-000000000006';
SELECT count(*) AS admin_identity_file_rows
FROM files
WHERE id IN ('f0000000-0000-4000-8000-0000000000a1', 'f0000000-0000-4000-8000-0000000000a2',
             'f0000000-0000-4000-8000-0000000000f0', 'f0000000-0000-4000-8000-0000000000b1')
\gset i_admin_

SET LOCAL app.current_user_id = 'a1000000-0000-4000-8000-000000000001';
SELECT count(*) AS owner_identity_file_rows
FROM files
WHERE id IN ('f0000000-0000-4000-8000-0000000000a1', 'f0000000-0000-4000-8000-0000000000a2',
             'f0000000-0000-4000-8000-0000000000f0', 'f0000000-0000-4000-8000-0000000000b1')
\gset i_owner_

SET LOCAL app.current_user_id = 'a1000000-0000-4000-8000-000000000001';
SELECT count(*) AS tenant_b_file_rows
FROM files
WHERE id = 'f0000000-0000-4000-8000-0000000000b1'
\gset i_b_

RESET ROLE;
ROLLBACK TO SAVEPOINT before_rls_probes;

INSERT INTO phase40_probe_results
SELECT 30, 'i', 'hardened role, admin identity: none of the 4 fixture files is visible',
       '0', :'i_admin_admin_identity_file_rows',
       'Admin is not a content role', 'intended-tested';

INSERT INTO phase40_probe_results
SELECT 31, 'i', 'hardened role, owner identity: only that owner''s 2 files are visible (probe is not vacuous)',
       '2', :'i_owner_owner_identity_file_rows',
       'Owner isolation is the only RLS arm', 'defense-in-depth';

INSERT INTO phase40_probe_results
SELECT 32, 'i', 'hardened role, tenant-A owner identity: the tenant-B file is invisible',
       '0', :'i_b_tenant_b_file_rows',
       'No cross-tenant row visibility', 'defense-in-depth';

-- =====================================================================
-- (d) Membership removal invalidates effective access
-- =====================================================================
\echo ''
\echo '### (d) membership removal invalidates access'

INSERT INTO phase40_probe_results
SELECT 33, 'd', 'before removal the same-org member resolves the G_A group share', 'GroupShare:View',
       COALESCE(pg_temp.rs_check_file_view(i.u_member_a, i.t_a, i.f_a), 'NULL'),
       'Membership is a live join', 'intended-tested'
FROM phase40_ids i;

DELETE FROM group_members
WHERE group_id = 'c0000000-0000-4000-8000-0000000000a1'
  AND user_id = 'a1000000-0000-4000-8000-000000000002';

INSERT INTO phase40_probe_results
SELECT 34, 'd', 'after removal the same-org member resolves nothing (no cached ACL at the API layer)', 'NULL',
       COALESCE(pg_temp.rs_check_file_view(i.u_member_a, i.t_a, i.f_a), 'NULL'),
       'Revocation must take effect immediately', 'intended-tested'
FROM phase40_ids i;

-- =====================================================================
-- (j) Connection-pool reuse cannot leak security context
-- =====================================================================
-- The shipped hook (backend/server/src/bootstrap.rs:124-138) runs
-- `SET app.current_user_id = <nil>` and `SET app.current_tenant_id = <nil>` on
-- every pool checkout. Probe 35 shows that the exact statement yields the nil
-- default; part 2 (after ROLLBACK, below) shows the session scope that makes the
-- reset necessary.
\echo ''
\echo '### (j) pool context reset'

SAVEPOINT before_reset_probe;
SET LOCAL app.current_user_id = 'a1000000-0000-4000-8000-000000000002';
SET LOCAL app.current_user_id = '00000000-0000-0000-0000-000000000000';
SELECT current_setting('app.current_user_id') AS after_hook_reset
\gset j_
ROLLBACK TO SAVEPOINT before_reset_probe;

INSERT INTO phase40_probe_results
SELECT 35, 'j', 'the hook''s reset statement leaves app.current_user_id at the nil UUID',
       '00000000-0000-0000-0000-000000000000', :'j_after_hook_reset',
       'Every checkout must start from the nil default', 'intended-tested';

INSERT INTO phase40_probe_results
SELECT 36, 'j', 'app.current_tenant_id is set by the pool but referenced by no policy (so it constrains nothing)', '0',
       (SELECT count(*)::text FROM pg_policies WHERE COALESCE(qual, '') ILIKE '%current_tenant_id%'),
       'Tenant context must be enforced somewhere, or be documented as inert', 'defense-in-depth';

\echo ''
\echo '### PROBE MATRIX'
\pset border 2
SELECT ordinal, case_id, probe, asserted, observed,
       CASE WHEN observed IS NOT DISTINCT FROM asserted THEN 'AS-ASSERTED' ELSE 'CONTRADICTION' END AS verdict,
       classification
FROM phase40_probe_results
ORDER BY ordinal;

\echo ''
\echo '### Matrix summary (expected: AS-ASSERTED | 36)'
SELECT CASE WHEN observed IS NOT DISTINCT FROM asserted THEN 'AS-ASSERTED' ELSE 'CONTRADICTION' END AS verdict,
       count(*) AS probes
FROM phase40_probe_results
GROUP BY 1
ORDER BY 1;

\echo ''
\echo '### Boundary requirement per probe'
SELECT ordinal, case_id, boundary_requirement
FROM phase40_probe_results
ORDER BY ordinal;

ROLLBACK;

-- =====================================================================
-- (j) part 2 — session-scoped GUC semantics on one connection
-- =====================================================================
-- A plain `SET` is session-scoped and survives transaction boundaries, so a
-- value written by one request stays on the idle connection after checkin and is
-- only scrubbed by the next checkout's `before_acquire` hook. This is the latent
-- leak channel that the shipped hook closes.
\echo ''
\echo '### (j) part 2 — session-scoped SET persists across transactions and is scrubbed on reset'
SET app.current_user_id = 'a1000000-0000-4000-8000-000000000002';
BEGIN;
SELECT current_setting('app.current_user_id') AS after_write_inside_transaction;
COMMIT;
SELECT current_setting('app.current_user_id') AS still_set_after_commit_same_connection;
SET app.current_user_id = '00000000-0000-0000-0000-000000000000';
SELECT current_setting('app.current_user_id') AS after_before_acquire_reset;
RESET app.current_user_id;
SELECT COALESCE(NULLIF(current_setting('app.current_user_id', true), ''), '<unset>') AS fresh_connection_default;

-- =====================================================================
-- (i) part 2 — row_security = off must fail closed (SQLSTATE 42501)
-- =====================================================================
-- Expected to print exactly one ERROR carrying SQLSTATE 42501. A non-owner role
-- that turns row security off must raise rather than silently return a
-- policy-filtered result;
-- backend/migrations/20260507220652_fix_rls_to_allow_nil_default.sql is why a
-- filtered read is otherwise indistinguishable from a full read.
\echo ''
\echo '### (i) part 2 — row_security = off fails closed (one deliberate ERROR follows)'
\set ON_ERROR_STOP off
\set VERBOSITY verbose
BEGIN;
CREATE ROLE phase40_negtest_probe_fc NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
GRANT SELECT ON files TO phase40_negtest_probe_fc;
SET LOCAL ROLE phase40_negtest_probe_fc;
SET LOCAL app.current_user_id = 'a1000000-0000-4000-8000-000000000001';
SET LOCAL row_security = off;
SELECT count(*) AS must_not_return FROM files;
ROLLBACK;
DROP ROLE IF EXISTS phase40_negtest_probe_fc;
\set VERBOSITY default
\set ON_ERROR_STOP on
