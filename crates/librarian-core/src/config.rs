//! Configuration loading and management.

use std::path::{Path, PathBuf};

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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    #[default]
    #[serde(alias = "lmstudio")]
    LmStudio,
    #[serde(alias = "openai")]
    OpenAi,
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
    /// Optional taxonomy of top-level categories to guide AI classification.
    ///
    /// Each entry is a category name (e.g. "Family") optionally followed by
    /// subcategories. When provided, the LLM is instructed to organise files
    /// into a 2-level hierarchy using these categories as guidance.
    ///
    /// Example YAML:
    /// ```yaml
    /// taxonomy:
    ///   Family:
    ///     - Health
    ///     - Education
    ///     - Records
    ///   Finance:
    ///     - Tax
    ///     - Insurance
    /// ```
    #[serde(default)]
    pub taxonomy: Taxonomy,
}

/// A suggested folder taxonomy for AI classification.
///
/// Maps top-level category names to optional lists of subcategories.
/// When empty, the LLM uses its own judgement for folder structure.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Taxonomy {
    pub categories: std::collections::BTreeMap<String, Vec<String>>,
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
            taxonomy: Taxonomy::default(),
        }
    }
}

impl Taxonomy {
    /// Whether a taxonomy has been configured.
    pub fn is_empty(&self) -> bool {
        self.categories.is_empty()
    }

    /// Format the taxonomy as a prompt-friendly string.
    ///
    /// Returns something like:
    /// ```text
    /// - Family: Health, Education, Records
    /// - Finance: Tax, Insurance
    /// - Home: Network, Maintenance
    /// ```
    pub fn to_prompt_string(&self) -> String {
        let mut out = String::new();
        for (category, subs) in &self.categories {
            if subs.is_empty() {
                out.push_str(&format!("- {category}\n"));
            } else {
                out.push_str(&format!("- {category}: {}\n", subs.join(", ")));
            }
        }
        out
    }
}

