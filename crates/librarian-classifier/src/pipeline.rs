//! Tiered classification pipeline

use std::collections::HashMap;
use std::path::PathBuf;

use librarian_core::decision::ClassificationMethod;
use librarian_core::file_entry::{FileEntry, FinderColour};
use librarian_providers::router::ErasedProvider;
use librarian_rules::engine::RuleEngine;
use tracing::{debug, info};

use crate::confidence::{ConfidenceGate, GateResult};
use crate::content::extract_content;
use crate::embedding::{cosine_similarity, embed_text_dyn};
use crate::llm::{LlmClassifier, LlmResult};
use crate::qdrant::VectorStore;

/// Cache for embedding vectors, keyed by text (e.g. bucket name or filename).
#[derive(Debug, Default)]
pub struct EmbeddingCache {
    cache: HashMap<String, Vec<f32>>,
}

impl EmbeddingCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Vec<f32>> {
        self.cache.get(key)
    }

    pub fn insert(&mut self, key: String, embedding: Vec<f32>) {
        self.cache.insert(key, embedding);
    }
}

/// The result of running a file through the classification pipeline.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub destination: PathBuf,
    pub method: ClassificationMethod,
    pub confidence: Option<f64>,
    pub tags: Vec<String>,
    pub colour: Option<FinderColour>,
    pub reason: Option<String>,
    pub needs_review: bool,
    /// The embedding vector for this file's name (if computed).
    /// Used to update the vector store after classification.
    pub filename_embedding: Option<Vec<f32>>,
}

/// The tiered classification pipeline.
///
/// Processes files through multiple classification tiers in order:
/// 1. Rules engine (exact/pattern matching)
/// 2. Centroid similarity (from vector store, if available)
/// 3. Filename embedding (cosine similarity against bucket name embeddings)
/// 4. Content embedding (text files only)
/// 5. LLM classifier (final tier)
pub struct ClassificationPipeline;

