//! `librarian review` -- interactive review of needs-review folder.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use chrono::Utc;
use librarian_core::config;
use librarian_learning::corrections::{Correction, CorrectionSource, record_correction};

pub async fn run() -> anyhow::Result<()> {
    let cfg = config::load_default()?;
    let home = config::librarian_home();
    let decisions_path = home.join("history/decisions.jsonl");
    let corrections_path = home.join("history/corrections.jsonl");
    let needs_review = &cfg.needs_review_path;

    if !needs_review.exists() {
        println!(
            "NeedsReview folder does not exist: {}",
            needs_review.display()
        );
        return Ok(());
    }

    let entries: Vec<_> = std::fs::read_dir(needs_review)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            // Skip sidecar files (*.reason.txt)
            !e.path()
                .file_name()
                .map(|f| f.to_string_lossy().ends_with(".reason.txt"))
                .unwrap_or(false)
        })
        .collect();

    if entries.is_empty() {
        println!("No files pending review.");
        return Ok(());
    }

    println!("Found {} file(s) to review.\n", entries.len());

    let stdin = io::stdin();
    let mut reader = stdin.lock();

    for entry in &entries {
        let path = entry.path();
        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        println!("--- File: {} ---", filename);

        // Look for a sidecar reason file
        let reason_path = path.with_extension(format!(
            "{}.reason.txt",
            path.extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default()
        ));
        if reason_path.exists()
            && let Ok(reason) = std::fs::read_to_string(&reason_path)
        {
            println!("Reason: {}", reason.trim());
        }

        // Suggest the destination root as a starting point
        let suggested = cfg.destination_root.join(&filename);
        println!("Suggested destination: {}", suggested.display());

        print!("[a]ccept, [s]kip, [m]ove to <path>, [q]uit: ");
        io::stdout().flush()?;

        let mut input = String::new();
        reader.read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        match input.as_str() {
            "a" | "accept" => {
                // Accept: move to suggested destination
                if let Some(parent) = suggested.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let file_hash = librarian_core::hasher::hash_file_sync(&path)?;
                let filetype = path.extension().map(|e| e.to_string_lossy().to_lowercase());

                let correction = Correction {
                    original_path: path.clone(),
                    corrected_path: suggested.clone(),
                    file_hash,
                    source: CorrectionSource::Review,
                    corrected_tags: None,
                    timestamp: Utc::now(),
                    source_inbox: "NeedsReview".to_string(),
                    filetype,
                };

                record_correction(&corrections_path, &decisions_path, &correction)?;
                std::fs::rename(&path, &suggested)?;
                println!("Moved to {}\n", suggested.display());

                // Clean up sidecar if it exists
                if reason_path.exists() {
                    let _ = std::fs::remove_file(&reason_path);
                }
            }
            "s" | "skip" => {
                println!("Skipped.\n");
                continue;
            }
            "q" | "quit" => {
                println!("Review session ended.");
                return Ok(());
            }
            other if other.starts_with("m ") || other.starts_with("move ") => {
                let dest_str = other
                    .strip_prefix("m ")
                    .or_else(|| other.strip_prefix("move "))
                    .unwrap_or("");
                let dest = PathBuf::from(dest_str.trim());

                if dest.as_os_str().is_empty() {
                    println!("No destination provided. Skipping.\n");
                    continue;
                }

                let dest = if dest.is_dir() {
                    dest.join(&filename)
                } else {
                    dest
                };

                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let file_hash = librarian_core::hasher::hash_file_sync(&path)?;
                let filetype = path.extension().map(|e| e.to_string_lossy().to_lowercase());

                let correction = Correction {
                    original_path: path.clone(),
                    corrected_path: dest.clone(),
                    file_hash,
                    source: CorrectionSource::Review,
                    corrected_tags: None,
                    timestamp: Utc::now(),
                    source_inbox: "NeedsReview".to_string(),
                    filetype,
                };

                record_correction(&corrections_path, &decisions_path, &correction)?;
                std::fs::rename(&path, &dest)?;
                println!("Moved to {}\n", dest.display());

                // Clean up sidecar if it exists
                if reason_path.exists() {
                    let _ = std::fs::remove_file(&reason_path);
                }
            }
            _ => {
                println!("Unknown command. Skipping.\n");
            }
        }
    }

    println!("Review complete.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn sidecar_files_are_filtered() {
        let dir = tempfile::tempdir().unwrap();
        let nr = dir.path().join("NeedsReview");
        std::fs::create_dir_all(&nr).unwrap();

        // Create a file and its sidecar
        std::fs::write(nr.join("report.pdf"), "pdf").unwrap();
        std::fs::write(nr.join("report.pdf.reason.txt"), "low confidence").unwrap();
        std::fs::write(nr.join("photo.jpg"), "jpg").unwrap();

        let entries: Vec<_> = std::fs::read_dir(&nr)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter(|e| {
                !e.path()
                    .file_name()
                    .map(|f| f.to_string_lossy().ends_with(".reason.txt"))
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(entries.len(), 2, "sidecar .reason.txt should be filtered");
        let names: Vec<String> = entries
            .iter()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"report.pdf".to_string()));
        assert!(names.contains(&"photo.jpg".to_string()));
    }

    #[test]
    fn empty_review_folder_returns_no_entries() {
        let dir = tempfile::tempdir().unwrap();
        let nr = dir.path().join("NeedsReview");
        std::fs::create_dir_all(&nr).unwrap();

        let entries: Vec<_> = std::fs::read_dir(&nr)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .collect();

        assert!(entries.is_empty());
    }

    #[test]
    fn move_to_directory_appends_filename() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("Documents");
        std::fs::create_dir_all(&dest).unwrap();
        let filename = "report.pdf";

        let final_dest = if dest.is_dir() {
            dest.join(filename)
        } else {
            dest.clone()
        };

        assert_eq!(final_dest, dest.join("report.pdf"));
    }

    #[test]
    fn reason_path_construction() {
        let path = PathBuf::from("/tmp/NeedsReview/report.pdf");
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let reason_path = path.with_extension(format!("{ext}.reason.txt"));
        assert_eq!(
            reason_path,
            PathBuf::from("/tmp/NeedsReview/report.pdf.reason.txt")
        );
    }
}
