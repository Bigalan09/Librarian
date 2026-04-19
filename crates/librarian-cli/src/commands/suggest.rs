//! `librarian suggest-structure` — AI-driven folder hierarchy and rules generation.
//!
//! Scans inbox folders, builds a file inventory, sends it to the LLM, and
//! proposes a destination folder hierarchy plus initial rules.yaml entries.

use std::path::PathBuf;

use librarian_core::IgnoreEngine;
use librarian_core::config::{self, ProviderType};
use librarian_core::file_entry::FileEntry;
use librarian_core::walker;
use librarian_providers::router::ErasedProvider;
use librarian_providers::traits::ChatMessage;

/// Proposed folder structure from the LLM.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StructureSuggestion {
    pub folders: Vec<FolderSuggestion>,
    pub rules: Vec<RuleSuggestion>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FolderSuggestion {
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RuleSuggestion {
    pub name: String,
    pub match_field: String,
    pub match_pattern: String,
    pub destination: String,
    pub tags: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    source: Vec<PathBuf>,
    destination: Option<PathBuf>,
    provider: Option<String>,
    llm_model: Option<String>,
    embed_model: Option<String>,
    max_files: Option<usize>,
    apply_folders: bool,
    apply_rules: bool,
) -> anyhow::Result<()> {
    let mut cfg = config::load_default()?;

    if let Some(p) = &provider {
        cfg.provider.provider_type = match p.as_str() {
            "openai" => ProviderType::OpenAi,
            _ => ProviderType::LmStudio,
        };
    }
    if let Some(m) = llm_model {
        cfg.provider.llm_model = Some(m);
    }
    if let Some(m) = embed_model {
        cfg.provider.embed_model = Some(m);
    }

    let sources = if source.is_empty() {
        cfg.inbox_folders.clone()
    } else {
        source
    };
    let dest_root = destination.unwrap_or_else(|| cfg.destination_root.clone());
    let max = max_files.unwrap_or(200);

    // Connect to AI provider
    let router = librarian_providers::router::ProviderRouter::new(&cfg.provider).await?;
    let provider = router.active()?;

    // Scan source folders to build a file inventory
    let mut all_entries = Vec::new();
    for src in &sources {
        if !src.exists() {
            tracing::warn!("source folder does not exist: {}", src.display());
            continue;
        }
        let inbox_name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_owned());

        let ignore_engine = IgnoreEngine::new(src, None)?;
        let entries = walker::scan_directory(src, &inbox_name, &ignore_engine, max).await?;
        all_entries.extend(entries);
    }

    if all_entries.is_empty() {
        println!("No files found in inbox folders. Nothing to suggest.");
        return Ok(());
    }

    // Truncate to max files
    if all_entries.len() > max {
        all_entries.truncate(max);
    }

    // Discover existing folders
    let existing_folders = discover_existing_folders(&dest_root);

    println!(
        "Analysing {} files from {} source(s)...\n",
        all_entries.len(),
        sources.len()
    );

    // Build and send the prompt
    let suggestion = request_structure(provider, &all_entries, &existing_folders).await?;

    // Display results
    println!("Suggested folder structure:");
    println!("==========================\n");
    for folder in &suggestion.folders {
        println!("  {}/", folder.path);
        println!("    {}\n", folder.description);
    }

    if !suggestion.rules.is_empty() {
        println!("\nSuggested rules:");
        println!("================\n");
        let yaml = format_rules_yaml(&suggestion.rules);
        println!("{yaml}");
    }

    // Optionally create the folder structure
    if apply_folders {
        println!("\nCreating folder structure...");
        for folder in &suggestion.folders {
            let path = dest_root.join(&folder.path);
            if !path.exists() {
                std::fs::create_dir_all(&path)?;
                println!("  Created {}", path.display());
            } else {
                println!("  Exists  {}", path.display());
            }
        }
        println!("Done.");
    }

    // Optionally append rules to rules.yaml
    if apply_rules && !suggestion.rules.is_empty() {
        let rules_path = config::librarian_home().join("rules.yaml");
        let yaml = format_rules_yaml(&suggestion.rules);

        if rules_path.exists() {
            let existing = std::fs::read_to_string(&rules_path)?;
            // Append new rules
            let mut new_content = existing;
            if !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            new_content.push_str("\n# AI-suggested rules\n");
            // Strip the leading "rules:\n" since we're appending to existing
            for line in yaml.lines() {
                if line.trim() == "rules:" {
                    continue;
                }
                new_content.push_str(line);
                new_content.push('\n');
            }
            std::fs::write(&rules_path, new_content)?;
        } else {
            std::fs::write(&rules_path, &yaml)?;
        }
        println!("\nRules written to {}", rules_path.display());
    }

    if !apply_folders && !apply_rules {
        println!("\nTo apply:");
        println!("  librarian suggest-structure --apply-folders   # create directories");
        println!("  librarian suggest-structure --apply-rules     # write to rules.yaml");
    }

    Ok(())
}

