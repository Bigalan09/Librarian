//! Auto-generated rule proposals

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Minimal correction record for deserialization within librarian-rules.
///
/// This avoids a circular dependency on librarian-learning. The struct matches
/// the fields we need from corrections.jsonl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionRecord {
    pub source_inbox: String,
    pub filetype: Option<String>,
    pub corrected_path: PathBuf,
}

/// A suggested rule generated from correction patterns.
#[derive(Debug, Clone)]
pub struct SuggestedRule {
    pub name: String,
    pub yaml: String,
    pub pattern_count: usize,
}

/// Suggest new rules based on correction patterns.
///
/// Scans corrections for repeated patterns (same `source_inbox`, same `filetype`,
/// same corrected destination folder). When a pattern appears 3 or more times a
/// rule is suggested.
///
/// `existing_rules_yaml` is the content of the current rules file, used to
/// avoid suggesting duplicates.
pub fn suggest_rules(
    corrections: &[CorrectionRecord],
    existing_rules_yaml: &str,
) -> Vec<SuggestedRule> {
    // Count patterns: (source_inbox, filetype, destination_folder) -> count
    let mut pattern_counts: HashMap<(String, Option<String>, String), usize> = HashMap::new();

    for c in corrections {
        let dest_folder = c
            .corrected_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let key = (c.source_inbox.clone(), c.filetype.clone(), dest_folder);
        *pattern_counts.entry(key).or_insert(0) += 1;
    }

    let mut suggestions = Vec::new();

    for ((source_inbox, filetype, dest_folder), count) in &pattern_counts {
        if *count < 3 {
            continue;
        }

        let rule_name = generate_rule_name(source_inbox, filetype.as_deref(), dest_folder);

        // Check if a similar rule already exists
        if existing_rules_yaml.contains(dest_folder.as_str()) {
            if let Some(ft) = filetype {
                if existing_rules_yaml.contains(ft) {
                    continue;
                }
            } else {
                continue;
            }
        }

        let yaml = generate_rule_yaml(&rule_name, filetype.as_deref(), dest_folder);

        suggestions.push(SuggestedRule {
            name: rule_name,
            yaml,
            pattern_count: *count,
        });
    }

    // Sort by pattern count descending for deterministic output
    suggestions.sort_by(|a, b| b.pattern_count.cmp(&a.pattern_count).then(a.name.cmp(&b.name)));

    suggestions
}

/// Read correction records directly from a JSONL file.
///
/// This reads only the fields needed for suggestion, avoiding a dependency
/// on the full Correction type from librarian-learning.
pub fn read_correction_records(path: &Path) -> anyhow::Result<Vec<CorrectionRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path)?;
    let mut records = Vec::new();
    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let record: CorrectionRecord = serde_json::from_str(line)?;
        records.push(record);
    }
    Ok(records)
}

/// Generate a human-readable rule name from the pattern.
fn generate_rule_name(source_inbox: &str, filetype: Option<&str>, dest_folder: &str) -> String {
    let folder_name = Path::new(dest_folder)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| dest_folder.to_string());

    match filetype {
        Some(ft) => format!("Auto: {} from {} to {}", ft.to_uppercase(), source_inbox, folder_name),
        None => format!("Auto: {} to {}", source_inbox, folder_name),
    }
}

/// Generate YAML for a suggested rule.
fn generate_rule_yaml(name: &str, filetype: Option<&str>, dest_folder: &str) -> String {
    let mut yaml = format!("  - name: \"{}\"\n", name);
    yaml.push_str("    match:\n");

    if let Some(ft) = filetype {
        yaml.push_str(&format!("      extension: \"{}\"\n", ft));
    }

    yaml.push_str(&format!("    destination: \"{}\"\n", dest_folder));

    yaml
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(inbox: &str, filetype: Option<&str>, corrected: &str) -> CorrectionRecord {
        CorrectionRecord {
            source_inbox: inbox.to_string(),
            filetype: filetype.map(|s| s.to_string()),
            corrected_path: PathBuf::from(corrected),
        }
    }

    #[test]
    fn three_identical_corrections_produce_one_suggestion() {
        let corrections = vec![
            make_record("Downloads", Some("pdf"), "/managed/Invoices/a.pdf"),
            make_record("Downloads", Some("pdf"), "/managed/Invoices/b.pdf"),
            make_record("Downloads", Some("pdf"), "/managed/Invoices/c.pdf"),
        ];

        let suggestions = suggest_rules(&corrections, "rules: []");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].pattern_count, 3);
        assert!(suggestions[0].yaml.contains("pdf"));
        assert!(suggestions[0].yaml.contains("Invoices"));
    }

    #[test]
    fn two_corrections_not_enough() {
        let corrections = vec![
            make_record("Downloads", Some("pdf"), "/managed/Invoices/a.pdf"),
            make_record("Downloads", Some("pdf"), "/managed/Invoices/b.pdf"),
        ];

        let suggestions = suggest_rules(&corrections, "rules: []");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn different_source_inbox_dont_combine() {
        let corrections = vec![
            make_record("Downloads", Some("pdf"), "/managed/Invoices/a.pdf"),
            make_record("Downloads", Some("pdf"), "/managed/Invoices/b.pdf"),
            make_record("Desktop", Some("pdf"), "/managed/Invoices/c.pdf"),
        ];

        let suggestions = suggest_rules(&corrections, "rules: []");
        // Neither group reaches 3
        assert!(suggestions.is_empty());
    }

    #[test]
    fn generated_yaml_is_valid_yaml() {
        let corrections = vec![
            make_record("Downloads", Some("pdf"), "/managed/Invoices/a.pdf"),
            make_record("Downloads", Some("pdf"), "/managed/Invoices/b.pdf"),
            make_record("Downloads", Some("pdf"), "/managed/Invoices/c.pdf"),
        ];

        let suggestions = suggest_rules(&corrections, "rules: []");
        assert_eq!(suggestions.len(), 1);

        // The YAML fragment should be parseable within a rules context
        let full_yaml = format!("rules:\n{}", suggestions[0].yaml);
        let parsed: serde_yaml::Value = serde_yaml::from_str(&full_yaml).unwrap();
        assert!(parsed["rules"].is_sequence());
    }

    #[test]
    fn duplicate_rule_not_suggested() {
        let corrections = vec![
            make_record("Downloads", Some("pdf"), "/managed/Invoices/a.pdf"),
            make_record("Downloads", Some("pdf"), "/managed/Invoices/b.pdf"),
            make_record("Downloads", Some("pdf"), "/managed/Invoices/c.pdf"),
        ];

        // Existing rules already contain this pattern
        let existing = r#"
rules:
  - name: "PDF Invoices"
    match:
      extension: "pdf"
    destination: "/managed/Invoices"
"#;

        let suggestions = suggest_rules(&corrections, existing);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn no_filetype_pattern() {
        let corrections = vec![
            make_record("Downloads", None, "/managed/Misc/a.bin"),
            make_record("Downloads", None, "/managed/Misc/b.bin"),
            make_record("Downloads", None, "/managed/Misc/c.bin"),
        ];

        let suggestions = suggest_rules(&corrections, "rules: []");
        assert_eq!(suggestions.len(), 1);
        assert!(!suggestions[0].yaml.contains("extension"));
    }
}
