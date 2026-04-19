//! `librarian process` — scan, classify, produce plan.

use std::path::PathBuf;

use librarian_classifier::pipeline::{ClassificationPipeline, EmbeddingCache};
use librarian_classifier::qdrant::InMemoryVectorStore;
use librarian_classifier::{ConfidenceGate, VectorStore};
use librarian_core::IgnoreEngine;
use librarian_core::config::{self, ProviderType};
use librarian_core::decision::ClassificationMethod;
use librarian_core::file_entry::{FileEntry, FinderColour};
use librarian_core::plan::{ActionType, Plan, PlanStats, PlannedAction};
use librarian_core::walker;
use librarian_providers::router::ErasedProvider;
use librarian_providers::traits::ChatMessage;

/// Scan the destination root for existing top-level folder names.
///
/// These are passed to the LLM and embedding tiers as context so the AI
/// knows what buckets already exist and can prefer them over inventing new ones.
fn discover_buckets(dest_root: &std::path::Path) -> Vec<String> {
    let mut buckets = Vec::new();
    if !dest_root.exists() {
        return buckets;
    }

    let Ok(entries) = std::fs::read_dir(dest_root) else {
        return buckets;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && !name.starts_with('.')
            && !name.starts_with('_')
        {
            buckets.push(name.to_string());
        }
    }

    buckets.sort();
    buckets
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    source: Vec<PathBuf>,
    destination: Option<PathBuf>,
    provider: Option<String>,
    llm_model: Option<String>,
    embed_model: Option<String>,
    rules_path: Option<PathBuf>,
    threshold: Option<f64>,
    dry_run: bool,
    plan_name: Option<String>,
    rename: bool,
) -> anyhow::Result<()> {
    if dry_run {
        tracing::info!("dry-run mode: plan will be saved but not applied");
    }

    let mut cfg = config::load_default()?;

    // Validate config
    match config::validate(&cfg) {
        Ok(warnings) => {
            for w in &warnings {
                tracing::warn!("{w}");
            }
        }
        Err(errors) => {
            for e in &errors {
                tracing::error!("{e}");
            }
            anyhow::bail!(
                "Configuration has {} error(s). Fix config.yaml and retry.",
                errors.len()
            );
        }
    }

    // Apply CLI overrides to config
    if let Some(p) = &provider {
        cfg.provider.provider_type = match p.as_str() {
            "openai" => ProviderType::OpenAi,
            _ => ProviderType::LmStudio,
        };
    }
    if let Some(m) = llm_model {
        cfg.provider.llm_model = Some(m);
    }
    if let Some(m) = embed_model {
        cfg.provider.embed_model = Some(m);
    }
    if let Some(t) = threshold {
        cfg.thresholds.filename_embedding = t;
        cfg.thresholds.content_embedding = t;
        cfg.thresholds.llm_confidence = t;
    }

    let sources = if source.is_empty() {
        cfg.inbox_folders.clone()
    } else {
        source
    };
    let dest_root = destination.unwrap_or_else(|| cfg.destination_root.clone());
    let rules_file = rules_path.unwrap_or_else(|| config::librarian_home().join("rules.yaml"));

    // Load rules
    if !rules_file.exists() {
        anyhow::bail!(
            "Rules file not found at {}. Run 'librarian init' to create a default rules file, \
             or pass --rules <path> to specify an alternative location.",
            rules_file.display()
        );
    }
    let rule_set = librarian_rules::load_rules(&rules_file)?;
    let engine = librarian_rules::RuleEngine::new(rule_set);

    // Try to initialise AI provider (optional — rules-only if unavailable)
    let ai_provider = if provider.is_some() {
        match librarian_providers::router::ProviderRouter::new(&cfg.provider).await {
            Ok(router) => {
                tracing::info!("AI provider connected: {}", router.active()?.name());
                Some(router)
            }
            Err(e) => {
                tracing::warn!("AI provider unavailable: {e}. Falling back to rules only.");
                None
            }
        }
    } else {
        None
    };

    // Load embedding cache
    let cache_path = config::librarian_home()
        .join("cache")
        .join("embeddings.msgpack");
    let embed_cache =
        librarian_providers::cache::EmbeddingCache::load(&cache_path).unwrap_or_default();

    // Load vector store (in-memory centroid store with msgpack persistence)
    let centroid_path = config::librarian_home()
        .join("cache")
        .join("centroids.msgpack");
    let mut vector_store = InMemoryVectorStore::load(&centroid_path).unwrap_or_else(|e| {
        tracing::warn!("Failed to load centroid store: {e}. Starting empty.");
        InMemoryVectorStore::new(centroid_path.clone())
    });

    // Discover existing buckets from destination directory structure
    let mut existing_buckets = discover_buckets(&dest_root);

    // Merge in bucket names from the vector store (centroids may know about
    // buckets that no longer have a physical directory)
    for bucket in vector_store.all_buckets() {
        if !existing_buckets.contains(&bucket) {
            existing_buckets.push(bucket);
        }
    }
    existing_buckets.sort();

    tracing::info!(
        bucket_count = existing_buckets.len(),
        "Discovered existing buckets: {:?}",
        existing_buckets
    );

    // Load few-shot examples from correction history
    let corrections_path = config::librarian_home()
        .join("history")
        .join("corrections.jsonl");
    let fewshot_count = cfg.fewshot_count as usize;

    // Scan each source folder
    let mut all_entries = Vec::new();
    let scan_start = std::time::Instant::now();
    let scan_pb = crate::output::create_scan_progress(sources.len() as u64);
    for src in &sources {
        if !src.exists() {
            tracing::warn!("source folder does not exist: {}", src.display());
            scan_pb.inc(1);
            continue;
        }
        let inbox_name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_owned());

        scan_pb.set_message(format!("Scanning {inbox_name}"));
        let ignore_engine = IgnoreEngine::new(src, None)?;
        let mut entries = walker::scan_directory(
            src,
            &inbox_name,
            &ignore_engine,
            cfg.max_moves_per_run as usize,
        )
        .await?;

        let hash_start = std::time::Instant::now();
        walker::hash_entries(&mut entries).await?;
        tracing::debug!(
            elapsed_ms = hash_start.elapsed().as_millis() as u64,
            count = entries.len(),
            "hashed files",
        );

        all_entries.extend(entries);
        scan_pb.inc(1);
    }
    scan_pb.finish_and_clear();

    tracing::info!(
        elapsed_ms = scan_start.elapsed().as_millis() as u64,
        files = all_entries.len(),
        sources = sources.len(),
        "scan complete",
    );

    // Build plan name
    let source_label = sources
        .first()
        .and_then(|s| s.file_name())
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "scan".to_owned());
    let name = plan_name.unwrap_or_else(|| Plan::auto_name(&source_label));

    let mut plan = Plan::new(&name, sources.clone(), dest_root.clone());
    let mut stats = PlanStats {
        total_files: all_entries.len(),
        ..PlanStats::default()
    };

    // Confidence gate
    let gate = ConfidenceGate::new(cfg.thresholds.clone());

    // Pipeline embedding cache (separate from the provider-level cache;
    // this one caches bucket-name embeddings during classification)
    let mut pipeline_cache = EmbeddingCache::new();

    // Classify each file
    let classify_start = std::time::Instant::now();
    let classify_pb = crate::output::create_classify_progress(all_entries.len() as u64);
    for entry in &all_entries {
        // Step 1: Rules (deterministic, always first)
        if let Some(rule) = engine.evaluate(entry) {
            let dest_dir =
                librarian_rules::RuleEngine::expand_destination(&rule.destination, entry);
            let destination_path = dest_root.join(&dest_dir).join(&entry.name);

            plan.actions.push(PlannedAction {
                file_hash: entry.hash.clone(),
                source_path: entry.path.clone(),
                destination_path,
                action_type: ActionType::Move,
                classification_method: ClassificationMethod::Rule,
                confidence: None,
                tags: rule.tags.clone(),
                colour: rule.colour,
                rename_to: None,
                original_name: None,
                reason: Some(format!("matched rule: {}", rule.name)),
            });
            stats.rule_matched += 1;
            classify_pb.inc(1);
            continue;
        }

        // Steps 2-5: AI classification (if provider available)
        if let Some(ref router) = ai_provider {
            let provider = router.active()?;

            // Select few-shot examples scoped to this file's inbox + filetype
            let filetype = entry.extension.as_deref();
            let few_shot_examples = librarian_learning::select_examples(
                &corrections_path,
                &entry.source_inbox,
                filetype,
                fewshot_count,
            )
            .unwrap_or_else(|e| {
                tracing::debug!("Failed to load few-shot examples: {e}");
                Vec::new()
            });

            let result = ClassificationPipeline::classify(
                entry,
                &engine,
                provider,
                &gate,
                &mut pipeline_cache,
                &existing_buckets,
                &few_shot_examples,
                Some(&vector_store),
            )
            .await;

            // Skip rule results (already handled above), process AI results
            if result.method == ClassificationMethod::Rule {
                // Shouldn't happen since rules already matched above, but be safe
                continue;
            }

            if result.needs_review {
                let destination_path = cfg.needs_review_path.join(&entry.name);
                plan.actions.push(PlannedAction {
                    file_hash: entry.hash.clone(),
                    source_path: entry.path.clone(),
                    destination_path,
                    action_type: ActionType::NeedsReview,
                    classification_method: result.method,
                    confidence: result.confidence,
                    tags: vec!["needs-review".to_owned()],
                    colour: Some(FinderColour::Yellow),
                    rename_to: None,
                    original_name: None,
                    reason: result.reason,
                });
                stats.needs_review += 1;
            } else {
                // Update vector store with the classification result's embedding
                if let Some(ref embedding) = result.filename_embedding {
                    let ft = entry.extension.as_deref().unwrap_or("unknown");
                    let bucket = result.destination.to_str().unwrap_or("unknown");
                    vector_store.upsert(
                        &entry.source_inbox,
                        ft,
                        bucket,
                        embedding,
                        0.3, // learning rate for gradual drift
                    );
                }

                // Optionally suggest a rename
                let rename_to = if rename {
                    let dest_str = result.destination.to_string_lossy();
                    suggest_rename(provider, entry, &dest_str).await
                } else {
                    None
                };

                let final_name = rename_to.as_deref().unwrap_or(&entry.name);
                let destination_path = dest_root.join(&result.destination).join(final_name);
                let original_name = rename_to.as_ref().map(|_| entry.name.clone());

                plan.actions.push(PlannedAction {
                    file_hash: entry.hash.clone(),
                    source_path: entry.path.clone(),
                    destination_path,
                    action_type: ActionType::Move,
                    classification_method: result.method,
                    confidence: result.confidence,
                    tags: result.tags,
                    colour: result.colour,
                    rename_to,
                    original_name,
                    reason: result.reason,
                });
                stats.ai_classified += 1;
            }
        } else {
            // No AI provider — skip unmatched files
            plan.actions.push(PlannedAction {
                file_hash: entry.hash.clone(),
                source_path: entry.path.clone(),
                destination_path: PathBuf::new(),
                action_type: ActionType::Skip,
                classification_method: ClassificationMethod::None,
                confidence: None,
                tags: Vec::new(),
                colour: None,
                rename_to: None,
                original_name: None,
                reason: Some("no matching rule (AI provider not configured)".to_owned()),
            });
            stats.skipped += 1;
        }
        classify_pb.inc(1);
    }
    classify_pb.finish_and_clear();

    tracing::info!(
        elapsed_ms = classify_start.elapsed().as_millis() as u64,
        rule_matched = stats.rule_matched,
        ai_classified = stats.ai_classified,
        needs_review = stats.needs_review,
        "classification complete",
    );

    // Save embedding cache
    if let Err(e) = embed_cache.save(&cache_path) {
        tracing::warn!("failed to save embedding cache: {e}");
    }

    // Save vector store (centroid updates from this run)
    if let Err(e) = vector_store.save() {
        tracing::warn!("failed to save vector store: {e}");
    }

    plan.stats = stats;

    // Save plan
    let plans_dir = config::librarian_home().join("plans");
    std::fs::create_dir_all(&plans_dir)?;
    plan.save(&plans_dir)?;

    // Summary
    println!();
    crate::output::print_summary(&plan.stats);
    println!(
        "Plan saved: {} ({} files, {} moves)",
        plan.name,
        plan.stats.total_files,
        plan.stats.rule_matched + plan.stats.ai_classified,
    );
    println!("Run 'librarian apply --plan {}' to execute.", plan.name);

    Ok(())
}

