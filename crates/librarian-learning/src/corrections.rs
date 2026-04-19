//! Correction recording

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use librarian_core::decision::{Decision, DecisionOutcome, DecisionType, append_decision};

/// How the correction was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionSource {
    /// Detected by the filesystem watcher (user moved a file manually).
    Watched,
    /// Recorded via `librarian correct` CLI command.
    Explicit,
    /// Accepted during `librarian review` interactive session.
    Review,
}

/// A single correction record: the user disagreed with a placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub original_path: PathBuf,
    pub corrected_path: PathBuf,
    pub file_hash: String,
    pub source: CorrectionSource,
    pub corrected_tags: Option<Vec<String>>,
    pub timestamp: DateTime<Utc>,
    pub source_inbox: String,
    pub filetype: Option<String>,
}

/// Append a correction to BOTH corrections.jsonl AND decisions.jsonl.
///
/// The correction is logged to the corrections file for the learning layer,
/// and also recorded as a `Decision` with type `Correction` in the main
/// decision log for audit purposes.
pub fn record_correction(
    corrections_path: &Path,
    decisions_path: &Path,
    correction: &Correction,
) -> anyhow::Result<()> {
    // 1. Append to corrections.jsonl
    if let Some(parent) = corrections_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(corrections_path)?;
    let line = serde_json::to_string(correction)?;
    writeln!(file, "{}", line)?;
    file.flush()?;

    // 2. Append to decisions.jsonl as a Correction decision
    let action = format!(
        "corrected from {} to {}",
        correction.original_path.display(),
        correction.corrected_path.display()
    );
    let decision = Decision::new(
        DecisionType::Correction,
        &correction.file_hash,
        correction.corrected_path.clone(),
        &action,
        DecisionOutcome::Corrected,
    );
    append_decision(decisions_path, &decision)?;

    Ok(())
}

/// Read all corrections from a JSONL file.
pub fn read_corrections(path: &Path) -> anyhow::Result<Vec<Correction>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path)?;
    let mut corrections = Vec::new();
    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let correction: Correction = serde_json::from_str(line)?;
        corrections.push(correction);
    }
    Ok(corrections)
}

/// Check whether a placement is still within the correction window.
///
/// Returns `true` if `placement_time` is less than `window_days` ago.
pub fn is_within_correction_window(placement_time: DateTime<Utc>, window_days: u32) -> bool {
    let now = Utc::now();
    let duration = now.signed_duration_since(placement_time);
    duration.num_days() < i64::from(window_days)
}

