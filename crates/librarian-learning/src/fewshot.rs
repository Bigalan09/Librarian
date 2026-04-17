//! Few-shot example selection

use std::path::Path;

use crate::corrections::{read_corrections, Correction};

/// Select few-shot prompt examples from correction history.
///
/// Scans corrections.jsonl, filters by `source_inbox` AND optionally `filetype`,
/// selects the last `max_count` by timestamp, and formats as prompt strings.
///
/// Per-folder isolation: Downloads corrections do NOT appear for Desktop queries.
pub fn select_examples(
    corrections_path: &Path,
    source_inbox: &str,
    filetype: Option<&str>,
    max_count: usize,
) -> anyhow::Result<Vec<String>> {
    let corrections = read_corrections(corrections_path)?;

    let mut filtered: Vec<&Correction> = corrections
        .iter()
        .filter(|c| c.source_inbox == source_inbox)
        .filter(|c| match filetype {
            Some(ft) => c.filetype.as_deref() == Some(ft),
            None => true,
        })
        .collect();

    // Sort by timestamp descending, take last N (most recent)
    filtered.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    let selected: Vec<&Correction> = if filtered.len() > max_count {
        filtered[filtered.len() - max_count..].to_vec()
    } else {
        filtered
    };

    let examples: Vec<String> = selected
        .iter()
        .map(|c| {
            let filename = c
                .original_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| c.original_path.display().to_string());
            format!(
                "You previously placed {} into {}. The user moved it to {}. Learn from this.",
                filename,
                c.original_path.display(),
                c.corrected_path.display()
            )
        })
        .collect();

    Ok(examples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corrections::{record_correction, Correction, CorrectionSource};
    use chrono::{Duration, Utc};
    use std::path::PathBuf;

    fn make_correction(
        source_inbox: &str,
        filetype: Option<&str>,
        original: &str,
        corrected: &str,
        age_days: i64,
    ) -> Correction {
        Correction {
            original_path: PathBuf::from(original),
            corrected_path: PathBuf::from(corrected),
            file_hash: format!("hash_{}", original),
            source: CorrectionSource::Explicit,
            corrected_tags: None,
            timestamp: Utc::now() - Duration::days(age_days),
            source_inbox: source_inbox.to_string(),
            filetype: filetype.map(|s| s.to_string()),
        }
    }

    fn setup_corrections(dir: &std::path::Path) -> PathBuf {
        let corrections_path = dir.join("corrections.jsonl");
        let decisions_path = dir.join("decisions.jsonl");

        let corrections = vec![
            make_correction("Downloads", Some("pdf"), "/managed/Work/a.pdf", "/managed/Personal/a.pdf", 5),
            make_correction("Downloads", Some("pdf"), "/managed/Work/b.pdf", "/managed/Personal/b.pdf", 3),
            make_correction("Downloads", Some("txt"), "/managed/Work/c.txt", "/managed/Docs/c.txt", 2),
            make_correction("Desktop", Some("pdf"), "/managed/Work/d.pdf", "/managed/Archive/d.pdf", 1),
        ];

        for c in &corrections {
            record_correction(&corrections_path, &decisions_path, c).unwrap();
        }

        corrections_path
    }

    #[test]
    fn filter_by_source_inbox() {
        let dir = tempfile::tempdir().unwrap();
        let corrections_path = setup_corrections(dir.path());

        let examples = select_examples(&corrections_path, "Downloads", None, 10).unwrap();
        assert_eq!(examples.len(), 3);

        let examples = select_examples(&corrections_path, "Desktop", None, 10).unwrap();
        assert_eq!(examples.len(), 1);
    }

    #[test]
    fn filter_by_filetype() {
        let dir = tempfile::tempdir().unwrap();
        let corrections_path = setup_corrections(dir.path());

        let examples = select_examples(&corrections_path, "Downloads", Some("pdf"), 10).unwrap();
        assert_eq!(examples.len(), 2);
    }

    #[test]
    fn max_count_limit() {
        let dir = tempfile::tempdir().unwrap();
        let corrections_path = setup_corrections(dir.path());

        let examples = select_examples(&corrections_path, "Downloads", None, 2).unwrap();
        assert_eq!(examples.len(), 2);
    }

    #[test]
    fn formatting() {
        let dir = tempfile::tempdir().unwrap();
        let corrections_path = setup_corrections(dir.path());

        let examples = select_examples(&corrections_path, "Desktop", Some("pdf"), 10).unwrap();
        assert_eq!(examples.len(), 1);
        assert!(examples[0].contains("You previously placed"));
        assert!(examples[0].contains("The user moved it to"));
        assert!(examples[0].contains("Learn from this."));
    }

    #[test]
    fn empty_corrections_returns_empty_vec() {
        let dir = tempfile::tempdir().unwrap();
        let corrections_path = dir.path().join("corrections.jsonl");

        let examples = select_examples(&corrections_path, "Downloads", None, 10).unwrap();
        assert!(examples.is_empty());
    }

    #[test]
    fn isolation_between_inboxes() {
        let dir = tempfile::tempdir().unwrap();
        let corrections_path = setup_corrections(dir.path());

        let downloads = select_examples(&corrections_path, "Downloads", Some("pdf"), 10).unwrap();
        let desktop = select_examples(&corrections_path, "Desktop", Some("pdf"), 10).unwrap();

        // Downloads corrections should NOT appear in Desktop results
        assert_eq!(downloads.len(), 2);
        assert_eq!(desktop.len(), 1);

        // Verify they contain different files
        for ex in &downloads {
            assert!(!ex.contains("d.pdf"));
        }
        for ex in &desktop {
            assert!(ex.contains("d.pdf"));
        }
    }
}
