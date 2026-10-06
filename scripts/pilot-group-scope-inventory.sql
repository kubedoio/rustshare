-- Read-only inventory for the Issue #333 / #330 group migration review.
-- Run against the intended database only after arranging secure, read-only
-- operator credentials:
--   psql -X "$DATABASE_URL" -v ON_ERROR_STOP=1 -f scripts/pilot-group-scope-inventory.sql
--
-- Output contains tenant/group UUIDs and aggregate counts, but no user names,
-- emails, resource names, or content. Treat the output as confidential
-- operational metadata; do not commit or attach it to a public issue.
-- Zero mismatch counts are not proof of correct historical ownership: legacy
-- rows may all carry the shared zero UUID and need an independent mapping.

BEGIN TRANSACTION READ ONLY;

-- Do not silently accept an inventory filtered by row-level security. This
-- setting does not bypass RLS; PostgreSQL raises an error if a policy would
-- hide rows. Roles that bypass RLS (including table owners in the default
-- configuration) are unaffected.
SET LOCAL row_security = off;

WITH membership_scope AS (
    SELECT
        gm.group_id,
        COUNT(*) AS membership_rows,
        COUNT(*) FILTER (WHERE u.id IS NULL) AS unresolved_user_rows,
        COUNT(DISTINCT u.tenant_id) FILTER (WHERE u.id IS NOT NULL) AS member_tenant_count,
        COALESCE(
            ARRAY_AGG(DISTINCT u.tenant_id ORDER BY u.tenant_id)
                FILTER (WHERE u.id IS NOT NULL),
            ARRAY[]::UUID[]
        ) AS member_tenant_ids,
        COUNT(*) FILTER (
            WHERE u.id IS NOT NULL AND u.tenant_id IS DISTINCT FROM g.tenant_id
        )
            AS membership_group_tenant_mismatches
    FROM group_members gm
    JOIN user_groups g ON g.id = gm.group_id
    LEFT JOIN users u ON u.id = gm.user_id
    GROUP BY gm.group_id, g.tenant_id
), group_share_scope AS (
    SELECT
        s.recipient_group_id AS group_id,
        COUNT(*) FILTER (
            WHERE s.revoked_at IS NULL
              AND (s.expires_at IS NULL OR s.expires_at > CURRENT_TIMESTAMP)
        ) AS unrevoked_unexpired_share_references,
        COUNT(*) FILTER (
            WHERE s.revoked_at IS NULL
              AND s.expires_at IS NOT NULL
              AND s.expires_at <= CURRENT_TIMESTAMP
        ) AS expired_unrevoked_share_references,
        COUNT(*) FILTER (WHERE s.revoked_at IS NOT NULL) AS revoked_share_references,
        COUNT(DISTINCT s.tenant_id) AS share_tenant_count,
        COALESCE(
            ARRAY_AGG(DISTINCT s.tenant_id ORDER BY s.tenant_id),
            ARRAY[]::UUID[]
        ) AS share_tenant_ids,
        COUNT(*) FILTER (WHERE s.tenant_id IS DISTINCT FROM g.tenant_id)
            AS share_group_tenant_mismatches,
        COUNT(*) FILTER (
            WHERE (s.file_id IS NOT NULL AND f.id IS NOT NULL
                   AND f.tenant_id IS DISTINCT FROM s.tenant_id)
               OR (s.folder_id IS NOT NULL AND d.id IS NOT NULL
                   AND d.tenant_id IS DISTINCT FROM s.tenant_id)
        ) AS share_resource_tenant_mismatches,
        COUNT(*) FILTER (
            WHERE s.file_id IS NOT NULL AND f.id IS NOT NULL AND fu.id IS NOT NULL
              AND fu.tenant_id IS DISTINCT FROM f.tenant_id
        ) AS file_owner_tenant_mismatches,
        COUNT(*) FILTER (
            WHERE s.folder_id IS NOT NULL AND d.id IS NOT NULL AND du.id IS NOT NULL
              AND du.tenant_id IS DISTINCT FROM d.tenant_id
        ) AS folder_owner_tenant_mismatches,
        COUNT(*) FILTER (
            WHERE creator.id IS NOT NULL
              AND creator.tenant_id IS DISTINCT FROM s.tenant_id
        )
            AS share_creator_tenant_mismatches,
        COUNT(*) FILTER (WHERE s.file_id IS NOT NULL AND f.id IS NOT NULL AND fu.id IS NULL)
            AS missing_file_owner_user_references,
        COUNT(*) FILTER (WHERE s.folder_id IS NOT NULL AND d.id IS NOT NULL AND du.id IS NULL)
            AS missing_folder_owner_user_references,
        COUNT(*) FILTER (WHERE s.created_by IS NOT NULL AND creator.id IS NULL)
            AS missing_share_creator_user_references,
        COUNT(*) FILTER (
            WHERE (s.file_id IS NULL AND s.folder_id IS NULL)
               OR (s.file_id IS NOT NULL AND s.folder_id IS NOT NULL)
        ) AS malformed_share_target_references,
        COUNT(*) FILTER (WHERE s.file_id IS NOT NULL AND f.id IS NULL)
            AS missing_file_references,
        COUNT(*) FILTER (WHERE s.folder_id IS NOT NULL AND d.id IS NULL)
            AS missing_folder_references
    FROM shares s
    JOIN user_groups g ON g.id = s.recipient_group_id
    LEFT JOIN files f ON f.id = s.file_id
    LEFT JOIN users fu ON fu.id = f.owner_id
    LEFT JOIN folders d ON d.id = s.folder_id
    LEFT JOIN users du ON du.id = d.owner_id
    LEFT JOIN users creator ON creator.id = s.created_by
    WHERE s.recipient_group_id IS NOT NULL
    GROUP BY s.recipient_group_id, g.tenant_id
)
SELECT
    g.id AS group_id,
    g.tenant_id AS recorded_group_tenant_id,
    (g.tenant_id = '00000000-0000-0000-0000-000000000000'::UUID) AS has_legacy_zero_tenant,
    COALESCE(m.membership_rows, 0) AS membership_rows,
    COALESCE(m.unresolved_user_rows, 0) AS unresolved_user_rows,
    COALESCE(m.member_tenant_count, 0) AS member_tenant_count,
    COALESCE(m.member_tenant_ids, ARRAY[]::UUID[]) AS member_tenant_ids,
    COALESCE(m.membership_group_tenant_mismatches, 0) AS membership_group_tenant_mismatches,
    COALESCE(s.unrevoked_unexpired_share_references, 0)
        AS unrevoked_unexpired_share_references,
    COALESCE(s.expired_unrevoked_share_references, 0)
        AS expired_unrevoked_share_references,
    COALESCE(s.revoked_share_references, 0) AS revoked_share_references,
    COALESCE(s.share_tenant_count, 0) AS share_tenant_count,
    COALESCE(s.share_tenant_ids, ARRAY[]::UUID[]) AS share_tenant_ids,
    COALESCE(s.share_group_tenant_mismatches, 0) AS share_group_tenant_mismatches,
    COALESCE(s.share_resource_tenant_mismatches, 0) AS share_resource_tenant_mismatches,
    COALESCE(s.file_owner_tenant_mismatches, 0) AS file_owner_tenant_mismatches,
    COALESCE(s.folder_owner_tenant_mismatches, 0) AS folder_owner_tenant_mismatches,
    COALESCE(s.share_creator_tenant_mismatches, 0) AS share_creator_tenant_mismatches,
    COALESCE(s.missing_file_owner_user_references, 0) AS missing_file_owner_user_references,
    COALESCE(s.missing_folder_owner_user_references, 0) AS missing_folder_owner_user_references,
    COALESCE(s.missing_share_creator_user_references, 0) AS missing_share_creator_user_references,
    COALESCE(s.malformed_share_target_references, 0) AS malformed_share_target_references,
    COALESCE(s.missing_file_references, 0) AS missing_file_references,
    COALESCE(s.missing_folder_references, 0) AS missing_folder_references
FROM user_groups g
LEFT JOIN membership_scope m ON m.group_id = g.id
LEFT JOIN group_share_scope s ON s.group_id = g.id
ORDER BY g.tenant_id, g.id;

ROLLBACK;
