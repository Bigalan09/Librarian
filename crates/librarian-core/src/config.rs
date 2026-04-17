//! Configuration loading and management.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Confidence thresholds per classification layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    /// Cosine similarity to accept filename embedding (default: 0.80).
    #[serde(default = "default_filename_embedding")]
    pub filename_embedding: f64,
    /// Cosine similarity to accept content embedding (default: 0.75).
    #[serde(default = "default_content_embedding")]
    pub content_embedding: f64,
    /// Self-reported LLM confidence to accept (default: 0.70).
    #[serde(default = "default_llm_confidence")]
    pub llm_confidence: f64,
}

fn default_filename_embedding() -> f64 {
    0.80
}
fn default_content_embedding() -> f64 {
    0.75
}
fn default_llm_confidence() -> f64 {
    0.70
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            filename_embedding: 0.80,
            content_embedding: 0.75,
            llm_confidence: 0.70,
        }
    }
}

/// AI provider type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    #[serde(alias = "lmstudio")]
    LmStudio,
    #[serde(alias = "openai")]
    OpenAi,
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::LmStudio
    }
}

/// AI provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default)]
    pub provider_type: ProviderType,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    pub api_key: Option<String>,
    pub llm_model: Option<String>,
    pub embed_model: Option<String>,
    pub rate_limit_rpm: Option<u32>,
}

fn default_base_url() -> String {
    "http://localhost:1234/v1".to_owned()
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_type: ProviderType::default(),
            base_url: default_base_url(),
            api_key: None,
            llm_model: None,
            embed_model: None,
            rate_limit_rpm: None,
        }
    }
}

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_inbox_folders")]
    pub inbox_folders: Vec<PathBuf>,
    #[serde(default = "default_destination_root")]
    pub destination_root: PathBuf,
    #[serde(default = "default_needs_review_path")]
    pub needs_review_path: PathBuf,
    #[serde(default = "default_trash_path")]
    pub trash_path: PathBuf,
    #[serde(default)]
    pub provider: ProviderConfig,
    #[serde(default)]
    pub thresholds: Thresholds,
    #[serde(default = "default_correction_window_days")]
    pub correction_window_days: u32,
    #[serde(default = "default_max_moves")]
    pub max_moves_per_run: u32,
    #[serde(default = "default_fewshot_count")]
    pub fewshot_count: u32,
    #[serde(default = "default_rule_suggestion_threshold")]
    pub rule_suggestion_threshold: u32,
}

fn default_inbox_folders() -> Vec<PathBuf> {
    let home = dirs_home();
    vec![home.join("Downloads"), home.join("Desktop")]
}

fn default_destination_root() -> PathBuf {
    dirs_home().join("Library-Managed")
}

fn default_needs_review_path() -> PathBuf {
    default_destination_root().join("NeedsReview")
}

fn default_trash_path() -> PathBuf {
    default_destination_root().join("_Trash")
}

fn default_correction_window_days() -> u32 {
    14
}
fn default_max_moves() -> u32 {
    500
}
fn default_fewshot_count() -> u32 {
    20
}
fn default_rule_suggestion_threshold() -> u32 {
    3
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            inbox_folders: default_inbox_folders(),
            destination_root: default_destination_root(),
            needs_review_path: default_needs_review_path(),
            trash_path: default_trash_path(),
            provider: ProviderConfig::default(),
            thresholds: Thresholds::default(),
            correction_window_days: 14,
            max_moves_per_run: 500,
            fewshot_count: 20,
            rule_suggestion_threshold: 3,
        }
    }
}

/// Path to the librarian home directory (~/.librarian).
pub fn librarian_home() -> PathBuf {
    dirs_home().join(".librarian")
}

/// Load configuration from a YAML file, merging with defaults.
pub fn load(path: &std::path::Path) -> anyhow::Result<AppConfig> {
    let contents = std::fs::read_to_string(path)?;
    let config: AppConfig = serde_yaml::from_str(&contents)?;
    Ok(config)
}

/// Load configuration from the default location (~/.librarian/config.yaml).
/// Returns defaults if the file does not exist.
pub fn load_default() -> anyhow::Result<AppConfig> {
    let path = librarian_home().join("config.yaml");
    if path.exists() {
        load(&path)
    } else {
        Ok(AppConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.correction_window_days, 14);
        assert_eq!(cfg.max_moves_per_run, 500);
        assert_eq!(cfg.fewshot_count, 20);
        assert_eq!(cfg.rule_suggestion_threshold, 3);
        assert_eq!(cfg.thresholds.filename_embedding, 0.80);
        assert_eq!(cfg.thresholds.content_embedding, 0.75);
        assert_eq!(cfg.thresholds.llm_confidence, 0.70);
        assert_eq!(cfg.provider.provider_type, ProviderType::LmStudio);
        assert_eq!(cfg.provider.base_url, "http://localhost:1234/v1");
    }

    #[test]
    fn parse_yaml_config() {
        let yaml = r#"
inbox_folders:
  - /tmp/inbox
destination_root: /tmp/managed
thresholds:
  filename_embedding: 0.90
  content_embedding: 0.85
  llm_confidence: 0.60
max_moves_per_run: 100
"#;
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.inbox_folders, vec![PathBuf::from("/tmp/inbox")]);
        assert_eq!(cfg.destination_root, PathBuf::from("/tmp/managed"));
        assert_eq!(cfg.thresholds.filename_embedding, 0.90);
        assert_eq!(cfg.max_moves_per_run, 100);
        // Defaults for unspecified fields
        assert_eq!(cfg.correction_window_days, 14);
    }

    #[test]
    fn parse_minimal_yaml_uses_defaults() {
        let yaml = "{}";
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.max_moves_per_run, 500);
        assert_eq!(cfg.thresholds.llm_confidence, 0.70);
    }

    #[test]
    fn load_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            "inbox_folders:\n  - /tmp/test\nmax_moves_per_run: 42\n",
        )
        .unwrap();

        let cfg = load(&path).unwrap();
        assert_eq!(cfg.inbox_folders, vec![PathBuf::from("/tmp/test")]);
        assert_eq!(cfg.max_moves_per_run, 42);
    }

    #[test]
    fn load_missing_file_errors() {
        let result = load(std::path::Path::new("/nonexistent/config.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn provider_type_deserialisation() {
        let yaml = "provider_type: openai";
        let pc: ProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(pc.provider_type, ProviderType::OpenAi);

        let yaml2 = "provider_type: lmstudio";
        let pc2: ProviderConfig = serde_yaml::from_str(yaml2).unwrap();
        assert_eq!(pc2.provider_type, ProviderType::LmStudio);
    }
}
