//! FileEntry type for scanned files.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Finder colour label indices matching macOS FinderInfo byte 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FinderColour {
    None = 0,
    Grey = 1,
    Green = 2,
    Purple = 3,
    Blue = 4,
    Yellow = 5,
    Red = 6,
    Orange = 7,
}

impl FinderColour {
    pub fn from_index(i: u8) -> Self {
        match i {
            1 => Self::Grey,
            2 => Self::Green,
            3 => Self::Purple,
            4 => Self::Blue,
            5 => Self::Yellow,
            6 => Self::Red,
            7 => Self::Orange,
            _ => Self::None,
        }
    }

    pub fn index(self) -> u8 {
        self as u8
    }
}

/// A filesystem object discovered during scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Filename with extension.
    pub name: String,
    /// File extension (lowercase, no dot). None for extensionless files.
    pub extension: Option<String>,
    /// File size in bytes.
    pub size_bytes: u64,
    /// blake3 hex digest. Empty until hashing is performed.
    pub hash: String,
    /// File creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp.
    pub modified_at: DateTime<Utc>,
    /// Current Finder tags (read from xattr or sidecar).
    pub tags: Vec<String>,
    /// Current Finder colour label.
    pub colour: Option<FinderColour>,
    /// Which inbox folder this file was found in.
    pub source_inbox: String,
}

impl FileEntry {
    /// Create a new FileEntry from a path and source inbox name.
    /// Populates metadata from the filesystem. Hash is left empty.
    pub fn from_path(path: PathBuf, source_inbox: &str) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(&path)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase());

        let created_at = metadata
            .created()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .into();
        let modified_at: DateTime<Utc> = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .into();

        Ok(Self {
            path,
            name,
            extension,
            size_bytes: metadata.len(),
            hash: String::new(),
            created_at,
            modified_at,
            tags: Vec::new(),
            colour: None,
            source_inbox: source_inbox.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finder_colour_round_trip() {
        for i in 0..=7u8 {
            let colour = FinderColour::from_index(i);
            assert_eq!(colour.index(), i);
        }
    }

    #[test]
    fn finder_colour_out_of_range_defaults_to_none() {
        assert_eq!(FinderColour::from_index(99), FinderColour::None);
    }

    #[test]
    fn file_entry_from_path() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_document.pdf");
        std::fs::write(&file_path, b"hello world").unwrap();

        let entry = FileEntry::from_path(file_path.clone(), "Downloads").unwrap();

        assert_eq!(entry.name, "test_document.pdf");
        assert_eq!(entry.extension.as_deref(), Some("pdf"));
        assert_eq!(entry.size_bytes, 11);
        assert!(entry.hash.is_empty());
        assert_eq!(entry.source_inbox, "Downloads");
        assert!(entry.tags.is_empty());
        assert!(entry.colour.is_none());
    }

    #[test]
    fn file_entry_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("Makefile");
        std::fs::write(&file_path, b"all:").unwrap();

        let entry = FileEntry::from_path(file_path, "Desktop").unwrap();
        assert_eq!(entry.name, "Makefile");
        assert!(entry.extension.is_none());
    }

    #[test]
    fn file_entry_serialisation_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("report.csv");
        std::fs::write(&file_path, b"a,b\n1,2").unwrap();

        let entry = FileEntry::from_path(file_path, "Documents").unwrap();
        let json = serde_json::to_string(&entry).unwrap();
        let restored: FileEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.name, restored.name);
        assert_eq!(entry.size_bytes, restored.size_bytes);
        assert_eq!(entry.extension, restored.extension);
    }
}