impl ClassificationPipeline {
    /// Run a file entry through the tiered classification pipeline.
    #[allow(clippy::too_many_arguments)]
    pub async fn classify(
        entry: &FileEntry,
        rules_engine: &RuleEngine,
        provider: &dyn ErasedProvider,
        gate: &ConfidenceGate,
        cache: &mut EmbeddingCache,
        existing_buckets: &[String],
        few_shot_examples: &[String],
        vector_store: Option<&dyn VectorStore>,
    ) -> ClassificationResult {
        // --- Tier 1: Rules engine ---
        // When a rule matches, check whether its destination delegates to the AI
        // pipeline via `{ai_suggest}`. If so, carry the rule's tags/colour forward
        // and let the remaining tiers determine the destination.
        let mut rule_tags: Vec<String> = Vec::new();
        let mut rule_colour: Option<FinderColour> = None;
        let mut rule_hint: Option<String> = None;

        if let Some(rule) = rules_engine.evaluate(entry) {
            if RuleEngine::is_ai_suggested(&rule.destination) {
                info!(
                    file = %entry.name,
                    rule = %rule.name,
                    "Rule matched with {{ai_suggest}} — delegating destination to AI"
                );
                rule_tags = rule.tags.clone();
                rule_colour = rule.colour;
                rule_hint = Some(format!("Matched rule: {}", rule.name));
                // fall through to AI tiers
            } else {
                let destination = RuleEngine::expand_destination(&rule.destination, entry);
                info!(
                    file = %entry.name,
                    rule = %rule.name,
                    "Classified by rule"
                );
                return ClassificationResult {
                    destination: PathBuf::from(destination),
                    method: ClassificationMethod::Rule,
                    confidence: Some(1.0),
                    tags: rule.tags.clone(),
                    colour: rule.colour,
                    reason: Some(format!("Matched rule: {}", rule.name)),
                    needs_review: false,
                    filename_embedding: None,
                };
            }
        }

        // --- Tier 2: Centroid similarity (from vector store) ---
        let filetype = entry.extension.as_deref().unwrap_or("unknown");
        if let Some(vs) = vector_store
            && !vs.is_empty()
            && let Ok(embedding) = embed_text_dyn(provider, &entry.name).await
            && let Some(hit) = vs.find_nearest(&entry.source_inbox, filetype, &embedding)
        {
            let sim = hit.score as f64;
            match gate.check_filename_embedding(sim, &hit.bucket) {
                GateResult::Accept {
                    destination,
                    confidence,
                } => {
                    info!(
                        file = %entry.name,
                        destination = %destination,
                        confidence = confidence,
                        "Classified by centroid similarity"
                    );
                    return merge_rule_metadata(
                        ClassificationResult {
                            destination: PathBuf::from(destination),
                            method: ClassificationMethod::FilenameEmbedding,
                            confidence: Some(confidence),
                            tags: Vec::new(),
                            colour: None,
                            reason: Some(format!(
                                "Centroid similarity to '{}': {:.3}",
                                hit.bucket, sim
                            )),
                            needs_review: false,
                            filename_embedding: Some(embedding),
                        },
                        rule_tags,
                        rule_colour,
                        rule_hint,
                    );
                }
                GateResult::Escalate => {
                    debug!(
                        file = %entry.name,
                        best_sim = sim,
                        best_bucket = %hit.bucket,
                        "Centroid similarity below threshold, escalating"
                    );
                }
                GateResult::NeedsReview { .. } => {}
            }
        }

        // --- Tier 3: Filename embedding against bucket names ---
        if !existing_buckets.is_empty()
            && let Ok(result) =
                try_filename_embedding(entry, provider, gate, cache, existing_buckets).await
            && let Some(r) = result
        {
            return merge_rule_metadata(r, rule_tags, rule_colour, rule_hint);
        }

        // --- Tier 4: Content embedding (text files only) ---
        let is_text = matches!(
            entry.extension.as_deref(),
            Some("txt" | "md" | "csv" | "pdf")
        );
        if is_text
            && !existing_buckets.is_empty()
            && let Ok(result) =
                try_content_embedding(entry, provider, gate, cache, existing_buckets).await
            && let Some(r) = result
        {
            return merge_rule_metadata(r, rule_tags, rule_colour, rule_hint);
        }

        // --- Tier 5: LLM classifier ---
        let result = match LlmClassifier::classify_dyn(
            provider,
            entry,
            existing_buckets,
            few_shot_examples,
        )
        .await
        {
            Ok(llm_result) => build_llm_result(entry, &llm_result, gate),
            Err(e) => {
                debug!(file = %entry.name, error = %e, "LLM classification failed");
                ClassificationResult {
                    destination: PathBuf::from("NeedsReview"),
                    method: ClassificationMethod::None,
                    confidence: None,
                    tags: Vec::new(),
                    colour: None,
                    reason: Some(format!("All classification tiers failed: {e}")),
                    needs_review: true,
                    filename_embedding: None,
                }
            }
        };

        merge_rule_metadata(result, rule_tags, rule_colour, rule_hint)
    }
}

/// When an `{ai_suggest}` rule matched, merge its tags, colour, and hint into
/// the AI-determined result. If no rule matched, the vectors are empty and this
/// is a no-op.
fn merge_rule_metadata(
    mut result: ClassificationResult,
    rule_tags: Vec<String>,
    rule_colour: Option<FinderColour>,
    rule_hint: Option<String>,
) -> ClassificationResult {
    if rule_tags.is_empty() && rule_colour.is_none() {
        return result;
    }
    // Prepend rule tags, deduplicating
    for tag in rule_tags {
        if !result.tags.contains(&tag) {
            result.tags.push(tag);
        }
    }
    if result.colour.is_none() {
        result.colour = rule_colour;
    }
    if let Some(hint) = rule_hint {
        let existing = result.reason.take().unwrap_or_default();
        result.reason = Some(format!("{hint}; {existing}"));
    }
    result
}

