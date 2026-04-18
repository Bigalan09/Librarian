pub mod confidence;
pub mod content;
pub mod embedding;
pub mod llm;
pub mod pipeline;
pub mod qdrant;

// Re-export key types for convenient access.
pub use confidence::{ConfidenceGate, GateResult};
pub use content::extract_content;
pub use embedding::cosine_similarity;
pub use llm::{LlmClassifier, LlmResult};
pub use pipeline::{ClassificationPipeline, ClassificationResult, EmbeddingCache};
pub use qdrant::{InMemoryVectorStore, SearchHit, VectorStore};
