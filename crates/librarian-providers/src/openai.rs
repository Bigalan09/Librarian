//! OpenAI provider client with rate limiting.
//!
//! Supports Bearer token authentication and basic token-bucket rate limiting.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tokio::time::Instant;
use tracing::{debug, warn};

use crate::traits::{ChatMessage, ChatResponse, ModelInfo, Provider};

/// Simple token-bucket rate limiter based on requests per minute.
#[derive(Debug)]
pub struct TokenBucket {
    rpm: u32,
    /// Timestamps of recent requests within the current window.
    timestamps: Mutex<Vec<Instant>>,
}

impl TokenBucket {
    /// Create a new token bucket with the given requests-per-minute limit.
    pub fn new(rpm: u32) -> Self {
        Self {
            rpm,
            timestamps: Mutex::new(Vec::new()),
        }
    }

    /// Wait until a request can be made without exceeding the rate limit.
    pub async fn acquire(&self) {
        loop {
            let now = Instant::now();
            let window = std::time::Duration::from_secs(60);

            {
                let mut ts = self.timestamps.lock().unwrap();
                // Remove timestamps older than 1 minute.
                ts.retain(|&t| now.duration_since(t) < window);

                if (ts.len() as u32) < self.rpm {
                    ts.push(now);
                    return;
                }

                // If at capacity, calculate how long to wait for the oldest to expire.
            }

            // Sleep briefly and retry.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

/// OpenAI API provider client.
#[derive(Debug)]
pub struct OpenAi {
    client: Client,
    base_url: String,
    api_key: String,
    llm_model: String,
    embed_model: String,
    rate_limiter: TokenBucket,
}

impl OpenAi {
    /// Create a new OpenAI client.
    ///
    /// - `api_key`: required API key for Bearer auth.
    /// - `llm_model`: defaults to `"gpt-4o-mini"`.
    /// - `embed_model`: defaults to `"text-embedding-3-small"`.
    /// - `rpm`: rate limit in requests per minute (default 20).
    pub fn new(
        api_key: &str,
        llm_model: Option<&str>,
        embed_model: Option<&str>,
        rpm: Option<u32>,
    ) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: api_key.to_string(),
            llm_model: llm_model.unwrap_or("gpt-4o-mini").to_string(),
            embed_model: embed_model.unwrap_or("text-embedding-3-small").to_string(),
            rate_limiter: TokenBucket::new(rpm.unwrap_or(20)),
        }
    }

    /// Create with custom base URL and client (for testing).
    #[cfg(test)]
    fn with_client(
        client: Client,
        base_url: &str,
        api_key: &str,
        llm_model: &str,
        embed_model: &str,
        rpm: u32,
    ) -> Self {
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            llm_model: llm_model.to_string(),
            embed_model: embed_model.to_string(),
            rate_limiter: TokenBucket::new(rpm),
        }
    }

    /// Send a request, retrying on 429 (rate limit) and 5xx (server error).
    ///
    /// Retries up to 2 times with exponential backoff (1s, 2s).
    async fn request_with_retry(
        &self,
        request: reqwest::RequestBuilder,
    ) -> anyhow::Result<reqwest::Response> {
        const MAX_RETRIES: u32 = 2;

        // Clone upfront for potential retries
        let mut current = request
            .try_clone()
            .ok_or_else(|| anyhow::anyhow!("Failed to clone request for retry"))?;
        let original = current
            .try_clone()
            .ok_or_else(|| anyhow::anyhow!("Failed to clone request for retry"))?;

        self.rate_limiter.acquire().await;
        let resp = {
            let r = original;
            r.send().await?
        };

        let status = resp.status();
        if !status.is_server_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Ok(resp);
        }

        // Retryable error - try up to MAX_RETRIES more times
        let mut last_resp = resp;
        for attempt in 1..=MAX_RETRIES {
            let delay = if last_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry_after = last_resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(1);
                warn!(
                    retry_after_secs = retry_after,
                    attempt, "Rate limited (429), retrying"
                );
                std::time::Duration::from_secs(retry_after)
            } else {
                let secs = 1u64 << (attempt - 1); // 1s, 2s
                warn!(status = %last_resp.status(), attempt, "Server error, retrying in {secs}s");
                std::time::Duration::from_secs(secs)
            };

