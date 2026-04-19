//! Regression tests for edge cases and previously reported issues.
//!
//! These tests guard against specific scenarios that could break:
//! - Unicode filenames
//! - Empty directories
//! - Large plan files
//! - Symlink handling
//! - Plan status transitions
//! - Decision log integrity

use std::path::PathBuf;

use librarian_core::IgnoreEngine;
use librarian_core::decision::{ClassificationMethod, read_decisions};
use librarian_core::file_entry::FileEntry;
use librarian_core::plan::{
    ActionType, Plan, PlanStats, PlanStatus, PlannedAction, clean_junk_filename, rename_file,
};
use librarian_core::walker;
use librarian_rules::{RuleEngine, load_rules};
use tempfile::tempdir;

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

// ---------------------------------------------------------------------------
// Unicode filename handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unicode_filenames_are_scanned_and_planned() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("inbox");
    let dest = dir.path().join("dest");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&dest).unwrap();

    // Create files with unicode names
    std::fs::write(source.join("résumé.pdf"), b"pdf content").unwrap();
    std::fs::write(source.join("日本語.txt"), b"text content").unwrap();
    std::fs::write(source.join("café_menu.md"), b"# Menu").unwrap();

    let engine = IgnoreEngine::new(&source, None).unwrap();
    let mut entries = walker::scan_directory(&source, "test", &engine, 100)
        .await
        .unwrap();
    assert_eq!(entries.len(), 3);

    walker::hash_entries(&mut entries).await.unwrap();
    assert!(entries.iter().all(|e| !e.hash.is_empty()));
}

#[tokio::test]
async fn unicode_filenames_move_correctly() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");
    let log = dir.path().join("decisions.jsonl");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&dest).unwrap();

    let file = source.join("Ñoño_report.pdf");
    std::fs::write(&file, b"content").unwrap();

    let dest_file = dest.join("docs").join("Ñoño_report.pdf");

    let mut plan = Plan::new("unicode-test", vec![source], dest);
    plan.actions.push(make_action(
        file.clone(),
        dest_file.clone(),
        ActionType::Move,
        ClassificationMethod::Rule,
    ));

    let report = plan.apply(&log, false).unwrap();
    assert_eq!(report.moved, 1);
    assert!(dest_file.exists());
    assert!(!file.exists());
}

// ---------------------------------------------------------------------------
// Empty directories
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_source_produces_empty_plan() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("empty-inbox");
    std::fs::create_dir_all(&source).unwrap();

    let engine = IgnoreEngine::new(&source, None).unwrap();
    let entries = walker::scan_directory(&source, "test", &engine, 100)
        .await
        .unwrap();
    assert!(entries.is_empty());
}

// ---------------------------------------------------------------------------
// Large plan handling
// ---------------------------------------------------------------------------

#[test]
fn large_plan_save_and_load() {
    let dir = tempdir().unwrap();
    let plans_dir = dir.path().join("plans");

    let mut plan = Plan::new(
        "large-plan",
        vec![PathBuf::from("/src")],
        PathBuf::from("/dest"),
    );

    // Add 1000 actions
    for i in 0..1000 {
        plan.actions.push(make_action(
            PathBuf::from(format!("/src/file_{i}.pdf")),
            PathBuf::from(format!("/dest/docs/file_{i}.pdf")),
            ActionType::Move,
            ClassificationMethod::Rule,
        ));
    }
    plan.stats = PlanStats::from_actions(&plan.actions);
    assert_eq!(plan.stats.total_files, 1000);

    plan.save(&plans_dir).unwrap();
    let loaded = Plan::load(&plans_dir.join(format!("{}.json", plan.id))).unwrap();
    assert_eq!(loaded.actions.len(), 1000);
    assert_eq!(loaded.stats.total_files, 1000);
}

// ---------------------------------------------------------------------------
// Junk filename cleaning regression
// ---------------------------------------------------------------------------

#[test]
fn clean_junk_filename_edge_cases() {
    // IMG_ with no digits (should NOT clean)
    assert_eq!(clean_junk_filename("IMG_abc.jpg"), None);

    // IMG_ with no extension (should NOT clean)
    assert_eq!(clean_junk_filename("IMG_1234"), None);

    // Empty string
    assert_eq!(clean_junk_filename(""), None);

    // scan_ with no digits
    assert_eq!(clean_junk_filename("scan_report.pdf"), None);

    // Screenshot with short date (not enough chars)
    assert_eq!(clean_junk_filename("Screenshot 20.png"), None);

    // Normal filenames should not be cleaned
    assert_eq!(clean_junk_filename("budget_2026.xlsx"), None);
    assert_eq!(clean_junk_filename("IMG_README.md"), None);
}

