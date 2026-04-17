//! Provider router for selecting the active AI provider.

use librarian_core::{ProviderConfig, ProviderType};
use tracing::info;

use crate::lmstudio::LmStudio;
use crate::openai::OpenAi;
use crate::traits::{ChatMessage, ChatResponse, ModelInfo, Provider};

/// Routes requests to the active provider based on configuration.
#[derive(Debug)]
pub struct ProviderRouter {
    lmstudio: Option<LmStudio>,
    openai: Option<OpenAi>,
    active_type: ProviderType,
}

impl ProviderRouter {
    /// Create a new router from provider configuration.
    ///
    /// Instantiates the provider indicated by `config.provider_type` and
    /// validates connectivity at startup.
    pub async fn new(config: &ProviderConfig) -> anyhow::Result<Self> {
        let mut router = Self {
            lmstudio: None,
            openai: None,
            active_type: config.provider_type,
        };

        match config.provider_type {
            ProviderType::LmStudio => {
                let provider = LmStudio::new(
                    Some(&config.base_url),
                    config.llm_model.as_deref(),
                    config.embed_model.as_deref(),
                );
                let model_info = Provider::validate(&provider).await?;
                info!(
                    provider = "lmstudio",
                    model = %model_info.id,
                    "Provider validated"
                );
                router.lmstudio = Some(provider);
            }
            ProviderType::OpenAi => {
                let api_key = config
                    .api_key
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("OpenAI provider requires an api_key"))?;
                let provider = OpenAi::new(
                    api_key,
                    config.llm_model.as_deref(),
                    config.embed_model.as_deref(),
                    config.rate_limit_rpm,
                );
                let model_info = Provider::validate(&provider).await?;
                info!(
                    provider = "openai",
                    model = %model_info.id,
                    "Provider validated"
                );
                router.openai = Some(provider);
            }
        }

        Ok(router)
    }

    /// Get a reference to the active provider.
    ///
    /// Returns an error if the active provider was not initialised (should not
    /// happen after successful `new()`).
    pub fn active(&self) -> anyhow::Result<&dyn ErasedProvider> {
        match self.active_type {
            ProviderType::LmStudio => self
                .lmstudio
                .as_ref()
                .map(|p| p as &dyn ErasedProvider)
                .ok_or_else(|| anyhow::anyhow!("LmStudio provider not initialised")),
            ProviderType::OpenAi => self
                .openai
                .as_ref()
                .map(|p| p as &dyn ErasedProvider)
                .ok_or_else(|| anyhow::anyhow!("OpenAi provider not initialised")),
        }
    }

    /// Return which provider type is active.
    pub fn active_type(&self) -> ProviderType {
        self.active_type
    }
}

/// Object-safe version of the Provider trait using boxed futures.
///
/// This exists because the `Provider` trait uses `impl Future` return types
/// (RPITIT), which are not object-safe. `ErasedProvider` wraps those into
/// boxed futures so we can use `&dyn ErasedProvider`.
#[allow(clippy::type_complexity)]
pub trait ErasedProvider: Send + Sync {
    fn validate(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<ModelInfo>> + Send + '_>>;
    fn chat(
        &self,
        messages: Vec<ChatMessage>,
        temperature: f64,
        max_tokens: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<ChatResponse>> + Send + '_>,
    >;
    fn embed(
        &self,
        texts: Vec<String>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<Vec<Vec<f32>>>> + Send + '_>,
    >;
    fn name(&self) -> &str;
}

impl<T: Provider> ErasedProvider for T {
    fn validate(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<ModelInfo>> + Send + '_>>
    {
        Box::pin(Provider::validate(self))
    }

    fn chat(
        &self,
        messages: Vec<ChatMessage>,
        temperature: f64,
        max_tokens: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<ChatResponse>> + Send + '_>,
    > {
        Box::pin(Provider::chat(self, messages, temperature, max_tokens))
    }

    fn embed(
        &self,
        texts: Vec<String>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<Vec<Vec<f32>>>> + Send + '_>,
    > {
        Box::pin(Provider::embed(self, texts))
    }

    fn name(&self) -> &str {
        Provider::name(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_type_default_is_lmstudio() {
        let config = ProviderConfig::default();
        assert_eq!(config.provider_type, ProviderType::LmStudio);
    }

    #[tokio::test]
    async fn openai_requires_api_key() {
        let config = ProviderConfig {
            provider_type: ProviderType::OpenAi,
            api_key: None,
            ..Default::default()
        };
        let result = ProviderRouter::new(&config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("api_key"));
    }

    #[tokio::test]
    async fn lmstudio_router_fails_on_unreachable() {
        let config = ProviderConfig {
            provider_type: ProviderType::LmStudio,
            base_url: "http://127.0.0.1:1/v1".to_string(),
            ..Default::default()
        };
        let result = ProviderRouter::new(&config).await;
        assert!(result.is_err());
    }
}
