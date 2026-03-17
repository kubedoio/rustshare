-- Files projection table
CREATE TABLE files (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    path TEXT NOT NULL,
    size BIGINT NOT NULL,
    mime_type VARCHAR(255) NOT NULL,
    content_hash VARCHAR(64) NOT NULL,
    storage_key VARCHAR(255) NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    parent_folder_id UUID REFERENCES folders(id) ON DELETE CASCADE,
    current_version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_files_owner ON files(owner_id);
CREATE INDEX idx_files_parent ON files(parent_folder_id);
CREATE INDEX idx_files_hash ON files(content_hash);

-- File versions table
CREATE TABLE file_versions (
    id UUID PRIMARY KEY,
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL,
    content_hash VARCHAR(64) NOT NULL,
    storage_key VARCHAR(255) NOT NULL,
    size BIGINT NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    change_description TEXT
);

-- Indexes
CREATE INDEX idx_file_versions_file ON file_versions(file_id, version_number DESC);
CREATE UNIQUE INDEX idx_file_versions_unique ON file_versions(file_id, version_number);
