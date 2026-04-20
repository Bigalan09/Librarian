//! E2E / smoke tests for the `librarian` CLI binary.
//!
//! These tests exercise the binary directly via `assert_cmd`, ensuring each
//! subcommand starts, produces expected output, and exits cleanly.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

/// Helper: build a `Command` for the `librarian` binary.
fn librarian() -> Command {
    Command::cargo_bin("librarian").unwrap()
}

/// Minimal home with config, plans dir, and nothing else.
fn setup_minimal_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(home.join("plans")).unwrap();
    std::fs::write(
        home.join("config.yaml"),
        "inbox_folders: []\ndestination_root: /tmp/managed\n",
    )
    .unwrap();
    (dir, home)
}

/// Full home with inbox, destination, rules, history, cache, and backup dirs.
fn setup_full_home() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    let inbox = home.join("inbox");
    let dest = home.join("managed");
    std::fs::create_dir_all(home.join("plans")).unwrap();
    std::fs::create_dir_all(home.join("history")).unwrap();
    std::fs::create_dir_all(home.join("cache")).unwrap();
    std::fs::create_dir_all(home.join("backup")).unwrap();
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::create_dir_all(&dest).unwrap();

    let config = format!(
        "inbox_folders:\n  - {}\ndestination_root: {}\nneeds_review_path: {}/NeedsReview\ntrash_path: {}/Trash\n",
        inbox.display(),
        dest.display(),
        dest.display(),
        dest.display(),
    );
    std::fs::write(home.join("config.yaml"), &config).unwrap();
    std::fs::write(
        home.join("rules.yaml"),
        "rules:\n  - name: \"PDFs\"\n    match:\n      extension: \"pdf\"\n    destination: \"Documents\"\n",
    )
    .unwrap();

    (dir, home, inbox, dest)
}

// ---------------------------------------------------------------------------
// Basic CLI smoke tests
// ---------------------------------------------------------------------------

#[test]
fn version_flag() {
    librarian()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("librarian"));
}

#[test]
fn help_flag() {
    librarian()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Organise files using rules and AI",
        ));
}

