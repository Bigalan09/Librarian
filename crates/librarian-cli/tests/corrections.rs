//! Integration tests for the correction and learning pipeline.

use std::path::PathBuf;

use librarian_core::config;
use librarian_core::decision::read_decisions;
use librarian_learning::corrections::{
    Correction, CorrectionSource, read_corrections, record_correction,
};
use librarian_learning::fewshot::select_examples;
use tempfile::tempdir;

fn make_correction(
    original: PathBuf,
    corrected: PathBuf,
    hash: &str,
    inbox: &str,
    filetype: Option<&str>,
) -> Correction {
    Correction {
        original_path: original,
        corrected_path: corrected,
        file_hash: hash.to_string(),
        source: CorrectionSource::Explicit,
        corrected_tags: None,
        timestamp: chrono::Utc::now(),
        source_inbox: inbox.to_string(),
        filetype: filetype.map(|s| s.to_string()),
    }
}

#[test]
fn record_and_read_corrections_round_trip() {
    let dir = tempdir().unwrap();
    let corrections_path = dir.path().join("corrections.jsonl");
    let decisions_path = dir.path().join("decisions.jsonl");

    let correction = make_correction(
        PathBuf::from("/inbox/report.pdf"),
        PathBuf::from("/managed/Work/report.pdf"),
        "abc123",
        "Downloads",
        Some("pdf"),
    );

    record_correction(&corrections_path, &decisions_path, &correction).unwrap();

    let corrections = read_corrections(&corrections_path).unwrap();
    assert_eq!(corrections.len(), 1);
    assert_eq!(corrections[0].file_hash, "abc123");
    assert_eq!(corrections[0].source_inbox, "Downloads");

    // Decision log should also have an entry
    let decisions = read_decisions(&decisions_path).unwrap();
    assert_eq!(decisions.len(), 1);
}

#[test]
fn multiple_corrections_accumulate() {
    let dir = tempdir().unwrap();
    let corrections_path = dir.path().join("corrections.jsonl");
    let decisions_path = dir.path().join("decisions.jsonl");

    for i in 0..5 {
        let correction = make_correction(
            PathBuf::from(format!("/inbox/file{i}.pdf")),
            PathBuf::from(format!("/managed/Work/file{i}.pdf")),
            &format!("hash{i}"),
            "Downloads",
            Some("pdf"),
        );
        record_correction(&corrections_path, &decisions_path, &correction).unwrap();
    }

    let corrections = read_corrections(&corrections_path).unwrap();
    assert_eq!(corrections.len(), 5);
}

#[test]
fn fewshot_examples_filter_by_inbox() {
    let dir = tempdir().unwrap();
    let corrections_path = dir.path().join("corrections.jsonl");
    let decisions_path = dir.path().join("decisions.jsonl");

    // Record corrections from different inboxes
    record_correction(
        &corrections_path,
        &decisions_path,
        &make_correction(
            PathBuf::from("/Downloads/a.pdf"),
            PathBuf::from("/Work/a.pdf"),
            "h1",
            "Downloads",
            Some("pdf"),
        ),
    )
    .unwrap();

    record_correction(
        &corrections_path,
        &decisions_path,
        &make_correction(
            PathBuf::from("/Desktop/b.pdf"),
            PathBuf::from("/Personal/b.pdf"),
            "h2",
            "Desktop",
            Some("pdf"),
        ),
    )
    .unwrap();

    // Select examples for Downloads only
    let examples = select_examples(&corrections_path, "Downloads", Some("pdf"), 10).unwrap();
    assert_eq!(examples.len(), 1);
    assert!(examples[0].contains("Downloads"));

    // Select examples for Desktop
    let desktop_examples = select_examples(&corrections_path, "Desktop", Some("pdf"), 10).unwrap();
    assert_eq!(desktop_examples.len(), 1);
}

#[test]
fn fewshot_examples_respect_max_count() {
    let dir = tempdir().unwrap();
    let corrections_path = dir.path().join("corrections.jsonl");
    let decisions_path = dir.path().join("decisions.jsonl");

    for i in 0..20 {
        record_correction(
            &corrections_path,
            &decisions_path,
            &make_correction(
                PathBuf::from(format!("/Downloads/f{i}.txt")),
                PathBuf::from(format!("/Work/f{i}.txt")),
                &format!("hash{i}"),
                "Downloads",
                Some("txt"),
            ),
        )
        .unwrap();
    }

    let examples = select_examples(&corrections_path, "Downloads", Some("txt"), 5).unwrap();
    assert_eq!(examples.len(), 5);
}

#[test]
fn rule_suggestion_from_corrections() {
    let dir = tempdir().unwrap();
    let corrections_path = dir.path().join("corrections.jsonl");
    let decisions_path = dir.path().join("decisions.jsonl");

    // Need 3+ identical patterns for a suggestion
    for i in 0..4 {
        record_correction(
            &corrections_path,
            &decisions_path,
            &make_correction(
                PathBuf::from(format!("/Downloads/invoice_{i}.pdf")),
                PathBuf::from(format!("/Work/Invoices/invoice_{i}.pdf")),
                &format!("ihash{i}"),
                "Downloads",
                Some("pdf"),
            ),
        )
        .unwrap();
    }

    let corrections = librarian_rules::read_correction_records(&corrections_path).unwrap();
    let suggestions = librarian_rules::suggest_rules(&corrections, "rules:\n");

    assert!(
        !suggestions.is_empty(),
        "Expected at least one rule suggestion from 4 similar corrections"
    );
}

#[test]
fn centroid_store_updates_on_correction() {
    let dir = tempdir().unwrap();
    let store_path = dir.path().join("centroids.msgpack");

    let mut store = librarian_learning::centroid::CentroidStore::new();
    assert!(store.is_empty());

    // Update a centroid
    let embedding = vec![1.0_f32, 0.0, 0.0];
    let key = (
        "Downloads".to_string(),
        "pdf".to_string(),
        "Work/Invoices".to_string(),
    );
    store.update_centroid(key, &embedding, 0.3);
    assert_eq!(store.len(), 1);

    // Find nearest should return the bucket
    let result = store.find_nearest("Downloads", "pdf", &embedding);
    assert!(result.is_some());
    let (bucket, similarity) = result.unwrap();
    assert_eq!(bucket, "Work/Invoices");
    assert!(similarity > 0.99);

    // Save and reload
    store.save(&store_path).unwrap();
    let loaded = librarian_learning::centroid::CentroidStore::load(&store_path).unwrap();
    assert_eq!(loaded.len(), 1);
}

#[test]
fn correction_window_filters_old_corrections() {
    let result = librarian_learning::corrections::is_within_correction_window(
        chrono::Utc::now() - chrono::Duration::days(30),
        14,
    );
    assert!(
        !result,
        "30-day-old correction should be outside 14-day window"
    );

    let recent = librarian_learning::corrections::is_within_correction_window(
        chrono::Utc::now() - chrono::Duration::days(1),
        14,
    );
    assert!(recent, "1-day-old correction should be within window");
}

#[test]
fn config_expand_tilde() {
    let expanded = config::expand_tilde(std::path::Path::new("~/Documents"));
    assert!(!expanded.to_string_lossy().starts_with('~'));
    assert!(expanded.to_string_lossy().contains("Documents"));
}

#[test]
fn config_expand_tilde_absolute_path_unchanged() {
    let path = std::path::Path::new("/absolute/path");
    let expanded = config::expand_tilde(path);
    assert_eq!(expanded, PathBuf::from("/absolute/path"));
}