            tokio::time::sleep(delay).await;
            self.rate_limiter.acquire().await;

            let next = current
                .try_clone()
                .ok_or_else(|| anyhow::anyhow!("Failed to clone request for retry"))?;
            last_resp = current.send().await?;
            current = next;

            let status = last_resp.status();
            if !status.is_server_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Ok(last_resp);
            }
        }

        Ok(last_resp)
    }
}

// --- API request/response types ---

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_completion_tokens: u32,
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

impl Provider for OpenAi {
    async fn validate(&self) -> anyhow::Result<ModelInfo> {
        debug!("Validating OpenAI connection");

        let url = format!("{}/models", self.base_url);
        let req = self.client.get(&url).bearer_auth(&self.api_key);

        let resp = self.request_with_retry(req).await?;
        let status = resp.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("OpenAI authentication failed (401): invalid API key");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI /models returned status {}: {}", status, text);
        }

        let models: ModelsResponse = resp.json().await?;
        let first = models
            .data
            .first()
            .ok_or_else(|| anyhow::anyhow!("No models available from OpenAI"))?;
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
        let start = std::time::Instant::now();
        debug!(model = %self.llm_model, "OpenAI chat request");

        let url = format!("{}/chat/completions", self.base_url);
        let body = ChatCompletionRequest {
            model: self.llm_model.clone(),
            messages,
            temperature,
            max_completion_tokens: max_tokens,
        };