fn discover_existing_folders(dest_root: &std::path::Path) -> Vec<String> {
    let mut folders = Vec::new();
    if !dest_root.exists() {
        return folders;
    }
    collect_folders_recursive(dest_root, dest_root, &mut folders, 0, 3);
    folders.sort();
    folders
}

fn collect_folders_recursive(
    root: &std::path::Path,
    current: &std::path::Path,
    folders: &mut Vec<String>,
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && !name.starts_with('.')
            && !name.starts_with('_')
        {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            folders.push(relative.to_string_lossy().to_string());
            collect_folders_recursive(root, &path, folders, depth + 1, max_depth);
        }
    }
}

/// Build the file inventory summary for the LLM prompt.
fn build_inventory(entries: &[FileEntry]) -> String {
    use std::collections::HashMap;

    // Group by extension
    let mut by_ext: HashMap<String, Vec<&FileEntry>> = HashMap::new();
    for entry in entries {
        let ext = entry
            .extension
            .as_deref()
            .unwrap_or("(no extension)")
            .to_string();
        by_ext.entry(ext).or_default().push(entry);
    }

    let mut lines = Vec::new();
    lines.push(format!("Total files: {}", entries.len()));
    lines.push(String::new());

    // Extension breakdown
    let mut ext_counts: Vec<(String, usize)> = by_ext
        .iter()
        .map(|(ext, files)| (ext.clone(), files.len()))
        .collect();
    ext_counts.sort_by_key(|e| std::cmp::Reverse(e.1));

    lines.push("File types:".to_string());
    for (ext, count) in &ext_counts {
        lines.push(format!("  .{ext}: {count} file(s)"));
    }
    lines.push(String::new());

    // Sample filenames (up to 5 per extension)
    lines.push("Sample filenames by type:".to_string());
    for (ext, files) in &by_ext {
        let samples: Vec<&str> = files.iter().take(5).map(|f| f.name.as_str()).collect();
        lines.push(format!("  .{ext}: {}", samples.join(", ")));
    }
    lines.push(String::new());

    // Source inboxes
    let mut inboxes: Vec<String> = entries.iter().map(|e| e.source_inbox.clone()).collect();
    inboxes.sort();
    inboxes.dedup();
    lines.push(format!("Source inboxes: {}", inboxes.join(", ")));

    lines.join("\n")
}