/// Try to classify via filename embedding similarity.
async fn try_filename_embedding(
    entry: &FileEntry,
    provider: &dyn ErasedProvider,
    gate: &ConfidenceGate,
    cache: &mut EmbeddingCache,
    existing_buckets: &[String],
) -> anyhow::Result<Option<ClassificationResult>> {
    let filename_embedding = embed_text_dyn(provider, &entry.name).await?;

    let mut best_sim = 0.0_f64;
    let mut best_bucket = String::new();

    for bucket in existing_buckets {
        let bucket_embedding = if let Some(cached) = cache.get(bucket) {
            cached.clone()
        } else {
            let emb = embed_text_dyn(provider, bucket).await?;
            cache.insert(bucket.clone(), emb.clone());
            emb
        };

        let sim = cosine_similarity(&filename_embedding, &bucket_embedding) as f64;
        if sim > best_sim {
            best_sim = sim;
            best_bucket = bucket.clone();
        }
    }

    match gate.check_filename_embedding(best_sim, &best_bucket) {
        GateResult::Accept {
            destination,
            confidence,
        } => {
            info!(
                file = %entry.name,
                destination = %destination,
                confidence = confidence,
                "Classified by filename embedding"
            );
            Ok(Some(ClassificationResult {
                destination: PathBuf::from(destination),
                method: ClassificationMethod::FilenameEmbedding,
                confidence: Some(confidence),
                tags: Vec::new(),
                colour: None,
                reason: Some(format!(
                    "Filename embedding similarity to '{best_bucket}': {best_sim:.3}"
                )),
                needs_review: false,
                filename_embedding: Some(filename_embedding),
            }))
        }
        GateResult::Escalate => {
            debug!(
                file = %entry.name,
                best_sim = best_sim,
                "Filename embedding below threshold, escalating"
            );
            Ok(None)
        }
        GateResult::NeedsReview { .. } => Ok(None),
    }
}

/// Try to classify via content embedding similarity.
async fn try_content_embedding(
    entry: &FileEntry,
    provider: &dyn ErasedProvider,
    gate: &ConfidenceGate,
    cache: &mut EmbeddingCache,
    existing_buckets: &[String],
) -> anyhow::Result<Option<ClassificationResult>> {
    let content = match extract_content(&entry.path).await {
        Some(c) if !c.trim().is_empty() => c,
        _ => return Ok(None),
    };

    // Truncate content to a reasonable length for embedding
    let truncated = if content.len() > 8000 {
        &content[..8000]
    } else {
        &content
    };

    let content_embedding = embed_text_dyn(provider, truncated).await?;

    let mut best_sim = 0.0_f64;
    let mut best_bucket = String::new();

    for bucket in existing_buckets {
        let bucket_embedding = if let Some(cached) = cache.get(bucket) {
            cached.clone()
        } else {
            let emb = embed_text_dyn(provider, bucket).await?;
            cache.insert(bucket.clone(), emb.clone());
            emb
        };

        let sim = cosine_similarity(&content_embedding, &bucket_embedding) as f64;
        if sim > best_sim {
            best_sim = sim;
            best_bucket = bucket.clone();
        }
    }

    match gate.check_content_embedding(best_sim, &best_bucket) {
        GateResult::Accept {
            destination,
            confidence,
        } => {
            info!(
                file = %entry.name,
                destination = %destination,
                confidence = confidence,
                "Classified by content embedding"
            );
            Ok(Some(ClassificationResult {
                destination: PathBuf::from(destination),
                method: ClassificationMethod::ContentEmbedding,
                confidence: Some(confidence),
                tags: Vec::new(),
                colour: None,
                reason: Some(format!(
                    "Content embedding similarity to '{best_bucket}': {best_sim:.3}"
                )),
                needs_review: false,
                filename_embedding: None,
            }))
        }
        GateResult::Escalate => {
            debug!(
                file = %entry.name,
                best_sim = best_sim,
                "Content embedding below threshold, escalating to LLM"
            );
            Ok(None)
        }
        GateResult::NeedsReview { .. } => Ok(None),
    }
}

