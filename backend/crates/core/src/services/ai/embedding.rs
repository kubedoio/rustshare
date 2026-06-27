//! Embedding generation for semantic search.
//!
//! Phase 1.5: Simple TF-IDF based embeddings for lightweight operation.
//! Future phases can swap in OpenAI, Ollama, or other embedding models.

use std::collections::HashMap;

/// Size of the embedding vector.
/// For TF-IDF, this is the vocabulary hash space size.
pub const EMBEDDING_DIM: usize = 768;

/// A document embedding - vector representation of text.
pub type Embedding = Vec<f32>;

/// Trait for generating embeddings from text.
///
/// Implementations can range from simple TF-IDF to neural network-based embeddings.
/// The returned future is required to be `Send` so it can be used across await
/// points in multi-threaded runtimes and trait-object callbacks.
pub trait EmbeddingGenerator: Send + Sync {
    /// Generate an embedding for the given text.
    ///
    /// # Arguments
    /// * `text` - The text to embed
    ///
    /// # Returns
    /// A normalized embedding vector
    fn generate(&self, text: &str) -> impl std::future::Future<Output = Embedding> + Send;

    /// Compute cosine similarity between two embeddings.
    ///
    /// # Arguments
    /// * `a` - First embedding
    /// * `b` - Second embedding
    ///
    /// # Returns
    /// Similarity score in range [0.0, 1.0] where 1.0 is identical
    fn similarity(&self, a: &Embedding, b: &Embedding) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            (dot_product / (norm_a * norm_b)).clamp(0.0, 1.0)
        }
    }
}

/// Simple TF-IDF based embedding generator.
///
/// This is a lightweight implementation suitable for Phase 1.5:
/// - Uses hash-based term frequency
/// - No external API dependencies
/// - Fast and deterministic
pub struct SimpleEmbeddingGenerator {
    /// The dimensionality of embeddings
    dim: usize,
}

impl SimpleEmbeddingGenerator {
    /// Create a new embedding generator with the default dimension.
    pub fn new() -> Self {
        Self::with_dim(EMBEDDING_DIM)
    }

    /// Create a new embedding generator with a specific dimension.
    pub fn with_dim(dim: usize) -> Self {
        Self { dim }
    }

    /// Tokenize text into terms (simple word-based tokenization).
    fn tokenize(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty() && s.len() > 2)
            .map(|s| s.to_string())
            .collect()
    }

    /// Compute term frequencies for the given tokens.
    fn compute_term_frequencies(&self, tokens: &[String]) -> HashMap<String, f32> {
        let mut frequencies: HashMap<String, u32> = HashMap::new();
        let total_tokens = tokens.len() as f32;

        for token in tokens {
            *frequencies.entry(token.clone()).or_insert(0) += 1;
        }

        frequencies
            .into_iter()
            .map(|(term, count)| (term, count as f32 / total_tokens.max(1.0)))
            .collect()
    }

    /// Hash a term to an index in the embedding vector.
    fn hash_term(&self, term: &str) -> usize {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        term.hash(&mut hasher);
        (hasher.finish() as usize) % self.dim
    }
}

impl Default for SimpleEmbeddingGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingGenerator for SimpleEmbeddingGenerator {
    async fn generate(&self, text: &str) -> Embedding {
        let tokens = self.tokenize(text);
        let term_freqs = self.compute_term_frequencies(&tokens);

        let mut embedding = vec![0.0f32; self.dim];

        // Hash terms to vector positions and accumulate weighted values
        for (term, freq) in term_freqs {
            let idx = self.hash_term(&term);
            // Use a simple weighting: term frequency * (1 + log(term length))
            let weight = freq * (1.0 + (term.len() as f32).ln().max(0.0));
            embedding[idx] += weight;
        }

        // Normalize the embedding
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embedding {
                *val /= norm;
            }
        }

        embedding
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_generation() {
        let generator = SimpleEmbeddingGenerator::new();

        let embedding = generator.generate("hello world").await;

        assert_eq!(embedding.len(), EMBEDDING_DIM);
        // Check normalization (length should be close to 1.0)
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001 || norm == 0.0);
    }

    #[tokio::test]
    async fn test_similarity_identical() {
        let generator = SimpleEmbeddingGenerator::new();

        let text = "rust programming language";
        let emb1 = generator.generate(text).await;
        let emb2 = generator.generate(text).await;

        let similarity = generator.similarity(&emb1, &emb2);
        assert!((similarity - 1.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_similarity_different() {
        let generator = SimpleEmbeddingGenerator::new();

        let emb1 = generator
            .generate("machine learning artificial intelligence")
            .await;
        let emb2 = generator.generate("rust programming language").await;

        let similarity = generator.similarity(&emb1, &emb2);
        // Different topics should have lower similarity
        assert!(similarity < 0.9);
    }

    #[tokio::test]
    async fn test_similarity_related() {
        let generator = SimpleEmbeddingGenerator::new();

        // These are somewhat related topics
        let emb1 = generator
            .generate("artificial intelligence and machine learning")
            .await;
        let emb2 = generator
            .generate("neural networks and deep learning")
            .await;

        let similarity = generator.similarity(&emb1, &emb2);
        // Related topics should have some similarity
        assert!(similarity > 0.0);
    }

    #[tokio::test]
    async fn test_empty_text() {
        let generator = SimpleEmbeddingGenerator::new();

        let embedding = generator.generate("").await;

        assert_eq!(embedding.len(), EMBEDDING_DIM);
        // Empty text should produce all zeros
        assert!(embedding.iter().all(|&x| x == 0.0));
    }

    #[tokio::test]
    async fn test_tokenization() {
        let generator = SimpleEmbeddingGenerator::new();

        let tokens = generator.tokenize("Hello, World! This is a test.");

        assert_eq!(tokens, vec!["hello", "world", "this", "test"]);
    }
}
