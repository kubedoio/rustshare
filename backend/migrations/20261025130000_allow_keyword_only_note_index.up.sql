-- Keyword-only Notes rows have no embedding; NULL keeps them out of pgvector
-- similarity searches while retaining their body and ACL projection.
ALTER TABLE note_index_chunks
    ALTER COLUMN embedding DROP NOT NULL;
