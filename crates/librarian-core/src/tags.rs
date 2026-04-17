//! Finder tags and colour labels.
//!
//! On macOS, tags and colour labels are stored in extended attributes on the
//! file itself. On other platforms (and as a fallback) a `.librarian-meta.json`
//! sidecar file placed alongside the target file is used instead.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::file_entry::FinderColour;

// ---------------------------------------------------------------------------
// Sidecar helpers (shared across platforms for the non-macOS path)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn sidecar_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    parent.join(".librarian-meta.json")
}

// ---------------------------------------------------------------------------
// macOS implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use anyhow::Context;

    const XATTR_TAGS: &str = "com.apple.metadata:_kMDItemUserTags";
    const XATTR_FINDER_INFO: &str = "com.apple.FinderInfo";
    const XATTR_ORIGINAL_NAME: &str = "com.apple.metadata:LibrarianOriginalName";

    /// Decode a binary-plist xattr value into a list of strings.
    fn decode_plist_tags(data: &[u8]) -> Result<Vec<String>> {
        let value: plist::Value = plist::from_bytes(data)
            .context("failed to decode binary plist for tags")?;

        let array = value
            .as_array()
            .context("tags plist is not an array")?;

        let tags = array
            .iter()
            .filter_map(|v| v.as_string())
            // Strip the optional "\n<colour_index>" suffix that Finder appends.
            .map(|s| {
                if let Some(pos) = s.find('\n') {
                    s[..pos].to_owned()
                } else {
                    s.to_owned()
                }
            })
            .filter(|s| !s.is_empty())
            .collect();

        Ok(tags)
    }

    /// Encode a list of tag strings into a binary plist byte vector.
    fn encode_plist_tags(tags: &[String]) -> Result<Vec<u8>> {
        // Finder stores tags as "<name>\n0" — we use colour index 0 (no colour)
        // for all tags written by Librarian.
        let array: Vec<plist::Value> = tags
            .iter()
            .map(|t| plist::Value::String(format!("{}\n0", t)))
            .collect();

        let plist_value = plist::Value::Array(array);
        let mut buf = Vec::new();
        plist_value
            .to_writer_binary(&mut buf)
            .context("failed to encode tags as binary plist")?;
        Ok(buf)
    }

    pub fn read_tags(path: &Path) -> Result<Vec<String>> {
        match xattr::get(path, XATTR_TAGS)
            .with_context(|| format!("xattr get tags on {}", path.display()))?
        {
            Some(data) => decode_plist_tags(&data),
            None => Ok(Vec::new()),
        }
    }

    pub fn write_tags(path: &Path, tags: &[String]) -> Result<()> {
        if tags.is_empty() {
            // Remove the xattr entirely when there are no tags.
            match xattr::remove(path, XATTR_TAGS) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(e)
                        .with_context(|| format!("xattr remove tags on {}", path.display()))
                }
            }
        } else {
            let data = encode_plist_tags(tags)?;
            xattr::set(path, XATTR_TAGS, &data)
                .with_context(|| format!("xattr set tags on {}", path.display()))?;
        }
        Ok(())
    }

    pub fn read_colour(path: &Path) -> Result<Option<FinderColour>> {
        match xattr::get(path, XATTR_FINDER_INFO)
            .with_context(|| format!("xattr get FinderInfo on {}", path.display()))?
        {
            Some(data) if data.len() > 9 => {
                // Byte 9 (0-indexed) of FinderInfo holds the colour in the
                // lower 3 bits of the flags byte.
                let colour_index = (data[9] >> 1) & 0x07;
                if colour_index == 0 {
                    Ok(None)
                } else {
                    Ok(Some(FinderColour::from_index(colour_index)))
                }
            }
            _ => Ok(None),
        }
    }

    pub fn write_colour(path: &Path, colour: FinderColour) -> Result<()> {
        // Read existing FinderInfo (32 bytes) or create a zeroed buffer.
        let mut data = match xattr::get(path, XATTR_FINDER_INFO)
            .with_context(|| format!("xattr get FinderInfo on {}", path.display()))?
        {
            Some(d) if d.len() >= 32 => d,
            Some(d) => {
                let mut buf = vec![0u8; 32];
                buf[..d.len()].copy_from_slice(&d);
                buf
            }
            None => vec![0u8; 32],
        };

        // Colour is stored in byte 9, bits 1-3 (the label flags).
        // Mask out existing colour bits and set new ones.
        data[9] = (data[9] & !0x0E) | ((colour.index() & 0x07) << 1);

        xattr::set(path, XATTR_FINDER_INFO, &data)
            .with_context(|| format!("xattr set FinderInfo on {}", path.display()))?;
        Ok(())
    }

    pub fn save_original_name(path: &Path, original_name: &str) -> Result<()> {
        xattr::set(path, XATTR_ORIGINAL_NAME, original_name.as_bytes())
            .with_context(|| format!("xattr set original name on {}", path.display()))?;
        Ok(())
    }

    pub fn read_original_name(path: &Path) -> Result<Option<String>> {
        match xattr::get(path, XATTR_ORIGINAL_NAME)
            .with_context(|| format!("xattr get original name on {}", path.display()))?
        {
            Some(data) => {
                let name = String::from_utf8(data)
                    .context("original name xattr is not valid UTF-8")?;
                Ok(Some(name))
            }
            None => Ok(None),
        }
    }

    pub fn remove_tags(path: &Path) -> Result<()> {
        match xattr::remove(path, XATTR_TAGS) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("xattr remove tags on {}", path.display()))
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Non-macOS implementation (sidecar JSON)
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Serialize, Deserialize)]
    struct SidecarMeta {
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        colour: Option<FinderColour>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_name: Option<String>,
    }

    fn read_sidecar(path: &Path) -> Result<SidecarMeta> {
        let sidecar = sidecar_path(path);
        if !sidecar.exists() {
            return Ok(SidecarMeta::default());
        }
        let content = std::fs::read_to_string(&sidecar)
            .with_context(|| format!("reading sidecar {}", sidecar.display()))?;
        let meta: SidecarMeta = serde_json::from_str(&content)
            .with_context(|| format!("parsing sidecar {}", sidecar.display()))?;
        Ok(meta)
    }

    fn write_sidecar(path: &Path, meta: &SidecarMeta) -> Result<()> {
        let sidecar = sidecar_path(path);
        let content = serde_json::to_string_pretty(meta)
            .context("serialising sidecar metadata")?;
        std::fs::write(&sidecar, content)
            .with_context(|| format!("writing sidecar {}", sidecar.display()))?;
        Ok(())
    }

    pub fn read_tags(path: &Path) -> Result<Vec<String>> {
        Ok(read_sidecar(path)?.tags)
    }

    pub fn write_tags(path: &Path, tags: &[String]) -> Result<()> {
        let mut meta = read_sidecar(path)?;
        meta.tags = tags.to_vec();
        write_sidecar(path, &meta)
    }

    pub fn read_colour(path: &Path) -> Result<Option<FinderColour>> {
        Ok(read_sidecar(path)?.colour)
    }

    pub fn write_colour(path: &Path, colour: FinderColour) -> Result<()> {
        let mut meta = read_sidecar(path)?;
        meta.colour = Some(colour);
        write_sidecar(path, &meta)
    }

    pub fn save_original_name(path: &Path, original_name: &str) -> Result<()> {
        let mut meta = read_sidecar(path)?;
        meta.original_name = Some(original_name.to_owned());
        write_sidecar(path, &meta)
    }

    pub fn read_original_name(path: &Path) -> Result<Option<String>> {
        Ok(read_sidecar(path)?.original_name)
    }

    pub fn remove_tags(path: &Path) -> Result<()> {
        let mut meta = read_sidecar(path)?;
        meta.tags.clear();
        write_sidecar(path, &meta)
    }

    // -----------------------------------------------------------------------
    // Re-export the SidecarMeta type so tests can inspect it.
    // -----------------------------------------------------------------------
    #[cfg(test)]
    pub(super) use SidecarMeta as _SidecarMeta;
}

