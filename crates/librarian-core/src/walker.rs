//! Directory scanner with ignore integration.

use std::path::Path;

use crate::file_entry::FileEntry;
use crate::hasher;
use crate::ignore::IgnoreEngine;

/// Scan a directory recursively, producing FileEntry items.
/// Respects the ignore engine and max_files limit.
pub async fn scan_directory(
    source_dir: &Path,
    source_inbox_name: &str,
    ignore_engine: &IgnoreEngine,
    max_files: usize,
) -> anyhow::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();
    let mut stack = vec![source_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut read_dir = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(e) => {
                tracing::warn!("cannot read directory {}: {}", dir.display(), e);
                continue;
            }
        };

        while let Some(entry) = read_dir.next_entry().await? {
            if entries.len() >= max_files {
                tracing::info!(
                    "reached max_files limit ({}) during scan of {}",
                    max_files,
                    source_dir.display()
                );
                return Ok(entries);
            }

            let path = entry.path();

            if ignore_engine.is_ignored(&path) {
                tracing::debug!("ignored: {}", path.display());
                continue;
            }

            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => {
                    tracing::warn!("cannot stat {}: {}", path.display(), e);
                    continue;
                }
            };

            if file_type.is_symlink() {
                if IgnoreEngine::is_external_symlink(&path, source_dir) {
                    tracing::debug!("ignored external symlink: {}", path.display());
                    continue;
                }
            }

            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            if file_type.is_file() {
                match FileEntry::from_path(path.clone(), source_inbox_name) {
                    Ok(entry) => entries.push(entry),
                    Err(e) => {
                        tracing::warn!("cannot read metadata for {}: {}", path.display(), e);
                    }
                }
            }
        }
    }

    Ok(entries)
}

/// Hash all entries in parallel using tokio tasks.
pub async fn hash_entries(entries: &mut [FileEntry]) -> anyhow::Result<()> {
    // Process in batches to avoid spawning thousands of tasks
    let batch_size = 64;
    for chunk in entries.chunks_mut(batch_size) {
        let mut handles = Vec::new();
        for entry in chunk.iter() {
            let path = entry.path.clone();
            handles.push(tokio::spawn(async move { hasher::hash_file(&path).await }));
        }

        for (entry, handle) in chunk.iter_mut().zip(handles) {
            match handle.await? {
                Ok(hash) => entry.hash = hash,
                Err(e) => {
                    tracing::warn!("failed to hash {}: {}", entry.path.display(), e);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file1.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("file2.pdf"), b"world").unwrap();
        std::fs::write(dir.path().join(".hidden"), b"secret").unwrap();
        std::fs::write(dir.path().join(".DS_Store"), b"junk").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/file3.csv"), b"a,b").unwrap();
        dir
    }

    #[tokio::test]
    async fn scan_finds_visible_files() {
        let dir = create_test_dir();
        let engine = IgnoreEngine::new(dir.path(), None).unwrap();
        let entries = scan_directory(dir.path(), "test", &engine, 1000)
            .await
            .unwrap();

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"file1.txt"));
        assert!(names.contains(&"file2.pdf"));
        assert!(names.contains(&"file3.csv"));
    }

    #[tokio::test]
    async fn scan_ignores_hidden_and_ds_store() {
        let dir = create_test_dir();
        let engine = IgnoreEngine::new(dir.path(), None).unwrap();
        let entries = scan_directory(dir.path(), "test", &engine, 1000)
            .await
            .unwrap();

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.contains(&".hidden"));
        assert!(!names.contains(&".DS_Store"));
    }

    #[tokio::test]
    async fn scan_respects_max_files() {
        let dir = create_test_dir();
        let engine = IgnoreEngine::new(dir.path(), None).unwrap();
        let entries = scan_directory(dir.path(), "test", &engine, 2)
            .await
            .unwrap();

        assert!(entries.len() <= 2);
    }

    #[tokio::test]
    async fn scan_populates_source_inbox() {
        let dir = create_test_dir();
        let engine = IgnoreEngine::new(dir.path(), None).unwrap();
        let entries = scan_directory(dir.path(), "Downloads", &engine, 1000)
            .await
            .unwrap();

        for entry in &entries {
            assert_eq!(entry.source_inbox, "Downloads");
        }
    }

    #[tokio::test]
    async fn hash_entries_populates_hashes() {
        let dir = create_test_dir();
        let engine = IgnoreEngine::new(dir.path(), None).unwrap();
        let mut entries = scan_directory(dir.path(), "test", &engine, 1000)
            .await
            .unwrap();

        assert!(entries.iter().all(|e| e.hash.is_empty()));

        hash_entries(&mut entries).await.unwrap();

        assert!(entries.iter().all(|e| !e.hash.is_empty()));
        // Verify hash is valid hex
        assert!(entries
            .iter()
            .all(|e| e.hash.chars().all(|c| c.is_ascii_hexdigit())));
    }

    #[tokio::test]
    async fn scan_handles_librarianignore() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"yes").unwrap();
        std::fs::write(dir.path().join("skip.log"), b"no").unwrap();
        std::fs::write(dir.path().join(".librarianignore"), "*.log\n").unwrap();

        let engine = IgnoreEngine::new(dir.path(), None).unwrap();
        let entries = scan_directory(dir.path(), "test", &engine, 1000)
            .await
            .unwrap();

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"keep.txt"));
        assert!(!names.contains(&"skip.log"));
    }
}
