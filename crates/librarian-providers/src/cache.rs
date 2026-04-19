//! Embedding cache with blake3-keyed hashing and msgpack persistence.

use std::collections::HashMap;
use std::path::Path;

use tracing::{debug, warn};

/// Cache for embedding vectors, keyed by blake3 hash of the input text.
///
/// Persisted to disk as msgpack. Detects corruption on load and falls back
/// to an empty cache.
#[derive(Debug)]
pub struct EmbeddingCache {
    entries: HashMap<String, Vec<f32>>,
}

impl EmbeddingCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Load cache from a msgpack file at `path`.
    ///
    /// If the file does not exist, returns an empty cache.
    /// If the file is corrupted (invalid msgpack), logs a warning and
    /// returns an empty cache.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            debug!(?path, "Cache file not found, starting empty");
            return Ok(Self::new());
        }

        let data = std::fs::read(path)?;
        match rmp_serde::from_slice::<HashMap<String, Vec<f32>>>(&data) {
            Ok(entries) => {
                debug!(count = entries.len(), "Loaded embedding cache");
                Ok(Self { entries })
            }
            Err(e) => {
                warn!(?path, error = %e, "Corrupt cache file, starting empty");
                Ok(Self::new())
            }
        }
    }

    /// Save the cache to a msgpack file at `path`.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = rmp_serde::to_vec(&self.entries)?;
        std::fs::write(path, data)?;
        debug!(count = self.entries.len(), ?path, "Saved embedding cache");
        Ok(())
    }

    /// Look up a cached embedding by the original text key.
    ///
    /// The key is hashed internally with blake3 for consistent storage.
    pub fn get(&self, key: &str) -> Option<&Vec<f32>> {
        let hash_key = hash_key(key);
        self.entries.get(&hash_key)
    }

    /// Insert an embedding for the given text key.
    ///
    /// The key is hashed internally with blake3.
    pub fn insert(&mut self, key: &str, embedding: Vec<f32>) {
        let hash_key = hash_key(key);
        self.entries.insert(hash_key, embedding);
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for EmbeddingCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Produce a hex-encoded blake3 hash of the input key.
fn hash_key(key: &str) -> String {
    blake3::hash(key.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.msgpack");

        let mut cache = EmbeddingCache::new();
        cache.insert("hello", vec![0.1, 0.2, 0.3]);
        cache.insert("world", vec![0.4, 0.5, 0.6]);
        cache.save(&path).unwrap();

        let loaded = EmbeddingCache::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get("hello").unwrap(), &vec![0.1, 0.2, 0.3]);
        assert_eq!(loaded.get("world").unwrap(), &vec![0.4, 0.5, 0.6]);
    }

    #[test]
    fn cache_miss() {
        let cache = EmbeddingCache::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn corruption_detection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.msgpack");

        // Write garbage data.
        std::fs::write(&path, b"this is not valid msgpack data!!!").unwrap();

        let cache = EmbeddingCache::load(&path).unwrap();
        assert!(cache.is_empty(), "Corrupt file should yield empty cache");
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does_not_exist.msgpack");

        let cache = EmbeddingCache::load(&path).unwrap();
        assert!(cache.is_empty());
    }

    #[test]
    fn insert_overwrites() {
        let mut cache = EmbeddingCache::new();
        cache.insert("key", vec![1.0]);
        cache.insert("key", vec![2.0]);
        assert_eq!(cache.get("key").unwrap(), &vec![2.0]);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn hash_key_is_deterministic() {
        let h1 = hash_key("test");
        let h2 = hash_key("test");
        assert_eq!(h1, h2);
        // blake3 hex is 64 chars.
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deep/cache.msgpack");

        let mut cache = EmbeddingCache::new();
        cache.insert("key", vec![1.0, 2.0]);
        cache.save(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn default_is_empty() {
        let cache = EmbeddingCache::default();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn different_keys_produce_different_hashes() {
        let h1 = hash_key("hello");
        let h2 = hash_key("world");
        assert_ne!(h1, h2);
    }
}