async fn request_structure(
    provider: &dyn ErasedProvider,
    entries: &[FileEntry],
    existing_folders: &[String],
) -> anyhow::Result<StructureSuggestion> {
    let inventory = build_inventory(entries);

    let mut system = String::from(
        "You are a file organisation expert. Your task is to analyse a collection of files \
         and propose an optimal folder hierarchy for organising them.\n\n\
         You MUST respond with valid JSON only, using this exact format:\n\
         {\n\
         \"folders\": [\n\
           {\"path\": \"Work/Documents\", \"description\": \"Work-related documents and reports\"},\n\
           {\"path\": \"Personal/Photos\", \"description\": \"Personal photographs\"}\n\
         ],\n\
         \"rules\": [\n\
           {\"name\": \"PDF invoices\", \"match_field\": \"extension\", \"match_pattern\": \"pdf\", \
            \"destination\": \"Work/Invoices\", \"tags\": [\"invoice\", \"work\"]},\n\
           {\"name\": \"Screenshots\", \"match_field\": \"filename\", \"match_pattern\": \"*Screenshot*\", \
            \"destination\": \"{year}/Personal/Screenshots\", \"tags\": [\"screenshot\"]}\n\
         ]\n\
         }\n\n\
         Guidelines:\n\
         - Create a logical hierarchy with at most 3 levels of nesting\n\
         - Use descriptive folder names (no abbreviations)\n\
         - Group related files together\n\
         - Consider both the file types and naming patterns\n\
         - For rules, match_field can be: \"extension\", \"filename\" (glob pattern), or \"regex\" (regex pattern)\n\
         - Use template variables in destinations: {year}, {month}, {date}, {ext}, {source}\n\
         - Prefer broader categories over too-specific folders\n\
         - Only suggest rules for clear, repeated patterns (at least 2-3 files match)\n\
         - Tags should be lowercase, descriptive keywords\n",
    );

    if !existing_folders.is_empty() {
        system.push_str("\nExisting folder structure (preserve and extend, don't restructure):\n");
        for folder in existing_folders {
            system.push_str(&format!("  {folder}/\n"));
        }
        system.push_str(
            "\nIntegrate new suggestions with the existing structure. \
             Prefer placing files in existing folders when appropriate.\n",
        );
    }

    let user_prompt =
        format!("Analyse these files and suggest an organisation structure:\n\n{inventory}");

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system,
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_prompt,
        },
    ];

    let response = provider.chat(messages, 0.3, 2048).await?;
    parse_suggestion(&response.content)
}

fn parse_suggestion(raw: &str) -> anyhow::Result<StructureSuggestion> {
    let json_str = extract_json(raw);
    match serde_json::from_str::<StructureSuggestion>(json_str) {
        Ok(suggestion) => Ok(suggestion),
        Err(e) => {
            tracing::warn!("Failed to parse LLM suggestion: {e}\nRaw: {raw}");
            Err(anyhow::anyhow!(
                "Failed to parse AI structure suggestion: {e}"
            ))
        }
    }
}

/// Extract JSON from a potentially markdown-wrapped response.
fn extract_json(raw: &str) -> &str {
    if let Some(start) = raw.find("```json") {
        let content = &raw[start + 7..];
        if let Some(end) = content.find("```") {
            return content[..end].trim();
        }
    }
    if let Some(start) = raw.find("```") {
        let content = &raw[start + 3..];
        if let Some(end) = content.find("```") {
            return content[..end].trim();
        }
    }
    if let Some(start) = raw.find('{')
        && let Some(end) = raw.rfind('}')
    {
        return &raw[start..=end];
    }
    raw.trim()
}