/// Record a post-correction-window move as a reorganisation in decisions.jsonl.
pub fn record_reorganisation(
    decisions_path: &Path,
    file_hash: &str,
    from: &Path,
    to: &Path,
) -> anyhow::Result<()> {
    let action = format!("reorganised from {} to {}", from.display(), to.display());
    let decision = Decision::new(
        DecisionType::Reorganisation,
        file_hash,
        to.to_path_buf(),
        &action,
        DecisionOutcome::Success,
    );
    append_decision(decisions_path, &decision)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_correction(source: CorrectionSource) -> Correction {
        Correction {
            original_path: PathBuf::from("/managed/Work/report.pdf"),
            corrected_path: PathBuf::from("/managed/Personal/report.pdf"),
            file_hash: "abc123".to_string(),
            source,
            corrected_tags: Some(vec!["personal".to_string()]),
            timestamp: Utc::now(),
            source_inbox: "Downloads".to_string(),
            filetype: Some("pdf".to_string()),
        }
    }

    #[test]
    fn record_and_read_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let corrections_path = dir.path().join("corrections.jsonl");
        let decisions_path = dir.path().join("decisions.jsonl");

        let c1 = make_correction(CorrectionSource::Explicit);
        let c2 = make_correction(CorrectionSource::Watched);

        record_correction(&corrections_path, &decisions_path, &c1).unwrap();
        record_correction(&corrections_path, &decisions_path, &c2).unwrap();

        let corrections = read_corrections(&corrections_path).unwrap();
        assert_eq!(corrections.len(), 2);
        assert_eq!(corrections[0].file_hash, "abc123");
        assert_eq!(corrections[1].source, CorrectionSource::Watched);

        // Verify decisions were also written
        let decisions = librarian_core::decision::read_decisions(&decisions_path).unwrap();
        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0].decision_type, DecisionType::Correction);
    }

    #[test]
    fn correction_window_within() {
        let recent = Utc::now() - Duration::days(5);
        assert!(is_within_correction_window(recent, 14));
    }

    #[test]
    fn correction_window_outside() {
        let old = Utc::now() - Duration::days(20);
        assert!(!is_within_correction_window(old, 14));
    }

    #[test]
    fn reorganisation_logging() {
        let dir = tempfile::tempdir().unwrap();
        let decisions_path = dir.path().join("decisions.jsonl");

        record_reorganisation(
            &decisions_path,
            "hash456",
            Path::new("/old/path.pdf"),
            Path::new("/new/path.pdf"),
        )
        .unwrap();

        let decisions = librarian_core::decision::read_decisions(&decisions_path).unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].decision_type, DecisionType::Reorganisation);
        assert!(decisions[0].action.contains("reorganised"));
    }

    #[test]
    fn three_correction_sources_serialise() {
        let sources = [
            CorrectionSource::Watched,
            CorrectionSource::Explicit,
            CorrectionSource::Review,
        ];

        for source in sources {
            let c = make_correction(source);
            let json = serde_json::to_string(&c).unwrap();
            let restored: Correction = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.source, source);
        }
    }

    #[test]
    fn read_nonexistent_returns_empty() {
        let corrections = read_corrections(Path::new("/nonexistent.jsonl")).unwrap();
        assert!(corrections.is_empty());
    }

    #[test]
    fn correction_window_zero_days() {
        let recent = Utc::now();
        assert!(!is_within_correction_window(recent, 0));
    }

    #[test]
    fn correction_window_boundary() {
        // Exactly window_days ago — uses < not <=, so this should be false
        let exactly = Utc::now() - Duration::days(14);
        assert!(!is_within_correction_window(exactly, 14));
    }

    #[test]
    fn record_correction_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let corrections_path = dir.path().join("nested/a/corrections.jsonl");
        let decisions_path = dir.path().join("nested/b/decisions.jsonl");

        let c = make_correction(CorrectionSource::Explicit);
        record_correction(&corrections_path, &decisions_path, &c).unwrap();

        let corrections = read_corrections(&corrections_path).unwrap();
        assert_eq!(corrections.len(), 1);
    }

    #[test]
    fn correction_with_no_tags_round_trips() {
        let mut c = make_correction(CorrectionSource::Review);
        c.corrected_tags = None;

        let json = serde_json::to_string(&c).unwrap();
        let restored: Correction = serde_json::from_str(&json).unwrap();
        assert!(restored.corrected_tags.is_none());
    }

    #[test]
    fn read_malformed_corrections_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.jsonl");
        std::fs::write(&path, "not valid json\n").unwrap();

        let result = read_corrections(&path);
        assert!(result.is_err());
    }

    #[test]
    fn read_corrections_skips_blank_lines() {
        let dir = tempfile::tempdir().unwrap();
        let corrections_path = dir.path().join("blanks.jsonl");
        let decisions_path = dir.path().join("decisions.jsonl");

        let c = make_correction(CorrectionSource::Explicit);
        record_correction(&corrections_path, &decisions_path, &c).unwrap();

        // Insert blank lines
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&corrections_path)
            .unwrap();
        writeln!(f).unwrap();
        writeln!(f, "   ").unwrap();

        let c2 = make_correction(CorrectionSource::Watched);
        record_correction(&corrections_path, &decisions_path, &c2).unwrap();

        let corrections = read_corrections(&corrections_path).unwrap();
        assert_eq!(corrections.len(), 2);
    }
}