#[test]
fn help_for_process() {
    librarian()
        .args(["process", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Scan inbox folders"));
}

#[test]
fn help_for_apply() {
    librarian()
        .args(["apply", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Execute a previously generated plan",
        ));
}

#[test]
fn help_for_rollback() {
    librarian()
        .args(["rollback", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Reverse an applied plan"));
}

#[test]
fn help_for_init() {
    librarian()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Scaffold"));
}

#[test]
fn help_for_status() {
    librarian()
        .args(["status", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("List plans"));
}

#[test]
fn help_for_plans() {
    librarian()
        .args(["plans", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Manage named plans"));
}

#[test]
fn help_for_rules() {
    librarian()
        .args(["rules", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Validate or suggest"));
}

#[test]
fn help_for_correct() {
    librarian()
        .args(["correct", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Record an explicit correction"));
}

#[test]
fn help_for_watch() {
    librarian()
        .args(["watch", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Watch destination"));
}

#[test]
fn help_for_review() {
    librarian()
        .args(["review", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Interactive review"));
}

#[test]
fn help_for_config() {
    librarian()
        .args(["config", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Show or edit"));
}

#[test]
fn help_for_completions() {
    librarian()
        .args(["completions", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Generate shell completions"));
}

#[test]
fn help_for_suggest_structure() {
    librarian()
        .args(["suggest-structure", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Suggest a folder structure"));
}

#[test]
fn unknown_subcommand_fails() {
    librarian()
        .arg("nonexistent-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

// ---------------------------------------------------------------------------
// Mutual exclusion of output flags
// ---------------------------------------------------------------------------

#[test]
fn verbose_and_quiet_are_mutually_exclusive() {
    librarian()
        .args(["--verbose", "--quiet", "status"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mutually exclusive"));
}

#[test]
fn verbose_and_json_are_mutually_exclusive() {
    librarian()
        .args(["--verbose", "--json", "status"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mutually exclusive"));
}

// ---------------------------------------------------------------------------
// Init command
// ---------------------------------------------------------------------------

#[test]
fn init_creates_directory_structure() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");

    librarian()
        .arg("init")
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Librarian initialised"));

    assert!(home.join("config.yaml").exists());
    assert!(home.join("rules.yaml").exists());
    assert!(home.join("ignore").exists());
    assert!(home.join("plans").is_dir());
    assert!(home.join("history").is_dir());
    assert!(home.join("cache").is_dir());
}

#[test]
fn init_is_idempotent() {
    let dir = tempdir().unwrap();

    // Run init twice
    librarian()
        .arg("init")
        .env("HOME", dir.path())
        .assert()
        .success();

    librarian()
        .arg("init")
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped").count(3)); // config, rules, ignore
}

// ---------------------------------------------------------------------------
// Status command
// ---------------------------------------------------------------------------

#[test]
fn status_with_no_plans() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(home.join("plans")).unwrap();

    // Write a minimal config so load_default succeeds
    let config = format!(
        "inbox_folders: []\ndestination_root: {}\nneeds_review_path: {}/NeedsReview\n",
        dir.path().join("managed").display(),
        dir.path().join("managed").display(),
    );
    std::fs::write(home.join("config.yaml"), config).unwrap();

    librarian()
        .arg("status")
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No plans found"));
}

// ---------------------------------------------------------------------------
// Plans subcommands
// ---------------------------------------------------------------------------

#[test]
fn plans_list_empty() {
    let (dir, _home) = setup_minimal_home();

    librarian()
        .arg("plans")
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No plans found"));
}

#[test]
fn plans_show_nonexistent() {
    let (dir, _home) = setup_minimal_home();

    librarian()
        .args(["plans", "show", "nonexistent-plan"])
        .env("HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn plans_delete_nonexistent() {
    let (dir, _home) = setup_minimal_home();

    librarian()
        .args(["plans", "delete", "nonexistent-plan"])
        .env("HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn plans_clean_empty() {
    let (dir, _home) = setup_minimal_home();

    librarian()
        .args(["plans", "clean", "--days", "30"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleaned 0 plan(s)"));
}

// ---------------------------------------------------------------------------
// Config subcommands
// ---------------------------------------------------------------------------

#[test]
fn config_show_displays_json() {
    let (dir, _home) = setup_minimal_home();

    librarian()
        .args(["config", "show"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("destination_root"));
}

#[test]
fn config_edit_missing_config() {
    let dir = tempdir().unwrap();
    // No config file exists
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(&home).unwrap();

    librarian()
        .args(["config", "edit"])
        .env("HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ---------------------------------------------------------------------------
// Rules subcommands
// ---------------------------------------------------------------------------

#[test]
fn rules_validate_with_fixture() {
    let rules_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample_rules.yaml");

    librarian()
        .args(["rules", "validate", "--rules", rules_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rules valid"));
}

#[test]
fn rules_validate_missing_file() {
    librarian()
        .args(["rules", "validate", "--rules", "/nonexistent/rules.yaml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn rules_suggest_no_corrections() {
    let (dir, home) = setup_minimal_home();
    std::fs::create_dir_all(home.join("history")).unwrap();

    librarian()
        .args(["rules", "suggest"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No corrections recorded"));
}

// ---------------------------------------------------------------------------
// Correct command
// ---------------------------------------------------------------------------

#[test]
fn correct_nonexistent_file() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(home.join("history")).unwrap();
    std::fs::write(
        home.join("config.yaml"),
        "inbox_folders: []\ndestination_root: /tmp/managed\n",
    )
    .unwrap();

    librarian()
        .args(["correct", "/nonexistent/file.pdf", "--to", "/tmp/dest"])
        .env("HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("File not found"));
}

#[test]
fn correct_requires_to_or_retag() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(home.join("history")).unwrap();
    std::fs::write(
        home.join("config.yaml"),
        "inbox_folders: []\ndestination_root: /tmp/managed\n",
    )
    .unwrap();

    // Create a real file to correct
    let file = dir.path().join("testfile.txt");
    std::fs::write(&file, "test content").unwrap();

    librarian()
        .args(["correct", file.to_str().unwrap()])
        .env("HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Nothing to do"));
}

// ---------------------------------------------------------------------------
// Apply command
// ---------------------------------------------------------------------------

#[test]
fn apply_no_plans_directory() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("config.yaml"),
        "inbox_folders: []\ndestination_root: /tmp/managed\n",
    )
    .unwrap();
    // No plans directory

    librarian()
        .args(["apply"])
        .env("HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No plans directory"));
}

#[test]
fn apply_nonexistent_plan() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(home.join("plans")).unwrap();
    std::fs::write(
        home.join("config.yaml"),
        "inbox_folders: []\ndestination_root: /tmp/managed\n",
    )
    .unwrap();

    librarian()
        .args(["apply", "--plan", "nonexistent"])
        .env("HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ---------------------------------------------------------------------------
// Rollback command
// ---------------------------------------------------------------------------

#[test]
fn rollback_no_plans_directory() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("config.yaml"),
        "inbox_folders: []\ndestination_root: /tmp/managed\n",
    )
    .unwrap();

    librarian()
        .args(["rollback"])
        .env("HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No plans directory"));
}

// ---------------------------------------------------------------------------
// Shell completions
// ---------------------------------------------------------------------------

#[test]
fn completions_bash() {
    librarian()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn completions_zsh() {
    librarian()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("compdef"));
}

#[test]
fn completions_fish() {
    librarian()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

// ---------------------------------------------------------------------------
// Process command (rules-only, no AI provider)
// ---------------------------------------------------------------------------

#[test]
fn process_rules_only_produces_plan() {
    let (dir, home, inbox, dest) = setup_full_home();

    std::fs::write(inbox.join("invoice.pdf"), "pdf content").unwrap();
    std::fs::write(inbox.join("photo.jpg"), "jpg content").unwrap();

    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Summary"))
        .stdout(predicate::str::contains("Matched rules"))
        .stdout(predicate::str::contains("Plan saved"));

    let plans: Vec<_> = std::fs::read_dir(home.join("plans"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(plans.len(), 1, "expected one plan file");
}

// ---------------------------------------------------------------------------
// Full lifecycle: process -> apply -> rollback
// ---------------------------------------------------------------------------

#[test]
fn full_lifecycle_process_apply_rollback() {
    let (dir, home, inbox, dest) = setup_full_home();

    std::fs::write(inbox.join("report.pdf"), "report content").unwrap();

    // Step 1: Process
    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success();

    // Find the plan name
    let plan_files: Vec<_> = std::fs::read_dir(home.join("plans"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(plan_files.len(), 1);
    let plan_name = plan_files[0]
        .path()
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Step 2: Apply with dry-run first
    librarian()
        .args(["apply", "--plan", &plan_name, "--dry-run"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run"));

    // File should still be in inbox
    assert!(inbox.join("report.pdf").exists());

    // Step 3: Apply for real with backup
    librarian()
        .args(["apply", "--plan", &plan_name, "--backup"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Apply complete"));

    // File should be moved
    assert!(!inbox.join("report.pdf").exists());
    assert!(dest.join("Documents").join("report.pdf").exists());

    // Step 4: Rollback
    librarian()
        .args(["rollback", "--plan", &plan_name])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Rolled back"));

    // File should be restored
    assert!(inbox.join("report.pdf").exists());
}

// ---------------------------------------------------------------------------
// Correct command with real file
// ---------------------------------------------------------------------------

#[test]
fn correct_moves_file_and_records() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    let inbox = home.join("inbox");
    let dest = home.join("managed");
    std::fs::create_dir_all(home.join("history")).unwrap();
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::create_dir_all(&dest).unwrap();

    let config = format!(
        "inbox_folders:\n  - {}\ndestination_root: {}\n",
        inbox.display(),
        dest.display(),
    );
    std::fs::write(home.join("config.yaml"), &config).unwrap();

    // Create source file
    let file = inbox.join("misplaced.txt");
    std::fs::write(&file, "test content").unwrap();

    let correct_dest = dest.join("Correct/Location/misplaced.txt");

    librarian()
        .args([
            "correct",
            file.to_str().unwrap(),
            "--to",
            correct_dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Correction recorded"));

    // File should be moved
    assert!(!file.exists());
    assert!(correct_dest.exists());

    // Correction should be in the log
    let corrections_path = home.join("history/corrections.jsonl");
    assert!(corrections_path.exists());
    let corrections = std::fs::read_to_string(&corrections_path).unwrap();
    assert!(corrections.contains("misplaced.txt"));
}

// ---------------------------------------------------------------------------
// Correct moves a directory (folder as atomic unit)
// ---------------------------------------------------------------------------

#[test]
fn correct_moves_directory() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    let inbox = home.join("inbox");
    let dest = home.join("managed");
    std::fs::create_dir_all(home.join("history")).unwrap();
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::create_dir_all(&dest).unwrap();

    let config = format!(
        "inbox_folders:\n  - {}\ndestination_root: {}\n",
        inbox.display(),
        dest.display(),
    );
    std::fs::write(home.join("config.yaml"), &config).unwrap();

    // Create a source directory with contents
    let folder = inbox.join("MyProject");
    std::fs::create_dir_all(folder.join("src")).unwrap();
    std::fs::write(folder.join("README.md"), "# My Project").unwrap();
    std::fs::write(folder.join("src/main.rs"), "fn main() {}").unwrap();

    let correct_dest = dest.join("Projects/MyProject");

    librarian()
        .args([
            "correct",
            folder.to_str().unwrap(),
            "--to",
            correct_dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Correction recorded"));

    // Directory should be moved
    assert!(!folder.exists());
    assert!(correct_dest.exists());
    assert!(correct_dest.join("README.md").exists());
    assert!(correct_dest.join("src/main.rs").exists());

    // Correction should be in the log
    let corrections_path = home.join("history/corrections.jsonl");
    assert!(corrections_path.exists());
    let corrections = std::fs::read_to_string(&corrections_path).unwrap();
    assert!(corrections.contains("MyProject"));
}

// ---------------------------------------------------------------------------
// Correct with retag only (no move)
// ---------------------------------------------------------------------------

#[test]
fn correct_retag_only() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(home.join("history")).unwrap();

    let config = format!(
        "inbox_folders: []\ndestination_root: {}\n",
        dir.path().join("managed").display(),
    );
    std::fs::write(home.join("config.yaml"), &config).unwrap();

    let file = dir.path().join("tagged.txt");
    std::fs::write(&file, "tag me").unwrap();

    librarian()
        .args([
            "correct",
            file.to_str().unwrap(),
            "--retag",
            "important,urgent",
        ])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Correction recorded"));

    // File should still exist in original location
    assert!(file.exists());
}

// ---------------------------------------------------------------------------
// Plans show after process
// ---------------------------------------------------------------------------

#[test]
fn plans_show_after_process() {
    let (dir, home, inbox, dest) = setup_full_home();

    std::fs::write(inbox.join("test.pdf"), "pdf").unwrap();

    // Process
    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success();

    // Get plan name
    let plan_files: Vec<_> = std::fs::read_dir(home.join("plans"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let plan_name = plan_files[0]
        .path()
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Show plan
    librarian()
        .args(["plans", "show", &plan_name])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: Draft"))
        .stdout(predicate::str::contains("Rule matched"));

    // Delete plan
    librarian()
        .args(["plans", "delete", &plan_name])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted"));

    // Verify it's gone
    librarian()
        .args(["plans", "show", &plan_name])
        .env("HOME", dir.path())
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Review with no NeedsReview folder
// ---------------------------------------------------------------------------

#[test]
fn review_no_needs_review_folder() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    std::fs::create_dir_all(&home).unwrap();

    let config = format!(
        "inbox_folders: []\ndestination_root: {dest}\nneeds_review_path: {dest}/NeedsReview\n",
        dest = dir.path().join("managed").display(),
    );
    std::fs::write(home.join("config.yaml"), &config).unwrap();

    librarian()
        .args(["review"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "NeedsReview folder does not exist",
        ));
}

#[test]
fn review_empty_needs_review() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    let needs_review = dir.path().join("managed/NeedsReview");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&needs_review).unwrap();

    let config = format!(
        "inbox_folders: []\ndestination_root: {dest}\nneeds_review_path: {nr}\n",
        dest = dir.path().join("managed").display(),
        nr = needs_review.display(),
    );
    std::fs::write(home.join("config.yaml"), &config).unwrap();

    librarian()
        .args(["review"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No files pending review"));
}

// ---------------------------------------------------------------------------
// Apply dry-run
// ---------------------------------------------------------------------------

#[test]
fn apply_dry_run_does_not_move_files() {
    let (dir, home, inbox, dest) = setup_full_home();

    std::fs::write(inbox.join("stay.pdf"), "stay here").unwrap();

    // Process
    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success();

    let plan_files: Vec<_> = std::fs::read_dir(home.join("plans"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let plan_name = plan_files[0]
        .path()
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Dry run
    librarian()
        .args(["apply", "--plan", &plan_name, "--dry-run"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run"))
        .stdout(predicate::str::contains("MOVE"));

    // File should still be in inbox
    assert!(inbox.join("stay.pdf").exists());
    assert!(!dest.join("Documents/stay.pdf").exists());
}

// ---------------------------------------------------------------------------
// Watch command (early exit when no applied plans)
// ---------------------------------------------------------------------------

#[test]
fn watch_no_applied_plans_exits_early() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".librarian");
    let dest = dir.path().join("managed");
    std::fs::create_dir_all(home.join("plans")).unwrap();
    std::fs::create_dir_all(&dest).unwrap();

    let config = format!("inbox_folders: []\ndestination_root: {}\n", dest.display(),);
    std::fs::write(home.join("config.yaml"), &config).unwrap();

    librarian()
        .arg("watch")
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No applied plans"));
}

// ---------------------------------------------------------------------------
// Apply with --aggressive but no backup
// ---------------------------------------------------------------------------

#[test]
fn apply_aggressive_without_backup_fails() {
    let (dir, home, inbox, dest) = setup_full_home();

    std::fs::write(inbox.join("test.pdf"), "content").unwrap();

    // Process first
    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success();

    let plan_files: Vec<_> = std::fs::read_dir(home.join("plans"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let plan_name = plan_files[0]
        .path()
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Try aggressive without backup
    librarian()
        .args(["apply", "--plan", &plan_name, "--aggressive"])
        .env("HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--aggressive requires --backup"));
}

// ---------------------------------------------------------------------------
// Quiet and JSON output flags
// ---------------------------------------------------------------------------

#[test]
fn quiet_flag_suppresses_output() {
    let (dir, _home) = setup_minimal_home();

    librarian()
        .args(["--quiet", "plans"])
        .env("HOME", dir.path())
        .assert()
        .success();
}

#[test]
fn json_flag_produces_output() {
    let (dir, _home) = setup_minimal_home();

    librarian()
        .args(["--json", "plans"])
        .env("HOME", dir.path())
        .assert()
        .success();
}

#[test]
fn quiet_and_json_are_mutually_exclusive() {
    librarian()
        .args(["--quiet", "--json", "status"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mutually exclusive"));
}

// ---------------------------------------------------------------------------
// Apply latest alias
// ---------------------------------------------------------------------------

#[test]
fn apply_latest_resolves_most_recent() {
    let (dir, _home, inbox, dest) = setup_full_home();

    std::fs::write(inbox.join("doc.pdf"), "content").unwrap();

    // Process
    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success();

    // Apply using "latest" alias
    librarian()
        .args(["apply", "--plan", "latest", "--backup"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Apply complete"));

    // File should be moved
    assert!(dest.join("Documents/doc.pdf").exists());
}

// ---------------------------------------------------------------------------
// Double process produces two plans
// ---------------------------------------------------------------------------

#[test]
fn process_twice_produces_two_plans() {
    let (dir, _home, inbox, dest) = setup_full_home();

    std::fs::write(inbox.join("a.txt"), "first").unwrap();

    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success();

    // Add another file and process again
    std::fs::write(inbox.join("b.txt"), "second").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success();

    // Plans list should show 2 plans (two lines of output with plan names)
    let output = librarian()
        .args(["plans"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();
    let plan_lines: Vec<_> = stdout.lines().filter(|l| l.contains("Draft")).collect();
    assert_eq!(plan_lines.len(), 2, "expected 2 plans, got: {stdout}");
}

// ---------------------------------------------------------------------------
// Rollback latest alias
// ---------------------------------------------------------------------------

#[test]
fn rollback_latest_alias() {
    let (dir, _home, inbox, dest) = setup_full_home();

    std::fs::write(inbox.join("temp.txt"), "rollback test").unwrap();

    // Process and apply
    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success();

    librarian()
        .args(["apply", "--plan", "latest", "--backup"])
        .env("HOME", dir.path())
        .assert()
        .success();

    // Rollback using "latest" alias
    librarian()
        .args(["rollback", "--plan", "latest"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Rolled back plan"));
}

// ---------------------------------------------------------------------------
// Plans clean with default days
// ---------------------------------------------------------------------------

#[test]
fn plans_clean_with_days() {
    let (dir, home) = setup_minimal_home();

    // Create an old plan file
    let plans_dir = home.join("plans");
    std::fs::write(plans_dir.join("ancient.json"), r#"{"id":"ancient","status":"Draft","actions":[],"source_dirs":[],"destination_root":"","created_at":"2020-01-01T00:00:00Z","applied_at":null}"#).unwrap();

    librarian()
        .args(["plans", "clean", "--days", "1"])
        .env("HOME", dir.path())
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Status shows plan counts
// ---------------------------------------------------------------------------

#[test]
fn status_shows_plan_summary() {
    let (dir, _home, inbox, dest) = setup_full_home();

    std::fs::write(inbox.join("status_test.txt"), "data").unwrap();

    librarian()
        .args([
            "process",
            "--source",
            inbox.to_str().unwrap(),
            "--destination",
            dest.to_str().unwrap(),
        ])
        .env("HOME", dir.path())
        .assert()
        .success();

    librarian()
        .args(["status"])
        .env("HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("plan"));
}

// ---------------------------------------------------------------------------
// Init double-run preserves existing config
// ---------------------------------------------------------------------------

#[test]
fn init_preserves_existing_rules() {
    let dir = tempdir().unwrap();

    // First init
    librarian()
        .args(["init"])
        .env("HOME", dir.path())
        .assert()
        .success();

    // Write custom rules
    let rules_path = dir.path().join(".librarian").join("rules.yaml");
    std::fs::write(
        &rules_path,
        "rules:\n  - name: Custom\n    match:\n      extension: xyz\n    destination: Custom\n",
    )
    .unwrap();

    // Second init should not overwrite
    librarian()
        .args(["init"])
        .env("HOME", dir.path())
        .assert()
        .success();

    let content = std::fs::read_to_string(&rules_path).unwrap();
    assert!(content.contains("Custom"));
}