/// Ask the LLM to suggest a cleaner filename for a file.
///
/// Returns `None` if the current name is already clean or the LLM declines.
async fn suggest_rename(
    provider: &dyn ErasedProvider,
    entry: &FileEntry,
    destination: &str,
) -> Option<String> {
    let ext = entry.extension.as_deref().unwrap_or("");
    let prompt = format!(
        "Given a file named '{}' being placed into folder '{}', suggest a cleaner, \
         more descriptive filename. Keep the extension '{ext}'. If the name is already \
         fine, respond with just the word KEEP. Otherwise respond with just the new \
         filename, nothing else.",
        entry.name, destination,
    );
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "You are a file renaming assistant. Respond with only the suggested \
                     filename or the word KEEP. No explanation."
                .to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: prompt,
        },
    ];
    match provider.chat(messages, 0.1, 64).await {
        Ok(resp) => {
            let mut suggestion = resp.content.trim().to_string();
            if suggestion.eq_ignore_ascii_case("KEEP") || suggestion == entry.name {
                return None;
            }
            // Ensure the LLM preserved the extension
            if !ext.is_empty() && !suggestion.ends_with(&format!(".{ext}")) {
                suggestion = format!("{suggestion}.{ext}");
            }
            Some(suggestion)
        }
        Err(e) => {
            tracing::debug!("Rename suggestion failed: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use librarian_providers::traits::{ChatResponse, ModelInfo, Provider};

    struct MockRenameProvider {
        response: String,
    }

    impl Provider for MockRenameProvider {
        async fn validate(&self) -> anyhow::Result<ModelInfo> {
            Ok(ModelInfo {
                id: "mock".to_string(),
            })
        }
        async fn chat(
            &self,
            _messages: Vec<ChatMessage>,
            _temperature: f64,
            _max_tokens: u32,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                content: self.response.clone(),
                model: "mock".to_string(),
            })
        }
        async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.0]).collect())
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

    #[tokio::test]
    async fn suggest_rename_returns_new_name() {
        let provider = MockRenameProvider {
            response: "clean-report-2025.pdf".to_string(),
        };
        let entry = make_entry("IMG_4382.pdf", Some("pdf"));
        let result = suggest_rename(&provider, &entry, "Documents").await;
        assert_eq!(result, Some("clean-report-2025.pdf".to_string()));
    }

    #[tokio::test]
    async fn suggest_rename_returns_none_on_keep() {
        let provider = MockRenameProvider {
            response: "KEEP".to_string(),
        };
        let entry = make_entry("report.pdf", Some("pdf"));
        let result = suggest_rename(&provider, &entry, "Documents").await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn suggest_rename_returns_none_on_keep_lowercase() {
        let provider = MockRenameProvider {
            response: "keep".to_string(),
        };
        let entry = make_entry("report.pdf", Some("pdf"));
        let result = suggest_rename(&provider, &entry, "Documents").await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn suggest_rename_returns_none_when_same_name() {
        let provider = MockRenameProvider {
            response: "report.pdf".to_string(),
        };
        let entry = make_entry("report.pdf", Some("pdf"));
        let result = suggest_rename(&provider, &entry, "Documents").await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn suggest_rename_trims_whitespace() {
        let provider = MockRenameProvider {
            response: "  renamed-file.txt  \n".to_string(),
        };
        let entry = make_entry("file.txt", Some("txt"));
        let result = suggest_rename(&provider, &entry, "Docs").await;
        assert_eq!(result, Some("renamed-file.txt".to_string()));
    }

    #[tokio::test]
    async fn suggest_rename_appends_missing_extension() {
        let provider = MockRenameProvider {
            response: "clean-report".to_string(),
        };
        let entry = make_entry("IMG_4382.pdf", Some("pdf"));
        let result = suggest_rename(&provider, &entry, "Documents").await;
        assert_eq!(result, Some("clean-report.pdf".to_string()));
    }

    #[test]
    fn discover_buckets_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create some subdirectories
        std::fs::create_dir(root.join("Documents")).unwrap();
        std::fs::create_dir(root.join("Photos")).unwrap();
        std::fs::create_dir(root.join("Invoices")).unwrap();
        std::fs::create_dir(root.join(".hidden")).unwrap();
        std::fs::create_dir(root.join("_Trash")).unwrap();
        // Create a file (should be ignored)
        std::fs::write(root.join("readme.txt"), "hi").unwrap();

        let buckets = discover_buckets(root);
        assert_eq!(buckets, vec!["Documents", "Invoices", "Photos"]);
    }

    #[test]
    fn discover_buckets_nonexistent_dir() {
        let buckets = discover_buckets(std::path::Path::new("/nonexistent/path"));
        assert!(buckets.is_empty());
    }

    #[test]
    fn discover_buckets_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let buckets = discover_buckets(dir.path());
        assert!(buckets.is_empty());
    }

    #[tokio::test]
    async fn suggest_rename_no_extension() {
        let provider = MockRenameProvider {
            response: "cleaned-name".to_string(),
        };
        let entry = make_entry("mystery_file", None);
        let result = suggest_rename(&provider, &entry, "Misc").await;
        // No extension, so no ".ext" appended
        assert_eq!(result, Some("cleaned-name".to_string()));
    }

    #[tokio::test]
    async fn suggest_rename_provider_error_returns_none() {
        struct FailingProvider;

        impl Provider for FailingProvider {
            async fn validate(&self) -> anyhow::Result<ModelInfo> {
                Ok(ModelInfo {
                    id: "mock".to_string(),
                })
            }
            async fn chat(
                &self,
                _messages: Vec<ChatMessage>,
                _temperature: f64,
                _max_tokens: u32,
            ) -> anyhow::Result<ChatResponse> {
                anyhow::bail!("connection refused")
            }
            async fn embed(&self, _texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
                Ok(vec![])
            }
            fn name(&self) -> &str {
                "failing"
            }
        }

        let provider = FailingProvider;
        let entry = make_entry("file.txt", Some("txt"));
        let result = suggest_rename(&provider, &entry, "Docs").await;
        assert_eq!(result, None, "provider error should return None");
    }

    #[test]
    fn discover_buckets_ignores_files_and_dotfiles() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir(root.join("Valid")).unwrap();
        std::fs::write(root.join("file.txt"), "not a dir").unwrap();
        std::fs::create_dir(root.join(".config")).unwrap();
        std::fs::create_dir(root.join("_backup")).unwrap();

        let buckets = discover_buckets(root);
        assert_eq!(buckets, vec!["Valid"]);
    }

    #[test]
    fn discover_buckets_returns_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir(root.join("Zebra")).unwrap();
        std::fs::create_dir(root.join("Alpha")).unwrap();
        std::fs::create_dir(root.join("Middle")).unwrap();

        let buckets = discover_buckets(root);
        assert_eq!(buckets, vec!["Alpha", "Middle", "Zebra"]);
    }
}
