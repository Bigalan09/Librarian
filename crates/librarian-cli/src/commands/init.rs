//! `librarian init` — scaffold configuration and folder structure.

use std::path::Path;

use librarian_core::config::librarian_home;

const DEFAULT_CONFIG: &str = r#"# Librarian configuration
# See docs for all options.

inbox_folders:
  - ~/Downloads
  - ~/Desktop

destination_root: ~/Library-Managed
needs_review_path: ~/Library-Managed/NeedsReview
trash_path: ~/Library-Managed/_Trash

provider:
  provider_type: lmstudio
  base_url: "http://localhost:1234/v1"
  # api_key: null            # Set for OpenAI
  # llm_model: null          # Override per-provider default
  # embed_model: null
  # rate_limit_rpm: 20       # OpenAI only

thresholds:
  filename_embedding: 0.80
  content_embedding: 0.75
  llm_confidence: 0.70

correction_window_days: 14
max_moves_per_run: 500
fewshot_count: 20
rule_suggestion_threshold: 3
"#;

const DEFAULT_RULES: &str = r#"# Librarian rules
# Rules are evaluated in order. First match wins.
# Patterns use glob syntax by default. Prefix with 'regex:' for regex.

rules:
  # Example: match PDF invoices
  # - name: "Work invoices"
  #   match:
  #     extension: "pdf"
  #     filename: "*invoice*"
  #   destination: "{year}/Work/Invoices"
  #   tags: ["invoice", "work"]

  # Example: match screenshots with regex
  # - name: "Screenshots"
  #   match:
  #     filename: "regex:^Screenshot \\d{4}-\\d{2}-\\d{2}"
  #     extension: "png"
  #   destination: "{year}/Personal/Screenshots"
  #   tags: ["screenshot"]
  #   clean_name: true
"#;

const DEFAULT_IGNORE: &str = r#"# Global ignore patterns (gitignore syntax)
# These patterns apply to all scans.

# Common junk
*.tmp
*.swp
*.crdownload
*.part
"#;

pub async fn run() -> anyhow::Result<()> {
    let home = librarian_home();

    let dirs = [
        home.clone(),
        home.join("plans"),
        home.join("history"),
        home.join("cache"),
        home.join("backup"),
        home.join("state"),
        home.join("logs"),
    ];

    for dir in &dirs {
        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
            println!("  Created {}", dir.display());
        }
    }

    write_if_missing(&home.join("config.yaml"), DEFAULT_CONFIG)?;
    write_if_missing(&home.join("rules.yaml"), DEFAULT_RULES)?;
    write_if_missing(&home.join("ignore"), DEFAULT_IGNORE)?;

    println!("\nLibrarian initialised at {}", home.display());
    println!("Edit config:  librarian config edit");
    println!("Add rules:    edit {}", home.join("rules.yaml").display());
    println!("First run:    librarian process --source ~/Downloads");

    Ok(())
}

fn write_if_missing(path: &Path, content: &str) -> anyhow::Result<()> {
    if path.exists() {
        println!("  Skipped {} (already exists)", path.display());
    } else {
        std::fs::write(path, content)?;
        println!("  Created {}", path.display());
    }
    Ok(())
}
