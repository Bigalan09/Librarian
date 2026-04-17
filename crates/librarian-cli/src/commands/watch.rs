//! `librarian watch` - watch destination for manual file corrections.
//!
//! Starts a CorrectionWatcher on the destination root and periodically
//! checks for file moves that look like user corrections. Runs until
//! interrupted with Ctrl-C.

use std::collections::HashMap;

use librarian_core::config;
use librarian_core::plan::{Plan, PlanStatus};
use librarian_learning::CorrectionWatcher;

pub async fn run() -> anyhow::Result<()> {
    let cfg = config::load_default()?;
    let home = config::librarian_home();
    let corrections_path = home.join("history").join("corrections.jsonl");
    let decisions_path = home.join("history").join("decisions.jsonl");
    let plans_dir = home.join("plans");

    // Build a manifest of known file placements from applied plans
    let mut manifest: HashMap<String, std::path::PathBuf> = HashMap::new();
    if plans_dir.exists() {
        for plan in Plan::list(&plans_dir)? {
            if plan.status != PlanStatus::Applied {
                continue;
            }
            for action in &plan.actions {
                if action.destination_path.exists() && !action.file_hash.is_empty() {
                    manifest.insert(action.file_hash.clone(), action.destination_path.clone());
                }
            }
        }
    }

    if manifest.is_empty() {
        println!("No applied plans with active file placements found. Apply a plan first.");
        return Ok(());
    }

    let watch_dirs = vec![cfg.destination_root.clone()];
    let watcher = CorrectionWatcher::new(&watch_dirs)?;

    println!(
        "Watching {} for corrections ({} tracked files). Press Ctrl-C to stop.",
        cfg.destination_root.display(),
        manifest.len(),
    );

    // Poll for events every 2 seconds
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let corrections = watcher.check_for_corrections(
            &manifest,
            cfg.correction_window_days,
            &corrections_path,
            &decisions_path,
        )?;

        for correction in &corrections {
            println!(
                "  correction: {} -> {}",
                correction.original_path.display(),
                correction.corrected_path.display(),
            );
        }
    }
}
