//! Vector store abstraction.
//!
//! Defines the `VectorStore` trait so the classification pipeline is not
//! coupled to any specific vector backend. Ships with an `InMemoryVectorStore`
//! backed by `librarian_learning::CentroidStore` (msgpack persistence).
//! A Qdrant implementation can be swapped in later by implementing the trait.

use std::path::{Path, PathBuf};

use librarian_learning::centroid::{CentroidKey, CentroidStore};
use tracing::debug;

/// Result from a nearest-neighbour search.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// The bucket/category name that matched.
    pub bucket: String,
    /// Cosine similarity score.
    pub score: f32,
}

/// Trait abstracting vector storage and retrieval.
///
/// Implementations must support:
/// - Upserting embeddings keyed by (source_inbox, filetype, bucket)
/// - Nearest-neighbour search within a scope
/// - Listing known bucket names
pub trait VectorStore: Send + Sync {
    /// Store or update an embedding for a bucket, scoped by inbox and filetype.
    fn upsert(
        &mut self,
        source_inbox: &str,
        filetype: &str,
        bucket: &str,
        embedding: &[f32],
        learning_rate: f32,
    );

    /// Find the nearest bucket centroid to `query` within the given scope.
    ///
    /// Returns `None` if no centroids exist for this scope.
    fn find_nearest(&self, source_inbox: &str, filetype: &str, query: &[f32]) -> Option<SearchHit>;

    /// Return all known bucket names (across all scopes).
    fn all_buckets(&self) -> Vec<String>;

    /// Whether the store has any data.
    fn is_empty(&self) -> bool;

    /// Persist the store to disk (no-op for backends with their own persistence).
    fn save(&self) -> anyhow::Result<()>;
}

/// In-memory vector store backed by `CentroidStore` with msgpack persistence.
///
/// This is the default backend. It stores running-average centroids per
/// (source_inbox, filetype, bucket) key and persists them as a single
/// msgpack file.
pub struct InMemoryVectorStore {
    store: CentroidStore,
    persist_path: PathBuf,
}

impl InMemoryVectorStore {
    /// Load from disk or create empty. The file is created on first `save()`.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let store = CentroidStore::load(path)?;
        debug!(
            centroids = store.len(),
            path = %path.display(),
            "Loaded in-memory vector store"
        );
        Ok(Self {
            store,
            persist_path: path.to_path_buf(),
        })
    }

    /// Create an empty store that will persist to `path`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            store: CentroidStore::new(),
            persist_path: path,
        }
    }

    /// Borrow the underlying CentroidStore.
    pub fn inner(&self) -> &CentroidStore {
        &self.store
    }
}

impl VectorStore for InMemoryVectorStore {
    fn upsert(
        &mut self,
        source_inbox: &str,
        filetype: &str,
        bucket: &str,
        embedding: &[f32],
        learning_rate: f32,
    ) {
        let key: CentroidKey = (
            source_inbox.to_string(),
            filetype.to_string(),
            bucket.to_string(),
        );
        self.store.update_centroid(key, embedding, learning_rate);
    }

    fn find_nearest(&self, source_inbox: &str, filetype: &str, query: &[f32]) -> Option<SearchHit> {
        self.store
            .find_nearest(source_inbox, filetype, query)
            .map(|(bucket, score)| SearchHit { bucket, score })
    }

    fn all_buckets(&self) -> Vec<String> {
        self.store.all_buckets()
    }

    fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    fn save(&self) -> anyhow::Result<()> {
        self.store.save(&self.persist_path)?;
        debug!(
            centroids = self.store.len(),
            path = %self.persist_path.display(),
            "Saved in-memory vector store"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_upsert_and_find() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vectors.msgpack");
        let mut store = InMemoryVectorStore::new(path);

        store.upsert("Downloads", "pdf", "Invoices", &[1.0, 0.0, 0.0], 1.0);
        store.upsert("Downloads", "pdf", "Photos", &[0.0, 1.0, 0.0], 1.0);

        let hit = store
            .find_nearest("Downloads", "pdf", &[0.9, 0.1, 0.0])
            .unwrap();
        assert_eq!(hit.bucket, "Invoices");
        assert!(hit.score > 0.9);
    }

    #[test]
    fn in_memory_all_buckets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vectors.msgpack");
        let mut store = InMemoryVectorStore::new(path);

        store.upsert("Downloads", "pdf", "Invoices", &[1.0], 1.0);
        store.upsert("Desktop", "png", "Screenshots", &[1.0], 1.0);

        let buckets = store.all_buckets();
        assert!(buckets.contains(&"Invoices".to_string()));
        assert!(buckets.contains(&"Screenshots".to_string()));
    }

    #[test]
    fn in_memory_persist_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vectors.msgpack");

        {
            let mut store = InMemoryVectorStore::new(path.clone());
            store.upsert("Downloads", "pdf", "Work", &[0.5, 0.5, 0.0], 1.0);
            store.save().unwrap();
        }

        let loaded = InMemoryVectorStore::load(&path).unwrap();
        assert!(!loaded.is_empty());
        let hit = loaded
            .find_nearest("Downloads", "pdf", &[0.5, 0.5, 0.0])
            .unwrap();
        assert_eq!(hit.bucket, "Work");
        assert!((hit.score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn in_memory_empty_scope_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vectors.msgpack");
        let store = InMemoryVectorStore::new(path);

        assert!(
            store
                .find_nearest("Downloads", "pdf", &[1.0, 0.0])
                .is_none()
        );
    }

    #[test]
    fn in_memory_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vectors.msgpack");
        let mut store = InMemoryVectorStore::new(path);

        assert!(store.is_empty());
        store.upsert("Downloads", "pdf", "Invoices", &[1.0], 1.0);
        assert!(!store.is_empty());
    }

    #[test]
    fn upsert_updates_centroid_with_learning_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vectors.msgpack");
        let mut store = InMemoryVectorStore::new(path);

        store.upsert("Downloads", "pdf", "Invoices", &[1.0, 0.0], 1.0);
        store.upsert("Downloads", "pdf", "Invoices", &[0.0, 1.0], 0.5);

        // With learning_rate=0.5: centroid = 0.5 * [1,0] + 0.5 * [0,1] = [0.5, 0.5]
        let hit = store.find_nearest("Downloads", "pdf", &[0.5, 0.5]).unwrap();
        assert_eq!(hit.bucket, "Invoices");
        assert!((hit.score - 1.0).abs() < 1e-5);
    }
}
