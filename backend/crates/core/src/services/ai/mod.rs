//! AI subsystem for RustShare
//!
//! This module provides AI-powered features including:
//! - Semantic search with permission filtering
//! - File summarization
//! - RAG-based Q&A with citations
//!
//! Contract A-01 through A-07: AI Safety
//! - All content is permission-filtered before returning
//! - All responses include source citations
//! - No hallucinations: only content from indexed files

pub mod embedding;
pub mod indexing;
pub mod vector_store;

pub use embedding::{EmbeddingGenerator, SimpleEmbeddingGenerator};
pub use indexing::{
    is_hidden_file_name, validate_and_project, ContentIndexer, EmbeddingPolicy, IndexAclProjection,
    IndexPrincipal, IndexVisibility, IndexedDocument, NoteAclPayload, RetrievalPrincipal,
};
pub use vector_store::{can_access, InMemoryVectorStore, VectorStore};
