//! Integration tests for the plan lifecycle: process -> apply -> rollback.

use std::path::PathBuf;

use librarian_core::decision::{read_decisions, ClassificationMethod};
use librarian_core::file_entry::FileEntry;
use librarian_core::plan::{ActionType, Plan, PlannedAction, PlanStats, PlanStatus};
use librarian_core::walker;
use librarian_core::IgnoreEngine;
use librarian_rules::{load_rules, RuleEngine};
use tempfile::tempdir;

/// Build a temp directory with sample files and return (source_dir, dest_dir, temp_handle).
fn setup_test_dirs() -> (PathBuf, PathBuf, tempfile::TempDir) {
    let root = tempdir().unwrap();
    let source = root.path().join("inbox");
    let dest = root.path().join("managed");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&dest).unwrap();

    // Create test files
    std::fs::write(source.join("invoice_acme.pdf"), b"invoice content").unwrap();
    std::fs::write(source.join("report.txt"), b"quarterly report").unwrap();
    std::fs::write(source.join("expenses.csv"), b"date,amount\n2026-01-01,100").unwrap();
    std::fs::write(source.join("random.xyz"), b"unknown file type").unwrap();

    (source, dest, root)
}

fn make_action(
    source: PathBuf,
    dest: PathBuf,
    action_type: ActionType,
    method: ClassificationMethod,
) -> PlannedAction {
    PlannedAction {
        file_hash: String::new(),
        source_path: source,
        destination_path: dest,
        action_type,
        classification_method: method,
        confidence: Some(1.0),
        tags: Vec::new(),
        colour: None,
        rename_to: None,
        original_name: None,
        reason: Some("test".to_string()),
    }
}

#[tokio::test]
async fn scan_hash_and_build_plan() {
    let (source, dest, _root) = setup_test_dirs();

    let engine = IgnoreEngine::new(&source, None).unwrap();
    let mut entries = walker::scan_directory(&source, "test-inbox", &engine, 100)
        .await
        .unwrap();

    assert_eq!(entries.len(), 4);
    assert!(entries.iter().all(|e| e.hash.is_empty()));

    walker::hash_entries(&mut entries).await.unwrap();
    assert!(entries.iter().all(|e| !e.hash.is_empty()));

    // Build a plan from the entries
    let mut plan = Plan::new("integration-test", vec![source], dest.clone());
    for entry in &entries {
        let dest_path = dest.join("sorted").join(&entry.name);
        plan.actions.push(make_action(
            entry.path.clone(),
            dest_path,
            ActionType::Move,
            ClassificationMethod::Rule,
        ));
    }
    plan.stats = PlanStats::from_actions(&plan.actions);

    assert_eq!(plan.status, PlanStatus::Draft);
    assert_eq!(plan.stats.total_files, 4);
    assert_eq!(plan.stats.rule_matched, 4);
}

#[tokio::test]
async fn full_apply_and_rollback_cycle() {
    let (source, dest, root) = setup_test_dirs();
    let log = root.path().join("decisions.jsonl");

    let engine = IgnoreEngine::new(&source, None).unwrap();
    let mut entries = walker::scan_directory(&source, "test-inbox", &engine, 100)
        .await
        .unwrap();
    walker::hash_entries(&mut entries).await.unwrap();

    // Build plan
    let mut plan = Plan::new("lifecycle-test", vec![source.clone()], dest.clone());
    for entry in &entries {
        let dest_path = dest.join("sorted").join(&entry.name);
        plan.actions
            .push(make_action(entry.path.clone(), dest_path, ActionType::Move, ClassificationMethod::Rule));
    }

    // Apply
    let report = plan.apply(&log, false).unwrap();
    assert_eq!(report.moved, 4);
    assert_eq!(report.errors.len(), 0);
    assert_eq!(plan.status, PlanStatus::Applied);

    // Verify files moved
    assert!(!source.join("invoice_acme.pdf").exists());
    assert!(dest.join("sorted").join("invoice_acme.pdf").exists());

    // Verify decisions logged
    let decisions = read_decisions(&log).unwrap();
    assert_eq!(decisions.len(), 4);

    // Rollback
    plan.rollback(&log).unwrap();
    assert_eq!(plan.status, PlanStatus::RolledBack);

    // Verify files restored
    assert!(source.join("invoice_acme.pdf").exists());
    assert!(!dest.join("sorted").join("invoice_acme.pdf").exists());
}

