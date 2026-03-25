-- Migration: Create file_thumbnails table
-- Created: 2026-03-24

CREATE TABLE file_thumbnails (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    size VARCHAR(10) NOT NULL CHECK (size IN ('sm', 'md', 'lg')),
    storage_path VARCHAR(500) NOT NULL,
    content_type VARCHAR(50) NOT NULL,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(file_id, size)
);

CREATE INDEX idx_file_thumbnails_file_id ON file_thumbnails(file_id);