#[test]
fn clean_junk_filename_lowercase_screenshot() {
    let result = clean_junk_filename("screenshot 2026-04-17 at 14.23.05.png");
    assert_eq!(result, Some("screenshot-2026-04-17-142305.png".to_owned()));
}

// ---------------------------------------------------------------------------
// Rename format regression
// ---------------------------------------------------------------------------

#[test]
fn rename_file_special_characters_in_topic() {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap();
    let result = rename_file("file.pdf", &date, "Q1 Report (Final!)", "pdf");
    assert_eq!(result, "2026-04-17_q1-report-final.pdf");
}

#[test]
fn rename_file_with_dotted_extension() {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let result = rename_file("file.tar.gz", &date, "archive", ".tar.gz");
    assert_eq!(result, "2026-01-01_archive.tar.gz");
}

// ---------------------------------------------------------------------------
// Plan status transitions regression
// ---------------------------------------------------------------------------

#[test]
fn cannot_apply_already_applied_plan() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");
    let log = dir.path().join("decisions.jsonl");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&dest).unwrap();

    let file = source.join("a.txt");
    std::fs::write(&file, b"content").unwrap();

    let mut plan = Plan::new("double-apply", vec![source], dest.clone());
    plan.actions.push(make_action(
        file.clone(),
        dest.join("a.txt"),
        ActionType::Move,
        ClassificationMethod::Rule,
    ));

    plan.apply(&log, false).unwrap();
    assert_eq!(plan.status, PlanStatus::Applied);

    // Second apply should fail
    let result = plan.apply(&log, false);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not in Draft"));
}

#[test]
fn cannot_rollback_draft_plan() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("decisions.jsonl");

    let mut plan = Plan::new(
        "draft-rollback",
        vec![PathBuf::from("/src")],
        PathBuf::from("/dest"),
    );
    assert_eq!(plan.status, PlanStatus::Draft);

    let result = plan.rollback(&log);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not in Applied"));
}

#[test]
fn cannot_rollback_already_rolled_back_plan() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");
    let log = dir.path().join("decisions.jsonl");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&dest).unwrap();

    let file = source.join("a.txt");
    std::fs::write(&file, b"content").unwrap();

    let mut plan = Plan::new("double-rollback", vec![source], dest.clone());
    plan.actions.push(make_action(
        file.clone(),
        dest.join("a.txt"),
        ActionType::Move,
        ClassificationMethod::Rule,
    ));

    plan.apply(&log, false).unwrap();
    plan.rollback(&log).unwrap();
    assert_eq!(plan.status, PlanStatus::RolledBack);

    // Second rollback should fail
    let result = plan.rollback(&log);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Skip and Collision actions in apply
// ---------------------------------------------------------------------------

#[test]
fn skip_and_ignored_actions_are_counted() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("decisions.jsonl");

    let mut plan = Plan::new(
        "skip-test",
        vec![PathBuf::from("/src")],
        PathBuf::from("/dest"),
    );
    plan.actions.push(make_action(
        PathBuf::from("/src/a.txt"),
        PathBuf::new(),
        ActionType::Skip,
        ClassificationMethod::None,
    ));
    plan.actions.push(make_action(
        PathBuf::from("/src/b.txt"),
        PathBuf::new(),
        ActionType::Ignored,
        ClassificationMethod::None,
    ));
    plan.actions.push(make_action(
        PathBuf::from("/src/c.txt"),
        PathBuf::new(),
        ActionType::Collision,
        ClassificationMethod::None,
    ));

    let report = plan.apply(&log, false).unwrap();
    assert_eq!(report.skipped, 3);
    assert_eq!(report.moved, 0);
}

// ---------------------------------------------------------------------------
// Decision log integrity
// ---------------------------------------------------------------------------

