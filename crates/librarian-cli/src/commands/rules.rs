//! `librarian rules` -- validate or suggest rules.

use std::path::PathBuf;

use librarian_core::config;

pub async fn validate(rules_path: Option<PathBuf>) -> anyhow::Result<()> {
    let path = rules_path.unwrap_or_else(|| config::librarian_home().join("rules.yaml"));

    if !path.exists() {
        anyhow::bail!(
            "Rules file not found at {}. Run 'librarian init' to create a default rules file.",
            path.display()
        );
    }

    match librarian_rules::load_rules(&path) {
        Ok(rule_set) => {
            println!(
                "Rules valid: {} rule(s) loaded from {}",
                rule_set.rules.len(),
                path.display()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Validation failed: {e}");
            std::process::exit(1);
        }
    }
}

pub async fn suggest() -> anyhow::Result<()> {
    let home = config::librarian_home();
    let corrections_path = home.join("history/corrections.jsonl");
    let rules_path = home.join("rules.yaml");

    // Read corrections
    let corrections = librarian_rules::read_correction_records(&corrections_path)?;

    if corrections.is_empty() {
        println!(
            "No corrections recorded yet. Use `librarian correct` or `librarian review` first."
        );
        return Ok(());
    }

    // Read existing rules YAML (or empty if no rules file yet)
    let existing_rules = if rules_path.exists() {
        std::fs::read_to_string(&rules_path)?
    } else {
        String::new()
    };

    let suggestions = librarian_rules::suggest_rules(&corrections, &existing_rules);

    if suggestions.is_empty() {
        println!(
            "No rule suggestions yet. Need at least 3 corrections with the same pattern \
             (source inbox + filetype + destination folder)."
        );
        println!("Current correction count: {}", corrections.len());
        return Ok(());
    }

    println!("Suggested rules ({} found):\n", suggestions.len());
    println!("# Add these to your rules.yaml:\n");
    println!("rules:");

    for suggestion in &suggestions {
        println!("{}", suggestion.yaml);
        println!("  # Based on {} correction(s)\n", suggestion.pattern_count);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn validate_valid_rules_file() {
        let dir = tempfile::tempdir().unwrap();
        let rules_path = dir.path().join("rules.yaml");
        std::fs::write(
            &rules_path,
            "rules:\n  - name: \"PDFs\"\n    match:\n      extension: \"pdf\"\n    destination: \"Documents\"\n",
        )
        .unwrap();

        let rule_set = librarian_rules::load_rules(&rules_path).unwrap();
        assert_eq!(rule_set.rules.len(), 1);
        assert_eq!(rule_set.rules[0].name, "PDFs");
    }

    #[test]
    fn validate_empty_rules_file() {
        let dir = tempfile::tempdir().unwrap();
        let rules_path = dir.path().join("rules.yaml");
        std::fs::write(&rules_path, "rules: []\n").unwrap();

        let rule_set = librarian_rules::load_rules(&rules_path).unwrap();
        assert!(rule_set.rules.is_empty());
    }

    #[test]
    fn validate_missing_rules_file() {
        let path = PathBuf::from("/nonexistent/rules.yaml");
        assert!(!path.exists());
    }

    #[test]
    fn validate_invalid_yaml_fails() {
        let dir = tempfile::tempdir().unwrap();
        let rules_path = dir.path().join("rules.yaml");
        std::fs::write(&rules_path, "not: valid: yaml: [[[").unwrap();

        let result = librarian_rules::load_rules(&rules_path);
        assert!(result.is_err());
    }

    #[test]
    fn suggest_no_corrections_returns_empty() {
        let corrections = Vec::new();
        let suggestions = librarian_rules::suggest_rules(&corrections, "");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn suggest_with_below_threshold_corrections() {
        // Need at least 3 corrections with the same pattern
        let corrections = vec![
            librarian_rules::CorrectionRecord {
                source_inbox: "Downloads".to_string(),
                filetype: Some("pdf".to_string()),
                corrected_path: PathBuf::from("/dest/Documents/a.pdf"),
            },
            librarian_rules::CorrectionRecord {
                source_inbox: "Downloads".to_string(),
                filetype: Some("pdf".to_string()),
                corrected_path: PathBuf::from("/dest/Documents/b.pdf"),
            },
        ];
        let suggestions = librarian_rules::suggest_rules(&corrections, "");
        assert!(suggestions.is_empty(), "2 corrections should not be enough");
    }

    #[test]
    fn suggest_with_sufficient_corrections() {
        let corrections = vec![
            librarian_rules::CorrectionRecord {
                source_inbox: "Downloads".to_string(),
                filetype: Some("pdf".to_string()),
                corrected_path: PathBuf::from("/dest/Documents/a.pdf"),
            },
            librarian_rules::CorrectionRecord {
                source_inbox: "Downloads".to_string(),
                filetype: Some("pdf".to_string()),
                corrected_path: PathBuf::from("/dest/Documents/b.pdf"),
            },
            librarian_rules::CorrectionRecord {
                source_inbox: "Downloads".to_string(),
                filetype: Some("pdf".to_string()),
                corrected_path: PathBuf::from("/dest/Documents/c.pdf"),
            },
        ];
        let suggestions = librarian_rules::suggest_rules(&corrections, "");
        assert!(
            !suggestions.is_empty(),
            "3 corrections should produce a suggestion"
        );
    }
}
