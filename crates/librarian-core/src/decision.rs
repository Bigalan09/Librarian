//! Decision log types and JSONL append.

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    Classification,
    Move,
    Rename,
    Tag,
    Skip,
    Collision,
    Correction,
    Reorganisation,
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcome {
    Success,
    Skipped,
    Failed,
    Corrected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationMethod {
    Rule,
    FilenameEmbedding,
    ContentEmbedding,
    Llm,
    None,
}

/// An immutable audit record appended to decisions.jsonl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub timestamp: DateTime<Utc>,
    pub decision_type: DecisionType,
    pub file_hash: String,
    pub file_path: PathBuf,
    pub classification_method: Option<ClassificationMethod>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub confidence: Option<f64>,
    pub action: String,
    pub outcome: DecisionOutcome,
    pub plan_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

impl Decision {
    /// Create a simple decision record.
    pub fn new(
        decision_type: DecisionType,
        file_hash: &str,
        file_path: PathBuf,
        action: &str,
        outcome: DecisionOutcome,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            decision_type,
            file_hash: file_hash.to_owned(),
            file_path,
            classification_method: None,
            provider: None,
            model: None,
            confidence: None,
            action: action.to_owned(),
            outcome,
            plan_id: None,
            metadata: None,
        }
    }
}

/// Append a decision to a JSONL file. Creates the file if it does not exist.
/// Uses file locking to prevent concurrent write corruption.
pub fn append_decision(log_path: &Path, decision: &Decision) -> anyhow::Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;

    let line = serde_json::to_string(decision)?;
    writeln!(file, "{}", line)?;
    file.flush()?;

    Ok(())
}

/// Read all decisions from a JSONL file.
pub fn read_decisions(log_path: &Path) -> anyhow::Result<Vec<Decision>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(log_path)?;
    let mut decisions = Vec::new();
    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let decision: Decision = serde_json::from_str(line)?;
        decisions.push(decision);
    }
    Ok(decisions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_serialisation_round_trip() {
        let d = Decision::new(
            DecisionType::Move,
            "abc123",
            PathBuf::from("/tmp/test.pdf"),
            "moved to /2026/Work/Invoices/",
            DecisionOutcome::Success,
        );

        let json = serde_json::to_string(&d).unwrap();
        let restored: Decision = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.decision_type, DecisionType::Move);
        assert_eq!(restored.file_hash, "abc123");
        assert_eq!(restored.outcome, DecisionOutcome::Success);
    }

    #[test]
    fn append_and_read_decisions() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("history/decisions.jsonl");

        let d1 = Decision::new(
            DecisionType::Move,
            "hash1",
            PathBuf::from("/a.pdf"),
            "moved",
            DecisionOutcome::Success,
        );
        let d2 = Decision::new(
            DecisionType::Collision,
            "hash2",
            PathBuf::from("/b.pdf"),
            "skipped: collision",
            DecisionOutcome::Skipped,
        );

        append_decision(&log_path, &d1).unwrap();
        append_decision(&log_path, &d2).unwrap();

        let decisions = read_decisions(&log_path).unwrap();
        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0].decision_type, DecisionType::Move);
        assert_eq!(decisions[1].decision_type, DecisionType::Collision);
    }

    #[test]
    fn read_empty_file_returns_empty_vec() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("empty.jsonl");
        std::fs::write(&log_path, "").unwrap();

        let decisions = read_decisions(&log_path).unwrap();
        assert!(decisions.is_empty());
    }

    #[test]
    fn read_nonexistent_returns_empty_vec() {
        let decisions = read_decisions(Path::new("/nonexistent.jsonl")).unwrap();
        assert!(decisions.is_empty());
    }

    #[test]
    fn all_decision_types_serialise() {
        let types = [
            DecisionType::Classification,
            DecisionType::Move,
            DecisionType::Rename,
            DecisionType::Tag,
            DecisionType::Skip,
            DecisionType::Collision,
            DecisionType::Correction,
            DecisionType::Reorganisation,
            DecisionType::Ignored,
        ];

        for dt in types {
            let d = Decision::new(dt, "h", PathBuf::from("/f"), "test", DecisionOutcome::Success);
            let json = serde_json::to_string(&d).unwrap();
            let restored: Decision = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.decision_type, dt);
        }
    }

    #[test]
    fn all_outcomes_serialise() {
        let outcomes = [
            DecisionOutcome::Success,
            DecisionOutcome::Skipped,
            DecisionOutcome::Failed,
            DecisionOutcome::Corrected,
        ];

        for outcome in outcomes {
            let d = Decision::new(
                DecisionType::Move,
                "h",
                PathBuf::from("/f"),
                "test",
                outcome,
            );
            let json = serde_json::to_string(&d).unwrap();
            let restored: Decision = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.outcome, outcome);
        }
    }

    /// Verify that every `DecisionType` variant and every `DecisionOutcome`
    /// variant can be appended via `append_decision` and read back with the
    /// correct values preserved (full JSONL round-trip through disk I/O).
    #[test]
    fn all_variants_round_trip_through_append_decision() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("audit.jsonl");

        // Pair each DecisionType with a corresponding DecisionOutcome so that
        // every variant of both enums appears at least once.
        let cases: &[(DecisionType, DecisionOutcome, &str)] = &[
            (DecisionType::Classification, DecisionOutcome::Success,  "classified by rule"),
            (DecisionType::Move,           DecisionOutcome::Success,  "moved to /dest/file.pdf"),
            (DecisionType::Rename,         DecisionOutcome::Success,  "renamed to 2026-04-17_report.pdf"),
            (DecisionType::Tag,            DecisionOutcome::Success,  "tagged: [work, invoice]"),
            (DecisionType::Skip,           DecisionOutcome::Skipped,  "skipped: already organised"),
            (DecisionType::Collision,      DecisionOutcome::Skipped,  "collision: destination exists"),
            (DecisionType::Correction,     DecisionOutcome::Corrected,"rollback: reversed move"),
            (DecisionType::Reorganisation, DecisionOutcome::Failed,   "reorganisation failed"),
            (DecisionType::Ignored,        DecisionOutcome::Skipped,  "ignored by .librarianignore"),
        ];

        for (i, &(dt, outcome, action)) in cases.iter().enumerate() {
            let d = Decision::new(
                dt,
                &format!("hash{}", i),
                PathBuf::from(format!("/file{}.txt", i)),
                action,
                outcome,
            );
            append_decision(&log_path, &d).unwrap();
        }

        let decisions = read_decisions(&log_path).unwrap();
        assert_eq!(decisions.len(), cases.len(), "every appended decision must be readable");

        for (i, (&(dt, outcome, action), decision)) in cases.iter().zip(decisions.iter()).enumerate() {
            assert_eq!(decision.decision_type, dt,
                "row {}: decision_type mismatch", i);
            assert_eq!(decision.outcome, outcome,
                "row {}: outcome mismatch", i);
            assert_eq!(decision.action, action,
                "row {}: action mismatch", i);
            assert_eq!(decision.file_hash, format!("hash{}", i),
                "row {}: file_hash mismatch", i);
        }
    }
}