/// Format rule suggestions as YAML for rules.yaml.
fn format_rules_yaml(rules: &[RuleSuggestion]) -> String {
    let mut yaml = String::from("rules:\n");
    for rule in rules {
        yaml.push_str(&format!("  - name: \"{}\"\n", rule.name));
        yaml.push_str("    match:\n");
        match rule.match_field.as_str() {
            "extension" => {
                yaml.push_str(&format!("      extension: \"{}\"\n", rule.match_pattern));
            }
            "filename" => {
                yaml.push_str(&format!("      filename: \"{}\"\n", rule.match_pattern));
            }
            "regex" => {
                yaml.push_str(&format!(
                    "      filename: \"regex:{}\"\n",
                    rule.match_pattern
                ));
            }
            other => {
                yaml.push_str(&format!("      {other}: \"{}\"\n", rule.match_pattern));
            }
        }
        yaml.push_str(&format!("    destination: \"{}\"\n", rule.destination));
        if !rule.tags.is_empty() {
            let tags: Vec<String> = rule.tags.iter().map(|t| format!("\"{t}\"")).collect();
            yaml.push_str(&format!("    tags: [{}]\n", tags.join(", ")));
        }
        yaml.push('\n');
    }
    yaml
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_entry(name: &str, ext: Option<&str>, inbox: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(format!("/tmp/{name}")),
            name: name.to_string(),
            extension: ext.map(|s| s.to_string()),
            size_bytes: 1024,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            tags: Vec::new(),
            colour: None,
            source_inbox: inbox.to_string(),
            is_dir: false,
        }
    }

    #[test]
    fn build_inventory_groups_by_extension() {
        let entries = vec![
            make_entry("invoice.pdf", Some("pdf"), "Downloads"),
            make_entry("report.pdf", Some("pdf"), "Downloads"),
            make_entry("photo.jpg", Some("jpg"), "Downloads"),
            make_entry("Makefile", None, "Desktop"),
        ];

        let inventory = build_inventory(&entries);
        assert!(inventory.contains("Total files: 4"));
        assert!(inventory.contains(".pdf: 2 file(s)"));
        assert!(inventory.contains(".jpg: 1 file(s)"));
        assert!(inventory.contains("Source inboxes: Desktop, Downloads"));
    }

    #[test]
    fn parse_suggestion_from_clean_json() {
        let json = r#"{
            "folders": [
                {"path": "Work/Documents", "description": "Work docs"},
                {"path": "Personal/Photos", "description": "Photos"}
            ],
            "rules": [
                {"name": "PDFs", "match_field": "extension", "match_pattern": "pdf",
                 "destination": "Work/Documents", "tags": ["document"]}
            ]
        }"#;

        let result = parse_suggestion(json).unwrap();
        assert_eq!(result.folders.len(), 2);
        assert_eq!(result.folders[0].path, "Work/Documents");
        assert_eq!(result.rules.len(), 1);
        assert_eq!(result.rules[0].name, "PDFs");
    }

    #[test]
    fn parse_suggestion_from_markdown() {
        let raw = "Here's my suggestion:\n```json\n{\"folders\": [{\"path\": \"Docs\", \"description\": \"Documents\"}], \"rules\": []}\n```\nHope this helps!";
        let result = parse_suggestion(raw).unwrap();
        assert_eq!(result.folders.len(), 1);
        assert_eq!(result.folders[0].path, "Docs");
    }

    #[test]
    fn format_rules_yaml_output() {
        let rules = vec![
            RuleSuggestion {
                name: "PDF invoices".to_string(),
                match_field: "extension".to_string(),
                match_pattern: "pdf".to_string(),
                destination: "Work/Invoices".to_string(),
                tags: vec!["invoice".to_string(), "work".to_string()],
            },
            RuleSuggestion {
                name: "Screenshots".to_string(),
                match_field: "filename".to_string(),
                match_pattern: "*Screenshot*".to_string(),
                destination: "{year}/Screenshots".to_string(),
                tags: vec!["screenshot".to_string()],
            },
        ];

        let yaml = format_rules_yaml(&rules);
        assert!(yaml.contains("rules:"));
        assert!(yaml.contains("name: \"PDF invoices\""));
        assert!(yaml.contains("extension: \"pdf\""));
        assert!(yaml.contains("tags: [\"invoice\", \"work\"]"));
        assert!(yaml.contains("filename: \"*Screenshot*\""));
    }

    #[test]
    fn format_rules_yaml_with_regex() {
        let rules = vec![RuleSuggestion {
            name: "Dated files".to_string(),
            match_field: "regex".to_string(),
            match_pattern: r"^\d{4}-\d{2}-\d{2}".to_string(),
            destination: "Archive".to_string(),
            tags: vec![],
        }];

        let yaml = format_rules_yaml(&rules);
        assert!(yaml.contains(r#"filename: "regex:^\d{4}-\d{2}-\d{2}""#));
    }

    #[test]
    fn discover_existing_folders_recursive() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("Work/Documents")).unwrap();
        std::fs::create_dir_all(root.join("Work/Projects")).unwrap();
        std::fs::create_dir_all(root.join("Personal/Photos")).unwrap();
        std::fs::create_dir(root.join(".hidden")).unwrap();
        std::fs::create_dir(root.join("_Trash")).unwrap();

        let folders = discover_existing_folders(root);
        assert!(folders.contains(&"Work".to_string()));
        assert!(folders.contains(&"Work/Documents".to_string()));
        assert!(folders.contains(&"Work/Projects".to_string()));
        assert!(folders.contains(&"Personal".to_string()));
        assert!(folders.contains(&"Personal/Photos".to_string()));
        assert!(!folders.contains(&".hidden".to_string()));
        assert!(!folders.contains(&"_Trash".to_string()));
    }

    #[test]
    fn discover_existing_folders_nonexistent() {
        let folders = discover_existing_folders(std::path::Path::new("/nonexistent"));
        assert!(folders.is_empty());
    }

    #[test]
    fn extract_json_from_code_fence() {
        let raw = "```json\n{\"a\": 1}\n```";
        assert_eq!(extract_json(raw), "{\"a\": 1}");
    }

    #[test]
    fn extract_json_from_bare_braces() {
        let raw = "result: {\"a\": 1} done";
        assert_eq!(extract_json(raw), "{\"a\": 1}");
    }

    #[test]
    fn extract_json_from_generic_code_fence() {
        let raw = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json(raw), "{\"key\": \"value\"}");
    }

    #[test]
    fn extract_json_no_braces_returns_trimmed() {
        let raw = "  just plain text  ";
        assert_eq!(extract_json(raw), "just plain text");
    }

    #[test]
    fn parse_suggestion_invalid_json_returns_error() {
        let result = parse_suggestion("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn format_rules_yaml_unknown_match_field() {
        let rules = vec![RuleSuggestion {
            name: "Custom".to_string(),
            match_field: "content".to_string(),
            match_pattern: "TODO".to_string(),
            destination: "Tasks".to_string(),
            tags: vec![],
        }];
        let yaml = format_rules_yaml(&rules);
        assert!(yaml.contains("content: \"TODO\""));
    }

    #[test]
    fn format_rules_yaml_empty_tags_omitted() {
        let rules = vec![RuleSuggestion {
            name: "No tags".to_string(),
            match_field: "extension".to_string(),
            match_pattern: "txt".to_string(),
            destination: "Text".to_string(),
            tags: vec![],
        }];
        let yaml = format_rules_yaml(&rules);
        assert!(!yaml.contains("tags:"));
    }

    #[test]
    fn build_inventory_single_file_no_extension() {
        let entries = vec![make_entry("Makefile", None, "Downloads")];
        let inventory = build_inventory(&entries);
        assert!(inventory.contains("Total files: 1"));
        assert!(inventory.contains("(no extension)"));
    }

    #[test]
    fn discover_existing_folders_max_depth() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create folders 4 levels deep (max depth is 3)
        std::fs::create_dir_all(root.join("A/B/C/D/E")).unwrap();

        let folders = discover_existing_folders(root);
        assert!(folders.contains(&"A".to_string()));
        assert!(folders.contains(&"A/B".to_string()));
        assert!(folders.contains(&"A/B/C".to_string()));
        assert!(folders.contains(&"A/B/C/D".to_string()));
        // D/E would be depth 4 from root, which is beyond max_depth of 3
        assert!(
            !folders.contains(&"A/B/C/D/E".to_string()),
            "depth 4 should be excluded"
        );
    }
}
