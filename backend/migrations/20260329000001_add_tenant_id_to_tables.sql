-- Add tenant_id column to support multi-tenant architecture

-- Users table
ALTER TABLE users ADD COLUMN tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
CREATE INDEX idx_users_tenant_id ON users(tenant_id);

-- Folders table  
ALTER TABLE folders ADD COLUMN tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
CREATE INDEX idx_folders_tenant_id ON folders(tenant_id);

-- Files table
ALTER TABLE files ADD COLUMN tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
CREATE INDEX idx_files_tenant_id ON files(tenant_id);

-- Shares table
ALTER TABLE shares ADD COLUMN tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
CREATE INDEX idx_shares_tenant_id ON shares(tenant_id);

-- File versions table
ALTER TABLE file_versions ADD COLUMN tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
CREATE INDEX idx_file_versions_tenant_id ON file_versions(tenant_id);

-- Notifications table
ALTER TABLE notifications ADD COLUMN tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
CREATE INDEX idx_notifications_tenant_id ON notifications(tenant_id);

-- User groups table
ALTER TABLE user_groups ADD COLUMN tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
CREATE INDEX idx_user_groups_tenant_id ON user_groups(tenant_id);

-- File thumbnails table
ALTER TABLE file_thumbnails ADD COLUMN tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
CREATE INDEX idx_file_thumbnails_tenant_id ON file_thumbnails(tenant_id);