// ---------------------------------------------------------------------------
// Public API — thin wrappers that delegate to the platform module.
// ---------------------------------------------------------------------------

/// Read the Finder tags attached to `path`.
pub fn read_tags(path: &Path) -> Result<Vec<String>> {
    platform::read_tags(path)
}

/// Write Finder tags to `path`, replacing any existing tags.
pub fn write_tags(path: &Path, tags: &[String]) -> Result<()> {
    platform::write_tags(path, tags)
}

/// Read the Finder colour label on `path`. Returns `None` if no label is set.
pub fn read_colour(path: &Path) -> Result<Option<FinderColour>> {
    platform::read_colour(path)
}

/// Set the Finder colour label on `path`.
pub fn write_colour(path: &Path, colour: FinderColour) -> Result<()> {
    platform::write_colour(path, colour)
}

/// Persist the original filename for `path` so it can be recovered after a
/// rename.
pub fn save_original_name(path: &Path, original_name: &str) -> Result<()> {
    platform::save_original_name(path, original_name)
}

/// Retrieve the previously saved original filename for `path`.
pub fn read_original_name(path: &Path) -> Result<Option<String>> {
    platform::read_original_name(path)
}

/// Remove all tags from `path` (used during rollback).
pub fn remove_tags(path: &Path) -> Result<()> {
    platform::remove_tags(path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ------------------------------------------------------------------
    // Sidecar round-trip tests (compile on all platforms)
    // ------------------------------------------------------------------

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn sidecar_tags_round_trip() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("document.pdf");
        std::fs::write(&file, b"dummy").unwrap();

        let tags = vec!["work".to_owned(), "urgent".to_owned()];
        write_tags(&file, &tags).unwrap();

        let read_back = read_tags(&file).unwrap();
        assert_eq!(read_back, tags);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn sidecar_colour_round_trip() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("report.pdf");
        std::fs::write(&file, b"dummy").unwrap();

        write_colour(&file, FinderColour::Yellow).unwrap();

        let colour = read_colour(&file).unwrap();
        assert_eq!(colour, Some(FinderColour::Yellow));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn sidecar_original_name_round_trip() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("renamed.pdf");
        std::fs::write(&file, b"dummy").unwrap();

        save_original_name(&file, "original.pdf").unwrap();

        let name = read_original_name(&file).unwrap();
        assert_eq!(name.as_deref(), Some("original.pdf"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn sidecar_remove_tags_clears_tags() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("archive.zip");
        std::fs::write(&file, b"dummy").unwrap();

        let tags = vec!["inbox".to_owned(), "review".to_owned()];
        write_tags(&file, &tags).unwrap();
        remove_tags(&file).unwrap();

        let remaining = read_tags(&file).unwrap();
        assert!(remaining.is_empty(), "expected no tags after remove_tags");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn sidecar_remove_tags_preserves_other_fields() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, b"dummy").unwrap();

        write_colour(&file, FinderColour::Green).unwrap();
        save_original_name(&file, "old-notes.txt").unwrap();
        write_tags(&file, &["temp".to_owned()]).unwrap();
        remove_tags(&file).unwrap();

        // Colour and original_name must survive the tag removal.
        assert_eq!(read_colour(&file).unwrap(), Some(FinderColour::Green));
        assert_eq!(
            read_original_name(&file).unwrap().as_deref(),
            Some("old-notes.txt")
        );
        assert!(read_tags(&file).unwrap().is_empty());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn sidecar_no_file_returns_defaults() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("ghost.pdf");
        // Do NOT create the file — the sidecar will not exist either.
        // The functions should return empty/None gracefully.
        assert!(read_tags(&file).unwrap().is_empty());
        assert!(read_colour(&file).unwrap().is_none());
        assert!(read_original_name(&file).unwrap().is_none());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn sidecar_write_empty_tags_is_idempotent() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("empty.txt");
        std::fs::write(&file, b"dummy").unwrap();

        write_tags(&file, &[]).unwrap();
        assert!(read_tags(&file).unwrap().is_empty());

        write_tags(&file, &[]).unwrap();
        assert!(read_tags(&file).unwrap().is_empty());
    }

    // ------------------------------------------------------------------
    // macOS xattr tests (compile only on macOS)
    // ------------------------------------------------------------------

    #[cfg(target_os = "macos")]
    #[test]
    fn xattr_tags_round_trip() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("xattr_test.txt");
        std::fs::write(&file, b"hello").unwrap();

        let tags = vec!["project".to_owned(), "important".to_owned()];
        write_tags(&file, &tags).unwrap();

        let read_back = read_tags(&file).unwrap();
        assert_eq!(read_back, tags);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn xattr_remove_tags() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("xattr_remove.txt");
        std::fs::write(&file, b"hello").unwrap();

        write_tags(&file, &["inbox".to_owned()]).unwrap();
        remove_tags(&file).unwrap();

        assert!(read_tags(&file).unwrap().is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn xattr_colour_round_trip() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("xattr_colour.txt");
        std::fs::write(&file, b"hello").unwrap();

        write_colour(&file, FinderColour::Red).unwrap();
        let colour = read_colour(&file).unwrap();
        assert_eq!(colour, Some(FinderColour::Red));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn xattr_original_name_round_trip() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("xattr_orig.txt");
        std::fs::write(&file, b"hello").unwrap();

        save_original_name(&file, "before_rename.txt").unwrap();
        let name = read_original_name(&file).unwrap();
        assert_eq!(name.as_deref(), Some("before_rename.txt"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn xattr_no_tags_returns_empty() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("bare.txt");
        std::fs::write(&file, b"hello").unwrap();

        assert!(read_tags(&file).unwrap().is_empty());
        assert!(read_original_name(&file).unwrap().is_none());
    }
}
