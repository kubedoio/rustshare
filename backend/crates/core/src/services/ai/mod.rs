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

// Re-exports used in subsequent Tasks 6-10; suppress unused-import warnings until then.
#[allow(unused_imports)]
pub use embedding::{EmbeddingGenerator, SimpleEmbeddingGenerator};
#[allow(unused_imports)]
pub use indexing::{
    can_access, validate_and_project, AclSearchFilter, ContentIndexer, EmbeddingPolicy,
    IndexAclProjection, IndexPrincipal, IndexVisibility, IndexedDocument, NoteAclPayload,
    RetrievalPrincipal,
};
#[allow(unused_imports)]
pub use vector_store::{InMemoryVectorStore, VectorStore};