        let req = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body);

        let resp = self.request_with_retry(req).await?;
        let status = resp.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("OpenAI authentication failed (401)");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI chat returned status {}: {}", status, text);
        }

        let completion: ChatCompletionResponse = resp.json().await?;
        let choice = completion
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No choices in OpenAI response"))?;

        debug!(
            elapsed_ms = start.elapsed().as_millis() as u64,
            "OpenAI chat complete"
        );
        Ok(ChatResponse {
            content: choice.message.content,
            model: completion.model,
        })
    }

    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        let start = std::time::Instant::now();
        debug!(model = %self.embed_model, count = texts.len(), "OpenAI embed request");

        let url = format!("{}/embeddings", self.base_url);
        let body = EmbeddingRequest {
            model: self.embed_model.clone(),
            input: texts,
        };

        let req = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body);

        let resp = self.request_with_retry(req).await?;
        let status = resp.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("OpenAI authentication failed (401)");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI embeddings returned status {}: {}", status, text);
        }

        let embedding_resp: EmbeddingResponse = resp.json().await?;
        debug!(
            elapsed_ms = start.elapsed().as_millis() as u64,
            "OpenAI embed complete"
        );
        Ok(embedding_resp
            .data
            .into_iter()
            .map(|d| d.embedding)
            .collect())
    }

    fn name(&self) -> &str {
        "openai"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chat_mock() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer test-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "choices": [{"message": {"role": "assistant", "content": "Hi from OpenAI"}}],
                    "model": "gpt-4o-mini"
                }"#,
            )
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
        );

        let msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        }];
        let resp = provider.chat(msgs, 0.7, 256).await.unwrap();
        assert_eq!(resp.content, "Hi from OpenAI");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn embed_mock() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .match_header("authorization", "Bearer test-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "data": [{"embedding": [0.1, 0.2], "index": 0}]
                }"#,
            )
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
        );

        let embeddings = provider.embed(vec!["test".to_string()]).await.unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0], vec![0.1, 0.2]);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn handles_401_unauthorized() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(401)
            .with_body(r#"{"error":{"message":"Invalid API key"}}"#)
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "bad-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
        );

        let msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }];
        let err = provider.chat(msgs, 0.7, 256).await.unwrap_err();
        assert!(err.to_string().contains("401"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn handles_429_rate_limit_with_retry() {
        let mut server = mockito::Server::new_async().await;

        // First call returns 429.
        let mock_429 = server
            .mock("POST", "/v1/chat/completions")
            .with_status(429)
            .with_header("retry-after", "1")
            .with_body(r#"{"error":{"message":"Rate limit exceeded"}}"#)
            .expect(1)
            .create_async()
            .await;

        // Second call (retry) returns 200.
        let mock_200 = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "choices": [{"message": {"role": "assistant", "content": "Retried OK"}}],
                    "model": "gpt-4o-mini"
                }"#,
            )
            .expect(1)
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
        );

        let msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }];
        let resp = provider.chat(msgs, 0.7, 256).await.unwrap();
        assert_eq!(resp.content, "Retried OK");
        mock_429.assert_async().await;
        mock_200.assert_async().await;
    }

    #[tokio::test]
    async fn handles_500_server_error_with_retries() {
        let mut server = mockito::Server::new_async().await;
        // Should receive 3 requests: 1 original + 2 retries
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(500)
            .with_body(r#"{"error":{"message":"Internal server error"}}"#)
            .expect(3)
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
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
    async fn retries_500_then_succeeds() {
        let mut server = mockito::Server::new_async().await;

        // First call returns 500
        let mock_500 = server
            .mock("POST", "/v1/chat/completions")
            .with_status(500)
            .with_body(r#"{"error":{"message":"Internal server error"}}"#)
            .expect(1)
            .create_async()
            .await;

        // Second call (retry) returns 200
        let mock_200 = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "choices": [{"message": {"role": "assistant", "content": "Recovered"}}],
                    "model": "gpt-4o-mini"
                }"#,
            )
            .expect(1)
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
        );

        let msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }];
        let resp = provider.chat(msgs, 0.7, 256).await.unwrap();
        assert_eq!(resp.content, "Recovered");
        mock_500.assert_async().await;
        mock_200.assert_async().await;
    }

    #[test]
    fn token_bucket_allows_within_limit() {
        let bucket = TokenBucket::new(10);
        // Should allow immediate access.
        let ts = bucket.timestamps.lock().unwrap();
        assert!(ts.is_empty());
    }

    #[tokio::test]
    async fn validate_mock() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .match_header("authorization", "Bearer test-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[{"id":"gpt-4o-mini","object":"model"}]}"#)
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
        );

        let info = provider.validate().await.unwrap();
        assert_eq!(info.id, "gpt-4o-mini");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn validate_401_unauthorized() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(401)
            .with_body(r#"{"error":{"message":"Invalid API key"}}"#)
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "bad-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
        );

        let err = provider.validate().await.unwrap_err();
        assert!(err.to_string().contains("401"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn embed_401_unauthorized() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .with_status(401)
            .with_body(r#"{"error":{"message":"Invalid API key"}}"#)
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "bad-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
        );

        let err = provider.embed(vec!["test".to_string()]).await.unwrap_err();
        assert!(err.to_string().contains("401"));
        mock.assert_async().await;
    }

    #[test]
    fn name_returns_openai() {
        let provider = OpenAi::new("test-key", None, None, None);
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn new_uses_defaults() {
        let provider = OpenAi::new("key", None, None, None);
        assert_eq!(provider.llm_model, "gpt-4o-mini");
        assert_eq!(provider.embed_model, "text-embedding-3-small");
        assert_eq!(provider.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn new_accepts_custom_models() {
        let provider = OpenAi::new(
            "key",
            Some("gpt-4"),
            Some("text-embedding-ada-002"),
            Some(100),
        );
        assert_eq!(provider.llm_model, "gpt-4");
        assert_eq!(provider.embed_model, "text-embedding-ada-002");
        assert_eq!(provider.rate_limiter.rpm, 100);
    }

    #[tokio::test]
    async fn token_bucket_acquire_succeeds_under_limit() {
        let bucket = TokenBucket::new(100);
        // Should acquire without blocking
        bucket.acquire().await;
        let ts = bucket.timestamps.lock().unwrap();
        assert_eq!(ts.len(), 1);
    }

    #[tokio::test]
    async fn chat_empty_choices_returns_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices":[],"model":"gpt-4o-mini"}"#)
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
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
    async fn validate_no_models_returns_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let provider = OpenAi::with_client(
            Client::new(),
            &format!("{}/v1", server.url()),
            "test-key",
            "gpt-4o-mini",
            "text-embedding-3-small",
            60,
        );

        let err = provider.validate().await.unwrap_err();
        assert!(err.to_string().contains("No models"));
        mock.assert_async().await;
    }
}
