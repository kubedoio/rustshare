-- Fix RLS policies to not block queries when app.current_user_id is nil.
-- The before_acquire hook sets nil UUID as a safe default, but there is
-- no per-request middleware yet to override it with the real user ID.
-- These policies fall back to permissive mode when the session variable
-- is nil, relying on explicit owner_id filtering in application code.

DROP POLICY IF EXISTS files_owner_isolation ON files;
DROP POLICY IF EXISTS folders_owner_isolation ON folders;
DROP POLICY IF EXISTS file_versions_owner_isolation ON file_versions;

CREATE POLICY files_owner_isolation ON files
    FOR ALL
    USING (
        current_setting('app.current_user_id', true)::text IS NULL
        OR current_setting('app.current_user_id', true)::text = '00000000-0000-0000-0000-000000000000'
        OR owner_id = current_setting('app.current_user_id', true)::uuid
    );

CREATE POLICY folders_owner_isolation ON folders
    FOR ALL
    USING (
        current_setting('app.current_user_id', true)::text IS NULL
        OR current_setting('app.current_user_id', true)::text = '00000000-0000-0000-0000-000000000000'
        OR owner_id = current_setting('app.current_user_id', true)::uuid
    );

CREATE POLICY file_versions_owner_isolation ON file_versions
    FOR ALL
    USING (
        current_setting('app.current_user_id', true)::text IS NULL
        OR current_setting('app.current_user_id', true)::text = '00000000-0000-0000-0000-000000000000'
        OR file_id IN (
            SELECT id FROM files WHERE owner_id = current_setting('app.current_user_id', true)::uuid
        )
    );
