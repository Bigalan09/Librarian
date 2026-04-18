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

#[cfg(test)]
mod tests {
    use super::*;
    use librarian_core::decision::ClassificationMethod;
    use librarian_core::plan::{ActionType, Plan, PlannedAction};

    fn make_action(dest: std::path::PathBuf, hash: &str) -> PlannedAction {
        PlannedAction {
            file_hash: hash.to_string(),
            source_path: std::path::PathBuf::from("/tmp/source/file.txt"),
            destination_path: dest,
            action_type: ActionType::Move,
            classification_method: ClassificationMethod::Rule,
            confidence: Some(1.0),
            tags: Vec::new(),
            colour: None,
            rename_to: None,
            original_name: None,
            reason: None,
        }
    }

    #[test]
    fn manifest_built_from_applied_plans() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        let dest = dir.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();

        let dest_file = dest.join("file.txt");
        std::fs::write(&dest_file, "content").unwrap();

        let mut plan = Plan::new(
            "test",
            vec![std::path::PathBuf::from("/tmp/src")],
            dest.clone(),
        );
        plan.actions.push(make_action(dest_file.clone(), "abc123"));
        plan.status = PlanStatus::Applied;
        plan.save(&plans_dir).unwrap();

        // Build manifest like the watch command does
        let mut manifest: HashMap<String, std::path::PathBuf> = HashMap::new();
        for p in Plan::list(&plans_dir).unwrap() {
            if p.status != PlanStatus::Applied {
                continue;
            }
            for action in &p.actions {
                if action.destination_path.exists() && !action.file_hash.is_empty() {
                    manifest.insert(action.file_hash.clone(), action.destination_path.clone());
                }
            }
        }

        assert_eq!(manifest.len(), 1);
        assert_eq!(manifest.get("abc123").unwrap(), &dest_file);
    }

    #[test]
    fn manifest_skips_draft_plans() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        let dest = dir.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("f.txt"), "data").unwrap();

        let mut plan = Plan::new(
            "draft",
            vec![std::path::PathBuf::from("/tmp/src")],
            dest.clone(),
        );
        plan.actions.push(make_action(dest.join("f.txt"), "hash1"));
        // Plan stays as Draft (default)
        plan.save(&plans_dir).unwrap();

        let mut manifest: HashMap<String, std::path::PathBuf> = HashMap::new();
        for p in Plan::list(&plans_dir).unwrap() {
            if p.status != PlanStatus::Applied {
                continue;
            }
            for action in &p.actions {
                if action.destination_path.exists() && !action.file_hash.is_empty() {
                    manifest.insert(action.file_hash.clone(), action.destination_path.clone());
                }
            }
        }

        assert!(manifest.is_empty(), "draft plans should be skipped");
    }

    #[test]
    fn manifest_skips_empty_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        let dest = dir.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("f.txt"), "data").unwrap();

        let mut plan = Plan::new(
            "test",
            vec![std::path::PathBuf::from("/tmp/src")],
            dest.clone(),
        );
        plan.actions.push(make_action(dest.join("f.txt"), ""));
        plan.status = PlanStatus::Applied;
        plan.save(&plans_dir).unwrap();

        let mut manifest: HashMap<String, std::path::PathBuf> = HashMap::new();
        for p in Plan::list(&plans_dir).unwrap() {
            if p.status != PlanStatus::Applied {
                continue;
            }
            for action in &p.actions {
                if action.destination_path.exists() && !action.file_hash.is_empty() {
                    manifest.insert(action.file_hash.clone(), action.destination_path.clone());
                }
            }
        }

        assert!(manifest.is_empty(), "empty hashes should be skipped");
    }

    #[test]
    fn manifest_skips_missing_dest_files() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        let dest = dir.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();

        let mut plan = Plan::new(
            "test",
            vec![std::path::PathBuf::from("/tmp/src")],
            dest.clone(),
        );
        // Destination file does NOT exist
        plan.actions
            .push(make_action(dest.join("gone.txt"), "hash1"));
        plan.status = PlanStatus::Applied;
        plan.save(&plans_dir).unwrap();

        let mut manifest: HashMap<String, std::path::PathBuf> = HashMap::new();
        for p in Plan::list(&plans_dir).unwrap() {
            if p.status != PlanStatus::Applied {
                continue;
            }
            for action in &p.actions {
                if action.destination_path.exists() && !action.file_hash.is_empty() {
                    manifest.insert(action.file_hash.clone(), action.destination_path.clone());
                }
            }
        }

        assert!(manifest.is_empty(), "missing files should be skipped");
    }
}
