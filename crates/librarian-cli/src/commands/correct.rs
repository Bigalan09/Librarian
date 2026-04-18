//! `librarian correct` -- record an explicit correction.

use std::path::PathBuf;

use chrono::Utc;
use librarian_core::config;
use librarian_core::decision::read_decisions;
use librarian_learning::corrections::{Correction, CorrectionSource, record_correction};

pub async fn run(file: PathBuf, to: Option<PathBuf>, retag: Option<String>) -> anyhow::Result<()> {
    let cfg = config::load_default()?;
    let home = config::librarian_home();
    let decisions_path = home.join("history/decisions.jsonl");
    let corrections_path = home.join("history/corrections.jsonl");

    // Resolve the file path
    let file = if file.is_relative() {
        std::env::current_dir()?.join(&file)
    } else {
        file
    };

    if !file.exists() {
        anyhow::bail!(
            "File not found at {}. Check the path and try again.",
            file.display()
        );
    }

    // Find the file in the decision log to get the original placement
    let file_hash = librarian_core::hasher::hash_file_sync(&file)?;
    let decisions = read_decisions(&decisions_path)?;

    let original_decision = decisions.iter().rev().find(|d| d.file_hash == file_hash);

    let original_path = match original_decision {
        Some(d) => d.file_path.clone(),
        None => {
            tracing::warn!(
                "File hash {} not found in decision log; recording correction anyway",
                file_hash
            );
            file.clone()
        }
    };

    // Determine the corrected path
    let corrected_path = match &to {
        Some(dest) => {
            let dest = if dest.is_relative() {
                std::env::current_dir()?.join(dest)
            } else {
                dest.clone()
            };
            // If dest is a directory, move the file into it
            if dest.is_dir() {
                dest.join(file.file_name().unwrap_or_default())
            } else {
                dest
            }
        }
        None => {
            if retag.is_none() {
                anyhow::bail!(
                    "Nothing to do: specify --to <destination> to move the file, \
                     --retag <tags> to update its tags, or both."
                );
            }
            // Retag only, no path change
            file.clone()
        }
    };

    let corrected_tags = retag.map(|tags| {
        tags.split(',')
            .map(|t| t.trim().to_string())
            .collect::<Vec<_>>()
    });

    let filetype = file.extension().map(|e| e.to_string_lossy().to_lowercase());

    // Detect source inbox from original path
    let source_inbox = detect_source_inbox(&original_path, &cfg);

    let correction = Correction {
        original_path,
        corrected_path: corrected_path.clone(),
        file_hash: file_hash.clone(),
        source: CorrectionSource::Explicit,
        corrected_tags,
        timestamp: Utc::now(),
        source_inbox,
        filetype,
    };

    record_correction(&corrections_path, &decisions_path, &correction)?;

    // If --to was specified, actually move the file
    if to.is_some() && file != corrected_path {
        if let Some(parent) = corrected_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&file, &corrected_path)?;
        println!("Moved {} -> {}", file.display(), corrected_path.display());
    }

    println!("Correction recorded for {}", file_hash);

    // Centroid drift happens automatically during the next `process` run:
    // the correction is recorded in corrections.jsonl, and few-shot examples
    // + embedding updates are computed when files are re-classified.

    Ok(())
}

/// Detect the source inbox name from a file path by comparing against
/// configured inbox folders.
fn detect_source_inbox(path: &std::path::Path, cfg: &config::AppConfig) -> String {
    for inbox in &cfg.inbox_folders {
        if let Some(name) = inbox.file_name()
            && (path.starts_with(inbox)
                || path
                    .to_string_lossy()
                    .contains(name.to_string_lossy().as_ref()))
        {
            return name.to_string_lossy().to_string();
        }
    }
    // Fallback
    path.parent()
        .and_then(|p| p.file_name())
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detect_inbox_from_matching_path() {
        let cfg = config::AppConfig {
            inbox_folders: vec![PathBuf::from("/home/user/Downloads")],
            ..Default::default()
        };
        let result = detect_source_inbox(
            std::path::Path::new("/home/user/Downloads/invoice.pdf"),
            &cfg,
        );
        assert_eq!(result, "Downloads");
    }

    #[test]
    fn detect_inbox_falls_back_to_parent() {
        let cfg = config::AppConfig {
            inbox_folders: vec![PathBuf::from("/home/user/Downloads")],
            ..Default::default()
        };
        let result =
            detect_source_inbox(std::path::Path::new("/other/path/Uploads/file.txt"), &cfg);
        assert_eq!(result, "Uploads");
    }

    #[test]
    fn detect_inbox_no_inboxes_configured() {
        let cfg = config::AppConfig {
            inbox_folders: vec![],
            ..Default::default()
        };
        let result = detect_source_inbox(std::path::Path::new("/some/dir/file.txt"), &cfg);
        assert_eq!(result, "dir");
    }

    #[test]
    fn detect_inbox_multiple_inboxes() {
        let cfg = config::AppConfig {
            inbox_folders: vec![
                PathBuf::from("/home/user/Downloads"),
                PathBuf::from("/home/user/Desktop"),
            ],
            ..Default::default()
        };
        let result =
            detect_source_inbox(std::path::Path::new("/home/user/Desktop/photo.jpg"), &cfg);
        assert_eq!(result, "Desktop");
    }
}
