pub mod cache;
pub mod lmstudio;
pub mod openai;
pub mod router;
pub mod sse;
pub mod traits;

// Re-export primary types for convenience.
pub use cache::EmbeddingCache;
pub use lmstudio::LmStudio;
pub use openai::OpenAi;
pub use router::{ErasedProvider, ProviderRouter};
pub use sse::{SseEvent, parse_sse_line};
pub use traits::{ChatMessage, ChatResponse, ModelInfo, Provider};
