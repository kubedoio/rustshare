-- Enable pgvector. Safe to run if already enabled.
CREATE EXTENSION IF NOT EXISTS vector;

-- Dimension must match rustshare_core::services::ai::embedding::EMBEDDING_DIM (currently 768).
-- Store one row per indexed chunk. file_id is the chunk id for notes.
CREATE TABLE IF NOT EXISTS note_index_chunks (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    note_id uuid NOT NULL,
    source_file_id uuid NOT NULL,
    file_name text NOT NULL,
    file_path text NOT NULL,
    content text NOT NULL,
    mime_type text NOT NULL,
    owner_id uuid NOT NULL,
    embedding vector(768) NOT NULL,
    acl_hash text NOT NULL DEFAULT '',
    acl_version bigint NOT NULL DEFAULT 1,
    read_acl text[] NOT NULL DEFAULT '{}',
    visibility text NOT NULL DEFAULT 'private',
    embedding_policy text NOT NULL DEFAULT 'allowed',
    indexed_at timestamptz NOT NULL DEFAULT NOW(),

    CONSTRAINT note_index_chunks_positive_acl_version CHECK (acl_version > 0)
);

-- Fast tenant-scoped similarity search with ACL pre-filtering.
CREATE INDEX IF NOT EXISTS idx_note_index_chunks_tenant_note
    ON note_index_chunks(tenant_id, note_id);

CREATE INDEX IF NOT EXISTS idx_note_index_chunks_embedding
    ON note_index_chunks USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);