#[test]
fn decision_log_survives_multiple_appends() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("decisions.jsonl");

    // Create plans and apply them sequentially
    for i in 0..3 {
        let source = dir.path().join(format!("source_{i}"));
        let dest = dir.path().join(format!("dest_{i}"));
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        let file = source.join("file.txt");
        std::fs::write(&file, format!("content {i}")).unwrap();

        let mut plan = Plan::new(&format!("plan-{i}"), vec![source], dest.clone());
        plan.actions.push(make_action(
            file.clone(),
            dest.join("file.txt"),
            ActionType::Move,
            ClassificationMethod::Rule,
        ));

        plan.apply(&log, false).unwrap();
    }

    let decisions = read_decisions(&log).unwrap();
    assert_eq!(decisions.len(), 3);
}

// ---------------------------------------------------------------------------
// Tag action (no move)
// ---------------------------------------------------------------------------

#[test]
fn tag_action_does_not_move_file() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    let log = dir.path().join("decisions.jsonl");
    std::fs::create_dir_all(&source).unwrap();

    let file = source.join("tagged.txt");
    std::fs::write(&file, b"tag me").unwrap();

    let mut plan = Plan::new("tag-test", vec![source.clone()], dir.path().join("dest"));
    plan.actions.push(PlannedAction {
        tags: vec!["important".to_string()],
        ..make_action(
            file.clone(),
            file.clone(), // dest same as source for Tag action
            ActionType::Tag,
            ClassificationMethod::Rule,
        )
    });

    let report = plan.apply(&log, false).unwrap();
    assert_eq!(report.tagged, 1);
    assert_eq!(report.moved, 0);
    assert!(file.exists(), "file should remain at original location");
}

// ---------------------------------------------------------------------------
// Rules engine edge cases
// ---------------------------------------------------------------------------

#[test]
fn rules_engine_handles_no_rules() {
    let dir = tempdir().unwrap();
    let rules_path = dir.path().join("empty_rules.yaml");
    std::fs::write(&rules_path, "rules: []\n").unwrap();

    let ruleset = load_rules(&rules_path).unwrap();
    let engine = RuleEngine::new(ruleset);

    let entry = FileEntry {
        path: PathBuf::from("/tmp/test.pdf"),
        name: "test.pdf".to_string(),
        extension: Some("pdf".to_string()),
        size_bytes: 100,
        hash: String::new(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        tags: Vec::new(),
        colour: None,
        source_inbox: "Downloads".to_string(),
    };

    assert!(engine.evaluate(&entry).is_none());
}

#[test]
fn rules_template_expansion() {
    let dir = tempdir().unwrap();
    let rules_path = dir.path().join("rules.yaml");
    std::fs::write(
        &rules_path,
        r#"rules:
  - name: "test"
    match:
      extension: "pdf"
    destination: "{year}/{month}/Documents"
"#,
    )
    .unwrap();

    let ruleset = load_rules(&rules_path).unwrap();
    let engine = RuleEngine::new(ruleset);

    let now = chrono::Utc::now();
    let entry = FileEntry {
        path: PathBuf::from("/tmp/test.pdf"),
        name: "test.pdf".to_string(),
        extension: Some("pdf".to_string()),
        size_bytes: 100,
        hash: String::new(),
        created_at: now,
        modified_at: now,
        tags: Vec::new(),
        colour: None,
        source_inbox: "Downloads".to_string(),
    };

    let rule = engine.evaluate(&entry).unwrap();
    let expanded = RuleEngine::expand_destination(&rule.destination, &entry);

    // Should contain the current year
    let year = now.format("%Y").to_string();
    assert!(
        expanded.contains(&year),
        "Expanded destination '{}' should contain year '{}'",
        expanded,
        year
    );
}

// ---------------------------------------------------------------------------
// Ignore engine edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn symlinks_in_inbox_are_handled() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("inbox");
    let target = dir.path().join("target");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&target).unwrap();

    // Create a real file and a symlink
    std::fs::write(source.join("real.txt"), b"real").unwrap();
    let target_file = target.join("external.txt");
    std::fs::write(&target_file, b"external").unwrap();

    // Create symlink to external file
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_file, source.join("link.txt")).unwrap();

    let engine = IgnoreEngine::new(&source, None).unwrap();
    let entries = walker::scan_directory(&source, "test", &engine, 100)
        .await
        .unwrap();

    // Real file should be found; symlink may or may not be depending on ignore rules
    assert!(entries.iter().any(|e| e.name == "real.txt"));
}