#[tokio::test]
async fn apply_with_backup_and_rollback() {
    let (source, dest, root) = setup_test_dirs();
    let log = root.path().join("decisions.jsonl");
    let backup_dir = root.path().join("backups");

    let engine = IgnoreEngine::new(&source, None).unwrap();
    let mut entries = walker::scan_directory(&source, "test-inbox", &engine, 100)
        .await
        .unwrap();
    walker::hash_entries(&mut entries).await.unwrap();

    let mut plan = Plan::new("backup-test", vec![source.clone()], dest.clone());
    for entry in &entries {
        let dest_path = dest.join("sorted").join(&entry.name);
        plan.actions
            .push(make_action(entry.path.clone(), dest_path, ActionType::Move, ClassificationMethod::Rule));
    }

    // Backup first
    plan.backup(&backup_dir).unwrap();
    assert!(plan.backup_path.is_some());

    // Apply
    plan.apply(&log, false).unwrap();
    assert_eq!(plan.status, PlanStatus::Applied);

    // Rollback (should restore from backup)
    plan.rollback(&log).unwrap();
    assert_eq!(plan.status, PlanStatus::RolledBack);

    // Files should be back
    assert!(source.join("invoice_acme.pdf").exists());
    assert!(source.join("report.txt").exists());
}

#[tokio::test]
async fn plan_save_load_preserves_state() {
    let (source, dest, root) = setup_test_dirs();
    let plans_dir = root.path().join("plans");

    let mut plan = Plan::new("save-load-test", vec![source.clone()], dest.clone());
    plan.actions.push(make_action(
        source.join("invoice_acme.pdf"),
        dest.join("invoices/invoice_acme.pdf"),
        ActionType::Move,
        ClassificationMethod::Rule,
    ));
    plan.stats = PlanStats::from_actions(&plan.actions);

    plan.save(&plans_dir).unwrap();

    let loaded = Plan::load(&plans_dir.join(format!("{}.json", plan.id))).unwrap();
    assert_eq!(loaded.name, "save-load-test");
    assert_eq!(loaded.actions.len(), 1);
    assert_eq!(loaded.status, PlanStatus::Draft);
    assert_eq!(loaded.stats.rule_matched, 1);
}

#[tokio::test]
async fn collision_detection_skips_existing() {
    let (source, dest, root) = setup_test_dirs();
    let log = root.path().join("decisions.jsonl");

    // Pre-create a file at the destination
    let dest_dir = dest.join("sorted");
    std::fs::create_dir_all(&dest_dir).unwrap();
    std::fs::write(dest_dir.join("invoice_acme.pdf"), b"existing").unwrap();

    let mut plan = Plan::new("collision-test", vec![source.clone()], dest.clone());
    plan.actions.push(make_action(
        source.join("invoice_acme.pdf"),
        dest_dir.join("invoice_acme.pdf"),
        ActionType::Move,
        ClassificationMethod::Rule,
    ));

    let report = plan.apply(&log, false).unwrap();
    assert_eq!(report.collisions, 1);
    assert_eq!(report.moved, 0);

    // Original should still be at source
    assert!(source.join("invoice_acme.pdf").exists());
    // Destination should be unchanged
    assert_eq!(
        std::fs::read_to_string(dest_dir.join("invoice_acme.pdf")).unwrap(),
        "existing"
    );
}

#[test]
fn rules_engine_matches_fixtures() {
    let rules_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/sample_rules.yaml");

    let ruleset = load_rules(&rules_path).unwrap();
    let engine = RuleEngine::new(ruleset);

    // Build a fake FileEntry for an invoice PDF
    let entry = FileEntry {
        path: PathBuf::from("/tmp/Downloads/invoice_q1.pdf"),
        name: "invoice_q1.pdf".to_string(),
        extension: Some("pdf".to_string()),
        size_bytes: 1000,
        hash: String::new(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        tags: Vec::new(),
        colour: None,
        source_inbox: "Downloads".to_string(),
    };

    let result = engine.evaluate(&entry);
    assert!(result.is_some(), "invoice PDF should match a rule");

    let rule = result.unwrap();
    assert_eq!(rule.name, "Work invoices");
    assert!(rule.tags.contains(&"invoice".to_string()));

    // CSV should match
    let csv_entry = FileEntry {
        name: "expenses.csv".to_string(),
        extension: Some("csv".to_string()),
        ..entry.clone()
    };
    let csv_result = engine.evaluate(&csv_entry);
    assert!(csv_result.is_some());
    assert_eq!(csv_result.unwrap().name, "CSV data files");

    // Unknown extension should not match
    let unknown = FileEntry {
        name: "random.xyz".to_string(),
        extension: Some("xyz".to_string()),
        ..entry
    };
    assert!(engine.evaluate(&unknown).is_none());
}
