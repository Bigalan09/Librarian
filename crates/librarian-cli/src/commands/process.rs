//! `librarian process` — scan, classify, produce plan.

use std::path::PathBuf;

use librarian_core::config::{self, ProviderType};
use librarian_core::decision::ClassificationMethod;
use librarian_core::file_entry::FinderColour;
use librarian_core::plan::{ActionType, Plan, PlannedAction, PlanStats};
use librarian_core::walker;
use librarian_core::IgnoreEngine;

pub async fn run(
    source: Vec<PathBuf>,
    destination: Option<PathBuf>,
    provider: Option<String>,
    llm_model: Option<String>,
    embed_model: Option<String>,
    rules_path: Option<PathBuf>,
    _threshold: Option<f64>,
    _dry_run: bool,
    plan_name: Option<String>,
    _rename: bool,
) -> anyhow::Result<()> {
    let mut cfg = config::load_default()?;

    // Apply CLI overrides to config
    if let Some(p) = &provider {
        cfg.provider.provider_type = match p.as_str() {
            "openai" => ProviderType::OpenAi,
            "lmstudio" | _ => ProviderType::LmStudio,
        };
    }
    if let Some(m) = llm_model {
        cfg.provider.llm_model = Some(m);
    }
    if let Some(m) = embed_model {
        cfg.provider.embed_model = Some(m);
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
            "Rules file not found: {}. Run 'librarian init' first.",
            rules_file.display()
        );
    }
    let rule_set = librarian_rules::load_rules(&rules_file)?;
    let engine = librarian_rules::RuleEngine::new(rule_set);

    // Try to initialise AI provider (optional — rules-only if unavailable)
    let ai_provider = if provider.is_some() {
        match librarian_providers::router::ProviderRouter::new(&cfg.provider).await {
            Ok(router) => {
                tracing::info!("AI provider connected: {}", router.active().name());
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
    let cache_path = config::librarian_home().join("cache").join("embeddings.msgpack");
    let mut embed_cache = librarian_providers::cache::EmbeddingCache::load(&cache_path)
        .unwrap_or_default();

    // Scan each source folder
    let mut all_entries = Vec::new();
    for src in &sources {
        if !src.exists() {
            tracing::warn!("source folder does not exist: {}", src.display());
            continue;
        }
        let inbox_name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_owned());

        let ignore_engine = IgnoreEngine::new(src, None)?;
        let mut entries = walker::scan_directory(
            src,
            &inbox_name,
            &ignore_engine,
            cfg.max_moves_per_run as usize,
        )
        .await?;
        walker::hash_entries(&mut entries).await?;
        all_entries.extend(entries);
    }

    tracing::info!(
        "scanned {} file(s) from {} source(s)",
        all_entries.len(),
        sources.len()
    );

    // Build plan name
    let source_label = sources
        .first()
        .and_then(|s| s.file_name())
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "scan".to_owned());
    let name = plan_name.unwrap_or_else(|| Plan::auto_name(&source_label));

    let mut plan = Plan::new(&name, sources.clone(), dest_root.clone());
    let mut stats = PlanStats::default();
    stats.total_files = all_entries.len();

    // Confidence gate
    let gate = librarian_classifier::ConfidenceGate::new(cfg.thresholds.clone());

    // Classify each file
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
            continue;
        }

        // Step 2-5: AI classification (if provider available)
        if let Some(ref router) = ai_provider {
            let provider = router.active();

            // Step 2: Filename embedding
            let _filename_result = librarian_classifier::embedding::embed_text_dyn(
                provider,
                &entry.name,
            )
            .await;

            // TODO: compare against bucket centroids from Qdrant
            // For now, fall through to LLM classifier

            // Step 4: LLM classifier
            let llm_result = librarian_classifier::llm::LlmClassifier::classify_dyn(
                provider,
                entry,
                &[], // existing buckets — will be populated from Qdrant
                &[], // few-shot examples — will come from learning layer
            )
            .await;

            match llm_result {
                Ok(result) => {
                    let gate_result = gate.check_llm_confidence(
                        result.confidence,
                        &result.destination,
                    );
                    match gate_result {
                        librarian_classifier::GateResult::Accept { destination, .. } => {
                            let destination_path =
                                dest_root.join(&destination).join(&entry.name);
                            plan.actions.push(PlannedAction {
                                file_hash: entry.hash.clone(),
                                source_path: entry.path.clone(),
                                destination_path,
                                action_type: ActionType::Move,
                                classification_method: ClassificationMethod::Llm,
                                confidence: Some(result.confidence),
                                tags: result.tags,
                                colour: None,
                                rename_to: None,
                                original_name: None,
                                reason: Some(result.reason),
                            });
                            stats.ai_classified += 1;
                        }
                        librarian_classifier::GateResult::NeedsReview { reason } => {
                            let destination_path =
                                cfg.needs_review_path.join(&entry.name);
                            plan.actions.push(PlannedAction {
                                file_hash: entry.hash.clone(),
                                source_path: entry.path.clone(),
                                destination_path,
                                action_type: ActionType::NeedsReview,
                                classification_method: ClassificationMethod::Llm,
                                confidence: Some(result.confidence),
                                tags: vec!["needs-review".to_owned()],
                                colour: Some(FinderColour::Yellow),
                                rename_to: None,
                                original_name: None,
                                reason: Some(reason),
                            });
                            stats.needs_review += 1;
                        }
                        librarian_classifier::GateResult::Escalate => {
                            // Should not happen for LLM tier — treat as needs-review
                            stats.needs_review += 1;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("LLM classification failed for {}: {e}", entry.name);
                    stats.skipped += 1;
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
                        reason: Some(format!("AI classification error: {e}")),
                    });
                }
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
    }

    // Save embedding cache
    if let Err(e) = embed_cache.save(&cache_path) {
        tracing::warn!("failed to save embedding cache: {e}");
    }

    plan.stats = stats;

    // Save plan
    let plans_dir = config::librarian_home().join("plans");
    std::fs::create_dir_all(&plans_dir)?;
    plan.save(&plans_dir)?;

    // Summary
    println!("\nSummary");
    println!("-------");
    println!("Matched rules        {:>5}", plan.stats.rule_matched);
    println!("AI classified        {:>5}", plan.stats.ai_classified);
    println!("Low confidence       {:>5}  -> NeedsReview", plan.stats.needs_review);
    println!("Skipped (no match)   {:>5}", plan.stats.skipped);
    println!("Total files          {:>5}", plan.stats.total_files);
    println!();
    println!(
        "Plan saved: {} ({} files, {} moves)",
        plan.name,
        plan.stats.total_files,
        plan.stats.rule_matched + plan.stats.ai_classified,
    );
    println!("Run 'librarian apply --plan {}' to execute.", plan.name);

    Ok(())
}
