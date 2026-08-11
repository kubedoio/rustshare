-- At most one ACTIVE workspace↔community mapping per community_id globally.
-- A deactivated mapping frees the community for another tenant. This makes
-- the cross-tenant ambiguity in mapping_by_community unrepresentable.
--
-- If this migration fails on an existing deployment with duplicate active
-- mappings, the duplicates must be reconciled (deactivate all but one) before
-- applying.
CREATE UNIQUE INDEX chat_workspace_communities_active_community
    ON chat_workspace_communities (community_id) WHERE active;