/// Path to the librarian home directory (~/.librarian).
pub fn librarian_home() -> PathBuf {
    dirs_home().join(".librarian")
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        dirs_home().join(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Validate that the config has sensible values.
///
/// Returns a list of warnings (non-fatal) and errors (fatal).
/// Call this after loading to catch misconfiguration early.
pub fn validate(config: &AppConfig) -> Result<Vec<String>, Vec<String>> {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    // Thresholds must be in [0.0, 1.0]
    let check_threshold = |name: &str, val: f64, errors: &mut Vec<String>| {
        if !(0.0..=1.0).contains(&val) {
            errors.push(format!(
                "{name} threshold must be between 0.0 and 1.0, got {val}"
            ));
        }
    };
    check_threshold(
        "filename_embedding",
        config.thresholds.filename_embedding,
        &mut errors,
    );
    check_threshold(
        "content_embedding",
        config.thresholds.content_embedding,
        &mut errors,
    );
    check_threshold(
        "llm_confidence",
        config.thresholds.llm_confidence,
        &mut errors,
    );

    if config.max_moves_per_run == 0 {
        errors.push("max_moves_per_run must be greater than 0".to_string());
    }

    // Check inbox folders exist
    for folder in &config.inbox_folders {
        if !folder.exists() {
            warnings.push(format!("inbox folder does not exist: {}", folder.display()));
        }
    }

    // Check destination root is writable (by checking parent exists)
    if !config.destination_root.exists()
        && let Some(parent) = config.destination_root.parent()
        && !parent.exists()
    {
        warnings.push(format!(
            "destination root parent does not exist: {}",
            parent.display()
        ));
    }

    if errors.is_empty() {
        Ok(warnings)
    } else {
        Err(errors)
    }
}

/// Load configuration from a YAML file, merging with defaults.
/// Expands `~` in all path fields to the user's home directory.
pub fn load(path: &std::path::Path) -> anyhow::Result<AppConfig> {
    let contents = std::fs::read_to_string(path)?;
    let mut config: AppConfig = serde_yaml::from_str(&contents)?;

    // Expand tildes in all path fields
    config.inbox_folders = config
        .inbox_folders
        .iter()
        .map(|p| expand_tilde(p))
        .collect();
    config.destination_root = expand_tilde(&config.destination_root);
    config.needs_review_path = expand_tilde(&config.needs_review_path);
    config.trash_path = expand_tilde(&config.trash_path);

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
    fn validate_default_config_passes() {
        let cfg = AppConfig {
            inbox_folders: vec![],
            ..Default::default()
        };
        let result = validate(&cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_bad_thresholds() {
        let mut cfg = AppConfig {
            inbox_folders: vec![],
            ..Default::default()
        };
        cfg.thresholds.filename_embedding = 1.5;
        let result = validate(&cfg);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors[0].contains("filename_embedding"));
    }

    #[test]
    fn validate_rejects_zero_max_moves() {
        let cfg = AppConfig {
            inbox_folders: vec![],
            max_moves_per_run: 0,
            ..Default::default()
        };
        let result = validate(&cfg);
        assert!(result.is_err());
    }

    #[test]
    fn load_missing_file_errors() {
        let result = load(std::path::Path::new("/nonexistent/config.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn expand_tilde_with_tilde() {
        let expanded = expand_tilde(Path::new("~/Documents/file.txt"));
        assert!(!expanded.starts_with("~"));
        assert!(expanded.to_string_lossy().contains("Documents/file.txt"));
    }

    #[test]
    fn expand_tilde_without_tilde() {
        let path = Path::new("/absolute/path/file.txt");
        let expanded = expand_tilde(path);
        assert_eq!(expanded, path);
    }

    #[test]
    fn validate_warns_for_nonexistent_inbox() {
        let cfg = AppConfig {
            inbox_folders: vec![PathBuf::from("/nonexistent_librarian_test_path/inbox")],
            ..Default::default()
        };
        let result = validate(&cfg);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("inbox folder does not exist"))
        );
    }

    #[test]
    fn validate_warns_for_nonexistent_dest_parent() {
        let cfg = AppConfig {
            inbox_folders: vec![],
            destination_root: PathBuf::from("/nonexistent_librarian_test/deeply/nested/root"),
            ..Default::default()
        };
        let result = validate(&cfg);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("destination root parent does not exist"))
        );
    }

    #[test]
    fn validate_multiple_bad_thresholds() {
        let mut cfg = AppConfig {
            inbox_folders: vec![],
            ..Default::default()
        };
        cfg.thresholds.filename_embedding = -0.5;
        cfg.thresholds.content_embedding = 2.0;
        cfg.thresholds.llm_confidence = 1.5;
        let result = validate(&cfg);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn load_from_file_expands_tildes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            "inbox_folders:\n  - ~/Downloads\ndestination_root: ~/Managed\n",
        )
        .unwrap();

        let cfg = load(&path).unwrap();
        assert!(
            !cfg.inbox_folders[0].starts_with("~"),
            "tilde should be expanded"
        );
        assert!(
            !cfg.destination_root.starts_with("~"),
            "tilde should be expanded"
        );
    }

    #[test]
    fn taxonomy_from_yaml() {
        let yaml = r#"
taxonomy:
  Family:
    - Health
    - Education
    - Records
  Finance:
    - Tax
    - Insurance
  Home: []
"#;
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!cfg.taxonomy.is_empty());
        assert_eq!(cfg.taxonomy.categories.len(), 3);
        assert_eq!(
            cfg.taxonomy.categories["Family"],
            vec!["Health", "Education", "Records"]
        );
        assert!(cfg.taxonomy.categories["Home"].is_empty());

        let prompt = cfg.taxonomy.to_prompt_string();
        assert!(prompt.contains("Family: Health, Education, Records"));
        assert!(prompt.contains("Finance: Tax, Insurance"));
        assert!(prompt.contains("- Home"));
    }

    #[test]
    fn taxonomy_defaults_to_empty() {
        let yaml = "{}";
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.taxonomy.is_empty());
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
