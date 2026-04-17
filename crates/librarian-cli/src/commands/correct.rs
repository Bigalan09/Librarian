//! `librarian correct` -- record an explicit correction.

use std::path::PathBuf;

use chrono::Utc;
use librarian_core::config;
use librarian_core::decision::read_decisions;
use librarian_learning::corrections::{record_correction, Correction, CorrectionSource};

pub async fn run(
    file: PathBuf,
    to: Option<PathBuf>,
    retag: Option<String>,
) -> anyhow::Result<()> {
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
        anyhow::bail!("File not found: {}", file.display());
    }

    // Find the file in the decision log to get the original placement
    let file_hash = hash_file(&file)?;
    let decisions = read_decisions(&decisions_path)?;

    let original_decision = decisions
        .iter()
        .rev()
        .find(|d| d.file_hash == file_hash);

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
                anyhow::bail!("Must specify --to or --retag (or both)");
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

    let filetype = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase());

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
    if let Some(_) = &to {
        if file != corrected_path {
            if let Some(parent) = corrected_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&file, &corrected_path)?;
            println!(
                "Moved {} -> {}",
                file.display(),
                corrected_path.display()
            );
        }
    }

    println!("Correction recorded for {}", file_hash);

    // Update centroids if we have an embedding store
    let centroid_path = home.join("history/centroids.msgpack");
    if centroid_path.exists() {
        tracing::info!("Centroid update would happen here with embedding support");
    }

    Ok(())
}

/// Hash a file using blake3.
fn hash_file(path: &PathBuf) -> anyhow::Result<String> {
    let data = std::fs::read(path)?;
    let hash = blake3::hash(&data);
    Ok(hash.to_hex().to_string())
}

/// Detect the source inbox name from a file path by comparing against
/// configured inbox folders.
fn detect_source_inbox(path: &PathBuf, cfg: &config::AppConfig) -> String {
    for inbox in &cfg.inbox_folders {
        if let Some(name) = inbox.file_name() {
            if path.starts_with(inbox)
                || path
                    .to_string_lossy()
                    .contains(&name.to_string_lossy().as_ref())
            {
                return name.to_string_lossy().to_string();
            }
        }
    }
    // Fallback
    path.parent()
        .and_then(|p| p.file_name())
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
