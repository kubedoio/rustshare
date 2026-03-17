-- Folders projection table
CREATE TABLE folders (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    path TEXT NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    parent_folder_id UUID REFERENCES folders(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for path lookups
CREATE INDEX idx_folders_path ON folders(path);
CREATE INDEX idx_folders_owner ON folders(owner_id);
CREATE INDEX idx_folders_parent ON folders(parent_folder_id);

-- Unique constraint: name must be unique within parent folder for same owner
CREATE UNIQUE INDEX idx_folders_unique_name ON folders(owner_id, parent_folder_id, name);
