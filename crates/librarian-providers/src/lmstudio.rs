//! LM Studio provider client.
//!
//! Connects to a local LM Studio instance via its OpenAI-compatible API.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::traits::{ChatMessage, ChatResponse, ModelInfo, Provider};

/// LM Studio provider client.
#[derive(Debug, Clone)]
pub struct LmStudio {
    client: Client,
    base_url: String,
    llm_model: String,
    embed_model: String,
}

impl LmStudio {
    /// Create a new LM Studio client.
    ///
    /// - `base_url`: defaults to `http://localhost:1234/v1` if `None`.
    /// - `llm_model`: the model ID for chat completions.
    /// - `embed_model`: the model ID for embeddings.
    pub fn new(base_url: Option<&str>, llm_model: Option<&str>, embed_model: Option<&str>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            base_url: base_url
                .unwrap_or("http://localhost:1234/v1")
                .trim_end_matches('/')
                .to_string(),
            llm_model: llm_model.unwrap_or("default").to_string(),
            embed_model: embed_model.unwrap_or("default").to_string(),
        }
    }

    /// Create with an explicit reqwest client (useful for testing).
    #[cfg(test)]
    fn with_client(client: Client, base_url: &str, llm_model: &str, embed_model: &str) -> Self {
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            llm_model: llm_model.to_string(),
            embed_model: embed_model.to_string(),
        }
    }
}

// --- API request/response types ---

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    model: String,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelData>,
}

#[derive(Deserialize)]
struct ModelData {
    id: String,
}

impl Provider for LmStudio {
    async fn validate(&self) -> anyhow::Result<ModelInfo> {
        debug!(base_url = %self.base_url, "Validating LM Studio connection");

        let url = format!("{}/models", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("LM Studio /models returned status {}", status);
        }
        let models: ModelsResponse = resp.json().await?;
        let first = models
            .data
            .first()
            .ok_or_else(|| anyhow::anyhow!("No models loaded in LM Studio"))?;
        Ok(ModelInfo {
            id: first.id.clone(),
        })
    }

    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        temperature: f64,
        max_tokens: u32,
    ) -> anyhow::Result<ChatResponse> {
        debug!(model = %self.llm_model, "LM Studio chat request");

        let url = format!("{}/chat/completions", self.base_url);
        let body = ChatCompletionRequest {
            model: self.llm_model.clone(),
            messages,
            temperature,
            max_tokens,
        };

        let resp = self.client.post(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("LM Studio chat returned status {}: {}", status, text);
        }

        let completion: ChatCompletionResponse = resp.json().await?;
        let choice = completion
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No choices in LM Studio response"))?;

        Ok(ChatResponse {
            content: choice.message.content,
            model: completion.model,
        })
    }

    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        debug!(model = %self.embed_model, count = texts.len(), "LM Studio embed request");

        let url = format!("{}/embeddings", self.base_url);
        let body = EmbeddingRequest {
            model: self.embed_model.clone(),
            input: texts,
        };

        let resp = self.client.post(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("LM Studio embeddings returned status {}: {}", status, text);
        }

        let embedding_resp: EmbeddingResponse = resp.json().await?;
        Ok(embedding_resp
            .data
            .into_iter()
            .map(|d| d.embedding)
            .collect())
    }

    fn name(&self) -> &str {
        "lmstudio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validate_mock() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[{"id":"test-model","object":"model"}]}"#)
            .create_async()
            .await;

        let provider = LmStudio::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-model",
            "test-embed",
        );

        let info = provider.validate().await.unwrap();
        assert_eq!(info.id, "test-model");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn chat_mock() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "choices": [{"message": {"role": "assistant", "content": "Hello there!"}}],
                    "model": "test-model"
                }"#,
            )
            .create_async()
            .await;

        let provider = LmStudio::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-model",
            "test-embed",
        );

        let msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }];
        let resp = provider.chat(msgs, 0.7, 256).await.unwrap();
        assert_eq!(resp.content, "Hello there!");
        assert_eq!(resp.model, "test-model");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn embed_mock() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "data": [
                        {"embedding": [0.1, 0.2, 0.3], "index": 0},
                        {"embedding": [0.4, 0.5, 0.6], "index": 1}
                    ]
                }"#,
            )
            .create_async()
            .await;

        let provider = LmStudio::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-model",
            "test-embed",
        );

        let embeddings = provider
            .embed(vec!["hello".to_string(), "world".to_string()])
            .await
            .unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0], vec![0.1, 0.2, 0.3]);
        assert_eq!(embeddings[1], vec![0.4, 0.5, 0.6]);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn connection_refused_error() {
        // Use an address that should refuse connections.
        let provider = LmStudio::new(Some("http://127.0.0.1:1"), Some("model"), Some("embed"));
        let result = provider.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn validate_returns_error_on_non_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let provider = LmStudio::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "model",
            "embed",
        );

        let err = provider.validate().await.unwrap_err();
        assert!(err.to_string().contains("500"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn validate_no_models_loaded() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let provider = LmStudio::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "model",
            "embed",
        );

        let err = provider.validate().await.unwrap_err();
        assert!(err.to_string().contains("No models loaded"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn chat_returns_error_on_non_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(500)
            .with_body(r#"{"error":"server error"}"#)
            .create_async()
            .await;

        let provider = LmStudio::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "model",
            "embed",
        );

        let msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }];
        let err = provider.chat(msgs, 0.7, 256).await.unwrap_err();
        assert!(err.to_string().contains("500"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn chat_empty_choices_returns_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices":[],"model":"test"}"#)
            .create_async()
            .await;

        let provider = LmStudio::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "model",
            "embed",
        );

        let msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }];
        let err = provider.chat(msgs, 0.7, 256).await.unwrap_err();
        assert!(err.to_string().contains("No choices"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn embed_returns_error_on_non_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .with_status(400)
            .with_body(r#"{"error":"bad request"}"#)
            .create_async()
            .await;

        let provider = LmStudio::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "model",
            "embed",
        );

        let err = provider.embed(vec!["test".to_string()]).await.unwrap_err();
        assert!(err.to_string().contains("400"));
        mock.assert_async().await;
    }

    #[test]
    fn name_returns_lmstudio() {
        let provider = LmStudio::new(None, None, None);
        assert_eq!(provider.name(), "lmstudio");
    }

    #[test]
    fn new_uses_defaults() {
        let provider = LmStudio::new(None, None, None);
        assert_eq!(provider.base_url, "http://localhost:1234/v1");
        assert_eq!(provider.llm_model, "default");
        assert_eq!(provider.embed_model, "default");
    }

    #[test]
    fn new_trims_trailing_slash() {
        let provider = LmStudio::new(Some("http://localhost:1234/v1/"), None, None);
        assert_eq!(provider.base_url, "http://localhost:1234/v1");
    }

    #[test]
    fn new_accepts_custom_values() {
        let provider = LmStudio::new(
            Some("http://myhost:5000/v1"),
            Some("llama-3"),
            Some("nomic-embed"),
        );
        assert_eq!(provider.base_url, "http://myhost:5000/v1");
        assert_eq!(provider.llm_model, "llama-3");
        assert_eq!(provider.embed_model, "nomic-embed");
    }
}
