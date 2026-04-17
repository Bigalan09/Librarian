//! Provider trait definition.

use serde::{Deserialize, Serialize};

/// A chat message with role and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Response from a chat completion request.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
}

/// Basic model information returned by validation.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
}

/// Trait for AI providers (LM Studio, OpenAI, etc.).
///
/// Uses native `async fn` in traits (stable in Rust 2024 edition).
pub trait Provider: Send + Sync {
    /// Validate connectivity and return info about the configured model.
    fn validate(&self) -> impl std::future::Future<Output = anyhow::Result<ModelInfo>> + Send;

    /// Send a chat completion request.
    fn chat(
        &self,
        messages: Vec<ChatMessage>,
        temperature: f64,
        max_tokens: u32,
    ) -> impl std::future::Future<Output = anyhow::Result<ChatResponse>> + Send;

    /// Generate embeddings for the given texts.
    fn embed(
        &self,
        texts: Vec<String>,
    ) -> impl std::future::Future<Output = anyhow::Result<Vec<Vec<f32>>>> + Send;

    /// Human-readable provider name.
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_serializes() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn chat_message_deserializes() {
        let json = r#"{"role":"assistant","content":"Hi there"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Hi there");
    }
}
