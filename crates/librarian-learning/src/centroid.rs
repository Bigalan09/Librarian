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
    pub fn update_centroid(&mut self, key: CentroidKey, new_embedding: &[f32], learning_rate: f32) {
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

    /// Return all unique bucket names across all keys.
    pub fn all_buckets(&self) -> Vec<String> {
        let mut buckets: Vec<String> = self
            .centroids
            .keys()
            .map(|(_, _, bucket)| bucket.clone())
            .collect();
        buckets.sort();
        buckets.dedup();
        buckets
    }

    /// Return all centroids that match a given source_inbox and filetype.
    ///
    /// Returns `(bucket_name, centroid_vector)` pairs for similarity comparison.
    pub fn centroids_for_scope(
        &self,
        source_inbox: &str,
        filetype: &str,
    ) -> Vec<(&str, &Vec<f32>)> {
        self.centroids
            .iter()
            .filter(|((inbox, ft, _), _)| inbox == source_inbox && ft == filetype)
            .map(|((_, _, bucket), vec)| (bucket.as_str(), vec))
            .collect()
    }

    /// Find the nearest centroid to a query vector within a scope.
    ///
    /// Returns `(bucket_name, similarity_score)` for the best match, or None
    /// if no centroids exist for this scope.
    pub fn find_nearest(
        &self,
        source_inbox: &str,
        filetype: &str,
        query: &[f32],
    ) -> Option<(String, f32)> {
        let scoped = self.centroids_for_scope(source_inbox, filetype);
        if scoped.is_empty() {
            return None;
        }

        let mut best_score = f32::NEG_INFINITY;
        let mut best_bucket = String::new();

        for (bucket, centroid) in scoped {
            let score = cosine_similarity(query, centroid);
            if score > best_score {
                best_score = score;
                best_bucket = bucket.to_string();
            }
        }

        Some((best_bucket, best_score))
    }

    /// Check whether the store has any centroids at all.
    pub fn is_empty(&self) -> bool {
        self.centroids.is_empty()
    }

    /// Number of centroids stored.
    pub fn len(&self) -> usize {
        self.centroids.len()
    }
}

/// Compute cosine similarity between two vectors.
///
/// Returns 0.0 if either vector has zero magnitude.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a * mag_b)
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

    #[test]
    fn all_buckets_returns_unique_sorted() {
        let mut store = CentroidStore::new();
        store.update_centroid(key("Downloads", "pdf", "Invoices"), &[1.0], 1.0);
        store.update_centroid(key("Desktop", "pdf", "Invoices"), &[1.0], 1.0);
        store.update_centroid(key("Downloads", "png", "Screenshots"), &[1.0], 1.0);
        store.update_centroid(key("Downloads", "pdf", "Work"), &[1.0], 1.0);

        let buckets = store.all_buckets();
        assert_eq!(buckets, vec!["Invoices", "Screenshots", "Work"]);
    }

    #[test]
    fn centroids_for_scope_filters_correctly() {
        let mut store = CentroidStore::new();
        store.update_centroid(key("Downloads", "pdf", "Invoices"), &[1.0, 0.0], 1.0);
        store.update_centroid(key("Downloads", "pdf", "Work"), &[0.0, 1.0], 1.0);
        store.update_centroid(key("Desktop", "pdf", "Invoices"), &[0.5, 0.5], 1.0);
        store.update_centroid(key("Downloads", "txt", "Notes"), &[0.3, 0.7], 1.0);

        let scoped = store.centroids_for_scope("Downloads", "pdf");
        assert_eq!(scoped.len(), 2);

        let bucket_names: Vec<&str> = scoped.iter().map(|(b, _)| *b).collect();
        assert!(bucket_names.contains(&"Invoices"));
        assert!(bucket_names.contains(&"Work"));
    }

    #[test]
    fn find_nearest_returns_best_match() {
        let mut store = CentroidStore::new();
        store.update_centroid(key("Downloads", "pdf", "Invoices"), &[1.0, 0.0, 0.0], 1.0);
        store.update_centroid(key("Downloads", "pdf", "Photos"), &[0.0, 1.0, 0.0], 1.0);
        store.update_centroid(key("Downloads", "pdf", "Code"), &[0.0, 0.0, 1.0], 1.0);

        // Query vector closest to Invoices
        let result = store.find_nearest("Downloads", "pdf", &[0.9, 0.1, 0.0]);
        assert!(result.is_some());
        let (bucket, score) = result.unwrap();
        assert_eq!(bucket, "Invoices");
        assert!(score > 0.9);
    }

    #[test]
    fn find_nearest_returns_none_for_empty_scope() {
        let store = CentroidStore::new();
        let result = store.find_nearest("Downloads", "pdf", &[1.0, 0.0]);
        assert!(result.is_none());
    }

    #[test]
    fn is_empty_and_len() {
        let mut store = CentroidStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        store.update_centroid(key("Downloads", "pdf", "Invoices"), &[1.0], 1.0);
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn update_centroid_learning_rate_one_replaces() {
        let mut store = CentroidStore::new();
        let k = key("Downloads", "pdf", "Invoices");

        store.update_centroid(k.clone(), &[1.0, 0.0], 1.0);
        store.update_centroid(k.clone(), &[0.0, 1.0], 1.0);

        let result = store.get(&k).unwrap();
        assert!((result[0] - 0.0).abs() < 1e-6);
        assert!((result[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn update_centroid_learning_rate_zero_preserves() {
        let mut store = CentroidStore::new();
        let k = key("Downloads", "pdf", "Invoices");

        store.update_centroid(k.clone(), &[1.0, 0.0], 1.0);
        store.update_centroid(k.clone(), &[0.0, 1.0], 0.0);

        let result = store.get(&k).unwrap();
        assert!((result[0] - 1.0).abs() < 1e-6);
        assert!((result[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deep/centroids.msgpack");

        let mut store = CentroidStore::new();
        store.update_centroid(key("Downloads", "pdf", "Work"), &[1.0], 1.0);
        store.save(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn cosine_similarity_mismatched_returns_zero() {
        // The private cosine_similarity returns 0.0 for mismatched lengths
        let result = cosine_similarity(&[1.0, 2.0], &[1.0, 2.0, 3.0]);
        assert!((result - 0.0).abs() < 1e-6);
    }

    #[test]
    fn find_nearest_with_single_centroid() {
        let mut store = CentroidStore::new();
        store.update_centroid(key("Downloads", "pdf", "Only"), &[0.5, 0.5], 1.0);

        let result = store.find_nearest("Downloads", "pdf", &[1.0, 0.0]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "Only");
    }
}
