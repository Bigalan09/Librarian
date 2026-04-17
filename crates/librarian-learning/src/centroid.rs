//! Centroid drift calculation

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Key for centroid lookup: (source_inbox, filetype, bucket_name).
///
/// Per-folder and per-filetype scoping is enforced by the key structure.
pub type CentroidKey = (String, String, String);

/// Stores running-average centroids for classification buckets,
/// scoped by source inbox and filetype.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CentroidStore {
    centroids: HashMap<CentroidKey, Vec<f32>>,
}

impl CentroidStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self {
            centroids: HashMap::new(),
        }
    }

    /// Load a centroid store from a msgpack file.
    pub fn load(path: &Path) -> anyhow::Result<CentroidStore> {
        if !path.exists() {
            return Ok(CentroidStore::new());
        }
        let data = std::fs::read(path)?;
        let store: CentroidStore = rmp_serde::from_slice(&data)?;
        Ok(store)
    }

    /// Save the centroid store to a msgpack file.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = rmp_serde::to_vec(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Update a centroid with a new embedding using a weighted running average.
    ///
    /// Formula: `centroid = (1 - learning_rate) * old + learning_rate * new`
    ///
    /// If the key does not exist yet, the new embedding is stored directly.
    pub fn update_centroid(
        &mut self,
        key: CentroidKey,
        new_embedding: &[f32],
        learning_rate: f32,
    ) {
        if let Some(existing) = self.centroids.get_mut(&key) {
            // Ensure dimensions match; if they don't, replace entirely
            if existing.len() == new_embedding.len() {
                for (old, new) in existing.iter_mut().zip(new_embedding.iter()) {
                    *old = (1.0 - learning_rate) * *old + learning_rate * *new;
                }
            } else {
                *existing = new_embedding.to_vec();
            }
        } else {
            self.centroids.insert(key, new_embedding.to_vec());
        }
    }

    /// Get the centroid for a given key, if it exists.
    pub fn get(&self, key: &CentroidKey) -> Option<&Vec<f32>> {
        self.centroids.get(key)
    }
}

impl Default for CentroidStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(inbox: &str, filetype: &str, bucket: &str) -> CentroidKey {
        (inbox.to_string(), filetype.to_string(), bucket.to_string())
    }

    #[test]
    fn update_shifts_centroid() {
        let mut store = CentroidStore::new();
        let k = key("Downloads", "pdf", "Invoices");

        // First update sets the centroid directly
        store.update_centroid(k.clone(), &[1.0, 0.0, 0.0], 0.5);
        assert_eq!(store.get(&k).unwrap(), &vec![1.0, 0.0, 0.0]);

        // Second update with learning_rate=0.5 should average
        store.update_centroid(k.clone(), &[0.0, 1.0, 0.0], 0.5);
        let result = store.get(&k).unwrap();
        assert!((result[0] - 0.5).abs() < 1e-6);
        assert!((result[1] - 0.5).abs() < 1e-6);
        assert!((result[2] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn load_save_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("centroids.msgpack");

        let mut store = CentroidStore::new();
        let k = key("Desktop", "png", "Screenshots");
        store.update_centroid(k.clone(), &[0.1, 0.2, 0.3], 1.0);

        store.save(&path).unwrap();

        let loaded = CentroidStore::load(&path).unwrap();
        let centroid = loaded.get(&k).unwrap();
        assert!((centroid[0] - 0.1).abs() < 1e-6);
        assert!((centroid[1] - 0.2).abs() < 1e-6);
        assert!((centroid[2] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn missing_key_returns_none() {
        let store = CentroidStore::new();
        let k = key("Nowhere", "xyz", "Nothing");
        assert!(store.get(&k).is_none());
    }

    #[test]
    fn per_key_isolation() {
        let mut store = CentroidStore::new();
        let k1 = key("Downloads", "pdf", "Invoices");
        let k2 = key("Desktop", "pdf", "Invoices");
        let k3 = key("Downloads", "txt", "Invoices");

        store.update_centroid(k1.clone(), &[1.0, 0.0], 1.0);
        store.update_centroid(k2.clone(), &[0.0, 1.0], 1.0);
        store.update_centroid(k3.clone(), &[0.5, 0.5], 1.0);

        assert_eq!(store.get(&k1).unwrap(), &vec![1.0, 0.0]);
        assert_eq!(store.get(&k2).unwrap(), &vec![0.0, 1.0]);
        assert_eq!(store.get(&k3).unwrap(), &vec![0.5, 0.5]);
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let store = CentroidStore::load(Path::new("/nonexistent.msgpack")).unwrap();
        assert!(store.get(&key("a", "b", "c")).is_none());
    }
}