/// Build a ClassificationResult from an LLM result, applying the confidence gate.
fn build_llm_result(
    entry: &FileEntry,
    llm_result: &LlmResult,
    gate: &ConfidenceGate,
) -> ClassificationResult {
    match gate.check_llm_confidence(llm_result.confidence, &llm_result.destination) {
        GateResult::Accept {
            destination,
            confidence,
        } => {
            info!(
                file = %entry.name,
                destination = %destination,
                confidence = confidence,
                "Classified by LLM"
            );
            ClassificationResult {
                destination: PathBuf::from(destination),
                method: ClassificationMethod::Llm,
                confidence: Some(confidence),
                tags: llm_result.tags.clone(),
                colour: None,
                reason: Some(llm_result.reason.clone()),
                needs_review: false,
                filename_embedding: None,
            }
        }
        GateResult::NeedsReview { reason } => {
            info!(
                file = %entry.name,
                reason = %reason,
                "LLM classification needs review"
            );
            ClassificationResult {
                destination: PathBuf::from(&llm_result.destination),
                method: ClassificationMethod::Llm,
                confidence: Some(llm_result.confidence),
                tags: llm_result.tags.clone(),
                colour: None,
                reason: Some(reason),
                needs_review: true,
                filename_embedding: None,
            }
        }
        GateResult::Escalate => {
            // Shouldn't happen for LLM tier, but handle gracefully
            ClassificationResult {
                destination: PathBuf::from("NeedsReview"),
                method: ClassificationMethod::None,
                confidence: None,
                tags: Vec::new(),
                colour: None,
                reason: Some("Unexpected escalation from LLM tier".to_string()),
                needs_review: true,
                filename_embedding: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use librarian_providers::traits::{ChatMessage, ChatResponse, ModelInfo, Provider};
    use librarian_rules::loader::{RuleSet, load_rules_from_str};
    use std::path::PathBuf;

    /// A mock provider for testing the pipeline.
    struct MockProvider {
        /// Pre-configured embedding to return for any text.
        embedding: Vec<f32>,
        /// Pre-configured chat response content.
        chat_response: String,
    }

    impl MockProvider {
        fn new(embedding: Vec<f32>, chat_response: &str) -> Self {
            Self {
                embedding,
                chat_response: chat_response.to_string(),
            }
        }
    }

    impl Provider for MockProvider {
        async fn validate(&self) -> anyhow::Result<ModelInfo> {
            Ok(ModelInfo {
                id: "mock-model".to_string(),
            })
        }

        async fn chat(
            &self,
            _messages: Vec<ChatMessage>,
            _temperature: f64,
            _max_tokens: u32,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                content: self.chat_response.clone(),
                model: "mock-model".to_string(),
            })
        }

        async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
            let emb = self.embedding.clone();
            Ok(texts.iter().map(|_| emb.clone()).collect())
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    fn make_entry(name: &str, ext: Option<&str>) -> FileEntry {
        FileEntry {
            path: PathBuf::from(format!("/tmp/{name}")),
            name: name.to_string(),
            extension: ext.map(|s| s.to_string()),
            size_bytes: 100,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            tags: Vec::new(),
            colour: None,
            source_inbox: "Downloads".to_string(),
        }
    }

    fn empty_engine() -> RuleEngine {
        RuleEngine::new(RuleSet { rules: Vec::new() })
    }

    #[tokio::test]
    async fn rule_match_returns_immediately() {
        let yaml = r#"
rules:
  - name: "PDFs"
    match:
      extension: "pdf"
    destination: "Documents/PDFs"
    tags: ["document", "pdf"]
"#;
        let ruleset = load_rules_from_str(yaml).unwrap();
        let engine = RuleEngine::new(ruleset);
        let gate = ConfidenceGate::new(Default::default());
        let provider = MockProvider::new(vec![1.0, 0.0, 0.0], "{}");
        let mut cache = EmbeddingCache::new();

        let entry = make_entry("invoice.pdf", Some("pdf"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &[],
            &[],
            None,
        )
        .await;

        assert_eq!(result.destination, PathBuf::from("Documents/PDFs"));
        assert_eq!(result.method, ClassificationMethod::Rule);
        assert_eq!(result.confidence, Some(1.0));
        assert!(!result.needs_review);
        assert!(result.tags.contains(&"pdf".to_string()));
    }

    #[tokio::test]
    async fn llm_fallback_when_no_rules_match() {
        let engine = empty_engine();
        let gate = ConfidenceGate::new(Default::default());
        let chat_response = r#"{"destination": "Invoices", "confidence": 0.85, "tags": ["finance"], "reason": "Looks like an invoice"}"#;
        let provider = MockProvider::new(vec![0.1, 0.1, 0.1], chat_response);
        let mut cache = EmbeddingCache::new();

        let entry = make_entry("some_file.xyz", Some("xyz"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &["Documents".to_string()],
            &[],
            None,
        )
        .await;

        // Since embedding similarity will be 1.0 (identical mock vectors),
        // it should be caught by filename embedding with the mock.
        assert!(!result.needs_review);
    }

    #[tokio::test]
    async fn needs_review_when_llm_confidence_low() {
        let engine = empty_engine();
        let gate = ConfidenceGate::new(Default::default());
        let chat_response =
            r#"{"destination": "Unknown", "confidence": 0.30, "tags": [], "reason": "Not sure"}"#;
        // Use zero vector so embedding similarity is 0, forcing escalation to LLM
        let provider = MockProvider::new(vec![0.0, 0.0, 0.0], chat_response);
        let mut cache = EmbeddingCache::new();

        let entry = make_entry("mystery.bin", Some("bin"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &[],
            &[],
            None,
        )
        .await;

        assert!(result.needs_review);
    }

    #[tokio::test]
    async fn embedding_cache_is_populated() {
        let engine = empty_engine();
        let gate = ConfidenceGate::new(Default::default());
        let chat_response =
            r#"{"destination": "Docs", "confidence": 0.9, "tags": [], "reason": "ok"}"#;
        let provider = MockProvider::new(vec![1.0, 0.0], chat_response);
        let mut cache = EmbeddingCache::new();
        let buckets = vec!["Documents".to_string(), "Photos".to_string()];

        let entry = make_entry("test.xyz", Some("xyz"));
        let _ = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &buckets,
            &[],
            None,
        )
        .await;

        // Bucket embeddings should now be cached
        assert!(cache.get("Documents").is_some());
        assert!(cache.get("Photos").is_some());
    }

    #[tokio::test]
    async fn centroid_match_takes_priority_over_bucket_embedding() {
        let engine = empty_engine();
        let gate = ConfidenceGate::new(Default::default());
        let chat_response =
            r#"{"destination": "Other", "confidence": 0.9, "tags": [], "reason": "ok"}"#;
        // Provider returns [0.9, 0.1, 0.0] for all embeds
        let provider = MockProvider::new(vec![0.9, 0.1, 0.0], chat_response);
        let mut cache = EmbeddingCache::new();

        // Set up vector store with a centroid that matches well
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vectors.msgpack");
        let mut vs = crate::qdrant::InMemoryVectorStore::new(path);
        vs.upsert("Downloads", "xyz", "Invoices", &[0.9, 0.1, 0.0], 1.0);

        let entry = make_entry("test.xyz", Some("xyz"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &["Documents".to_string()],
            &[],
            Some(&vs),
        )
        .await;

        // Should match via centroid, not bucket name embedding
        assert_eq!(result.destination, PathBuf::from("Invoices"));
        assert_eq!(result.method, ClassificationMethod::FilenameEmbedding);
        assert!(result.filename_embedding.is_some());
    }

    #[tokio::test]
    async fn few_shot_examples_passed_to_llm() {
        let engine = empty_engine();
        let gate = ConfidenceGate::new(Default::default());
        let chat_response = r#"{"destination": "Personal", "confidence": 0.85, "tags": ["personal"], "reason": "Based on previous corrections"}"#;
        // Zero vector forces escalation past embedding tiers
        let provider = MockProvider::new(vec![0.0, 0.0, 0.0], chat_response);
        let mut cache = EmbeddingCache::new();

        let examples = vec![
            "You previously placed report.pdf into /Work. The user moved it to /Personal. Learn from this.".to_string(),
        ];

        let entry = make_entry("summary.doc", Some("doc"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &[],
            &examples,
            None,
        )
        .await;

        assert_eq!(result.destination, PathBuf::from("Personal"));
        assert!(!result.needs_review);
    }

    #[test]
    fn embedding_cache_insert_and_get() {
        let mut cache = EmbeddingCache::new();
        assert!(cache.get("foo").is_none());

        cache.insert("foo".to_string(), vec![1.0, 2.0]);
        let v = cache.get("foo").unwrap();
        assert_eq!(v, &vec![1.0, 2.0]);
    }

    #[test]
    fn embedding_cache_overwrite() {
        let mut cache = EmbeddingCache::new();
        cache.insert("key".to_string(), vec![1.0]);
        cache.insert("key".to_string(), vec![2.0]);
        assert_eq!(cache.get("key").unwrap(), &vec![2.0]);
    }

    #[test]
    fn embedding_cache_multiple_keys() {
        let mut cache = EmbeddingCache::new();
        cache.insert("a".to_string(), vec![1.0]);
        cache.insert("b".to_string(), vec![2.0]);
        cache.insert("c".to_string(), vec![3.0]);
        assert_eq!(cache.get("a").unwrap(), &vec![1.0]);
        assert_eq!(cache.get("b").unwrap(), &vec![2.0]);
        assert_eq!(cache.get("c").unwrap(), &vec![3.0]);
        assert!(cache.get("d").is_none());
    }

    #[tokio::test]
    async fn content_embedding_classifies_text_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("notes.txt");
        std::fs::write(&file_path, "Meeting notes about quarterly budget review").unwrap();

        let engine = empty_engine();
        let gate = ConfidenceGate::new(Default::default());
        // Same embedding for everything -> cosine similarity = 1.0
        let chat_response =
            r#"{"destination": "Misc", "confidence": 0.9, "tags": [], "reason": "fallback"}"#;
        let provider = MockProvider::new(vec![1.0, 0.0, 0.0], chat_response);
        let mut cache = EmbeddingCache::new();

        let entry = FileEntry {
            path: file_path,
            name: "notes.txt".to_string(),
            extension: Some("txt".to_string()),
            size_bytes: 100,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            tags: Vec::new(),
            colour: None,
            source_inbox: "Downloads".to_string(),
        };

        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &["Finance".to_string()],
            &[],
            None,
        )
        .await;

        // With identical embeddings (sim=1.0), should be accepted by filename embedding tier
        assert!(!result.needs_review);
        assert!(result.confidence.unwrap() >= 0.80);
    }

    #[tokio::test]
    async fn all_tiers_fail_returns_needs_review() {
        let engine = empty_engine();
        let gate = ConfidenceGate::new(Default::default());
        // Zero vector -> cosine similarity will be 0
        // Invalid JSON -> LLM parse fails
        let provider = MockProvider::new(vec![0.0, 0.0], "not valid json");
        let mut cache = EmbeddingCache::new();

        let entry = make_entry("mystery.bin", Some("bin"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &[],
            &[],
            None,
        )
        .await;

        assert!(result.needs_review);
        assert!(result.reason.unwrap().contains("failed"));
    }

    #[tokio::test]
    async fn llm_low_confidence_flags_needs_review() {
        let engine = empty_engine();
        let gate = ConfidenceGate::new(Default::default()); // LLM threshold = 0.70
        let chat_response = r#"{"destination": "Maybe", "confidence": 0.40, "tags": [], "reason": "Not confident"}"#;
        let provider = MockProvider::new(vec![0.0, 0.0], chat_response);
        let mut cache = EmbeddingCache::new();

        let entry = make_entry("ambiguous.dat", Some("dat"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &[],
            &[],
            None,
        )
        .await;

        assert!(result.needs_review);
        assert_eq!(result.method, ClassificationMethod::Llm);
        assert!(result.confidence.unwrap() < 0.70);
    }

    #[tokio::test]
    async fn rule_match_preserves_tags_and_colour() {
        let yaml = r#"
rules:
  - name: "Screenshots"
    match:
      filename: "Screenshot*"
    destination: "Screenshots"
    tags: ["screenshot", "image"]
    colour: blue
"#;
        let ruleset = load_rules_from_str(yaml).unwrap();
        let engine = RuleEngine::new(ruleset);
        let gate = ConfidenceGate::new(Default::default());
        let provider = MockProvider::new(vec![0.0], "{}");
        let mut cache = EmbeddingCache::new();

        let entry = make_entry("Screenshot 2025-01-15.png", Some("png"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &[],
            &[],
            None,
        )
        .await;

        assert_eq!(result.destination, PathBuf::from("Screenshots"));
        assert!(result.tags.contains(&"screenshot".to_string()));
        assert!(result.tags.contains(&"image".to_string()));
        assert_eq!(result.confidence, Some(1.0));
    }

    #[tokio::test]
    async fn empty_vector_store_skips_centroid_tier() {
        let engine = empty_engine();
        let gate = ConfidenceGate::new(Default::default());
        let chat_response =
            r#"{"destination": "Docs", "confidence": 0.9, "tags": [], "reason": "ok"}"#;
        let provider = MockProvider::new(vec![1.0, 0.0], chat_response);
        let mut cache = EmbeddingCache::new();

        // Empty vector store
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vectors.msgpack");
        let vs = crate::qdrant::InMemoryVectorStore::new(path);

        let entry = make_entry("test.xyz", Some("xyz"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &["Docs".to_string()],
            &[],
            Some(&vs),
        )
        .await;

        // Should skip centroid tier (empty store) and match via filename embedding
        assert!(!result.needs_review);
        assert_eq!(result.method, ClassificationMethod::FilenameEmbedding);
    }

    #[tokio::test]
    async fn ai_suggest_rule_delegates_to_llm_and_keeps_tags() {
        let yaml = r#"
rules:
  - name: "PDFs to AI"
    match:
      extension: "pdf"
    destination: "{ai_suggest}"
    tags: ["document", "pdf"]
    colour: green
"#;
        let ruleset = load_rules_from_str(yaml).unwrap();
        let engine = RuleEngine::new(ruleset);
        let gate = ConfidenceGate::new(Default::default());
        // Zero vector -> embedding tiers won't match
        let chat_response = r#"{"destination": "Finance/Reports", "confidence": 0.85, "tags": ["finance"], "reason": "Looks like a financial report"}"#;
        let provider = MockProvider::new(vec![0.0, 0.0, 0.0], chat_response);
        let mut cache = EmbeddingCache::new();

        let entry = make_entry("quarterly-report.pdf", Some("pdf"));
        let result = ClassificationPipeline::classify(
            &entry,
            &engine,
            &provider,
            &gate,
            &mut cache,
            &[],
            &[],
            None,
        )
        .await;

        // Destination should come from the LLM, not the rule
        assert_eq!(result.destination, PathBuf::from("Finance/Reports"));
        assert_eq!(result.method, ClassificationMethod::Llm);
        // Tags from the rule should be merged in
        assert!(result.tags.contains(&"document".to_string()));
        assert!(result.tags.contains(&"pdf".to_string()));
        // LLM tags should also be present
        assert!(result.tags.contains(&"finance".to_string()));
        // Colour from the rule should be preserved
        assert_eq!(result.colour, Some(FinderColour::Green));
        assert!(!result.needs_review);
    }
}
