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
