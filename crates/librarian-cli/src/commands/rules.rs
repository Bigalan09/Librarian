//! `librarian rules` — validate or suggest rules.

use std::path::PathBuf;

use librarian_core::config;

pub async fn validate(rules_path: Option<PathBuf>) -> anyhow::Result<()> {
    let path = rules_path.unwrap_or_else(|| config::librarian_home().join("rules.yaml"));

    if !path.exists() {
        anyhow::bail!("Rules file not found: {}", path.display());
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
    tracing::info!("librarian rules suggest: not yet implemented (requires US3 learning layer)");
    println!("No rule suggestions available yet. Corrections must be recorded first.");
    Ok(())
}
