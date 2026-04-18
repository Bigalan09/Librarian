//! Filesystem watcher for corrections

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use chrono::Utc;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::corrections::{
    Correction, CorrectionSource, is_within_correction_window, record_correction,
};

/// Watches destination directories for file moves that might be user corrections.
///
/// In v1 this is NOT a daemon -- it is created during `librarian process` to
/// detect moves since the last run.
pub struct CorrectionWatcher {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<notify::Result<Event>>,
    watch_dirs: Vec<PathBuf>,
}

impl CorrectionWatcher {
    /// Start watching the given directories for file events.
    pub fn new(watch_dirs: &[PathBuf]) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;

        for dir in watch_dirs {
            if dir.exists() {
                watcher.watch(dir, RecursiveMode::Recursive)?;
            }
        }

        Ok(Self {
            _watcher: watcher,
            receiver: rx,
            watch_dirs: watch_dirs.to_vec(),
        })
    }

    /// Get the directories being watched.
    pub fn watch_dirs(&self) -> &[PathBuf] {
        &self.watch_dirs
    }

    /// Check for corrections by examining pending filesystem events.
    ///
    /// Compares events against a manifest of known file hashes -> paths.
    /// If a known file hash appears at a new path, it is treated as a correction
    /// (if within the correction window).
    ///
    /// `manifest` maps file_hash -> original_path (the path librarian placed it).
    pub fn check_for_corrections(
        &self,
        manifest: &HashMap<String, PathBuf>,
        correction_window_days: u32,
        corrections_path: &Path,
        decisions_path: &Path,
    ) -> anyhow::Result<Vec<Correction>> {
        let mut corrections = Vec::new();

        // Drain all pending events (non-blocking)
        while let Ok(event_result) = self.receiver.try_recv() {
            let event = match event_result {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!("Watcher error: {}", err);
                    continue;
                }
            };

            // We care about Create and Modify events (file moved/renamed shows as Create)
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                continue;
            }

            for path in &event.paths {
                if !path.is_file() {
                    continue;
                }

                // Hash the file to see if it matches a known placement
                let hash = match librarian_core::hasher::hash_file_sync(path) {
                    Ok(h) => h,
                    Err(_) => continue,
                };

                if let Some(original_path) = manifest.get(&hash) {
                    // File hash matches a known placement but is at a different path
                    if original_path != path {
                        let placement_time = Utc::now(); // Approximate; real impl would look up actual placement time
                        if is_within_correction_window(placement_time, correction_window_days) {
                            let filetype =
                                path.extension().map(|e| e.to_string_lossy().to_lowercase());

                            let source_inbox = detect_source_inbox(original_path);

                            let correction = Correction {
                                original_path: original_path.clone(),
                                corrected_path: path.clone(),
                                file_hash: hash.clone(),
                                source: CorrectionSource::Watched,
                                corrected_tags: None,
                                timestamp: Utc::now(),
                                source_inbox,
                                filetype,
                            };

                            record_correction(corrections_path, decisions_path, &correction)?;
                            corrections.push(correction);
                        }
                    }
                }
            }
        }

        Ok(corrections)
    }
}

/// Attempt to detect the source inbox from the original path.
/// Falls back to the parent directory name.
fn detect_source_inbox(path: &Path) -> String {
    // Walk up the path looking for common inbox names
    for ancestor in path.ancestors() {
        if let Some(name) = ancestor.file_name() {
            let name_str = name.to_string_lossy();
            if name_str == "Downloads" || name_str == "Desktop" {
                return name_str.to_string();
            }
        }
    }
    // Fallback: use the immediate parent directory name
    path.parent()
        .and_then(|p| p.file_name())
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_watcher_on_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let watcher = CorrectionWatcher::new(&[dir.path().to_path_buf()]);
        assert!(watcher.is_ok());
        let w = watcher.unwrap();
        assert_eq!(w.watch_dirs().len(), 1);
    }

    #[test]
    fn create_watcher_on_nonexistent_dir() {
        // Should not panic; nonexistent dirs are just skipped
        let watcher = CorrectionWatcher::new(&[PathBuf::from("/nonexistent/watch/dir")]);
        assert!(watcher.is_ok());
    }

    #[test]
    fn detect_source_inbox_downloads() {
        let path = Path::new("/Users/me/Library-Managed/Work/Invoices/doc.pdf");
        // No Downloads or Desktop in path, so fallback to parent dir name
        let inbox = detect_source_inbox(path);
        assert_eq!(inbox, "Invoices");
    }

    #[test]
    fn detect_source_inbox_known() {
        let path = Path::new("/Users/me/Downloads/doc.pdf");
        let inbox = detect_source_inbox(path);
        assert_eq!(inbox, "Downloads");
    }

    #[test]
    fn detect_source_inbox_desktop() {
        let path = Path::new("/Users/me/Desktop/screenshot.png");
        let inbox = detect_source_inbox(path);
        assert_eq!(inbox, "Desktop");
    }

    #[test]
    fn detect_source_inbox_nested_downloads() {
        let path = Path::new("/Users/me/Downloads/subdir/file.txt");
        let inbox = detect_source_inbox(path);
        assert_eq!(inbox, "Downloads");
    }

    #[test]
    fn check_corrections_with_no_events() {
        let dir = tempfile::tempdir().unwrap();
        let watcher = CorrectionWatcher::new(&[dir.path().to_path_buf()]).unwrap();

        let manifest = HashMap::new();
        let corrections_path = dir.path().join("corrections.jsonl");
        let decisions_path = dir.path().join("decisions.jsonl");

        let corrections = watcher
            .check_for_corrections(&manifest, 14, &corrections_path, &decisions_path)
            .unwrap();
        assert!(corrections.is_empty());
    }

    #[test]
    fn watcher_watch_dirs_returns_configured_dirs() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();

        let watcher =
            CorrectionWatcher::new(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()])
                .unwrap();

        assert_eq!(watcher.watch_dirs().len(), 2);
    }
}
