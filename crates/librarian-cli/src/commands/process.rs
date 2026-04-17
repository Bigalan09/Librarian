//! `librarian process` — scan, classify, produce plan.

use std::path::PathBuf;

use librarian_core::config;
use librarian_core::decision::ClassificationMethod;
use librarian_core::plan::{ActionType, Plan, PlannedAction, PlanStats};
use librarian_core::walker;
use librarian_core::IgnoreEngine;

pub async fn run(
    source: Vec<PathBuf>,
    destination: Option<PathBuf>,
    _provider: Option<String>,
    _llm_model: Option<String>,
    _embed_model: Option<String>,
    rules_path: Option<PathBuf>,
    _threshold: Option<f64>,
    _dry_run: bool,
    plan_name: Option<String>,
    _rename: bool,
) -> anyhow::Result<()> {
    let cfg = config::load_default()?;

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
        let mut entries =
            walker::scan_directory(src, &inbox_name, &ignore_engine, cfg.max_moves_per_run as usize)
                .await?;
        walker::hash_entries(&mut entries).await?;
        all_entries.extend(entries);
    }

    tracing::info!("scanned {} file(s) from {} source(s)", all_entries.len(), sources.len());

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

    // Classify each file using rules (US1 — rules only, no AI)
    for entry in &all_entries {
        if let Some(rule) = engine.evaluate(entry) {
            let dest_dir = librarian_rules::RuleEngine::expand_destination(&rule.destination, entry);
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
        } else {
            // US1: no AI yet — unmatched files are skipped
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
                reason: Some("no matching rule".to_owned()),
            });
            stats.skipped += 1;
        }
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
    println!("Skipped (no match)   {:>5}", plan.stats.skipped);
    println!("Total files          {:>5}", plan.stats.total_files);
    println!();
    println!(
        "Plan saved: {} ({} files, {} moves)",
        plan.name,
        plan.stats.total_files,
        plan.stats.rule_matched,
    );
    println!("Run 'librarian apply --plan {}' to execute.", plan.name);

    Ok(())
}
