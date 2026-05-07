-- Enable RLS on tenant-scoped tables
ALTER TABLE files ENABLE ROW LEVEL SECURITY;
ALTER TABLE folders ENABLE ROW LEVEL SECURITY;
ALTER TABLE file_versions ENABLE ROW LEVEL SECURITY;

-- Create policies for file isolation
CREATE POLICY files_owner_isolation ON files
    FOR ALL
    USING (owner_id = current_setting('app.current_user_id')::uuid);

-- Create policies for folder isolation
CREATE POLICY folders_owner_isolation ON folders
    FOR ALL
    USING (owner_id = current_setting('app.current_user_id')::uuid);

-- Create policies for file_version isolation (via parent file)
CREATE POLICY file_versions_owner_isolation ON file_versions
    FOR ALL
    USING (file_id IN (
        SELECT id FROM files WHERE owner_id = current_setting('app.current_user_id')::uuid
    ));

-- Note: Shares table is intentionally NOT covered by RLS because share access
-- is governed by the PermissionResolver, which may grant access to non-owners.
-- The shares table uses application-level permission checks.
