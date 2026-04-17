//! Qdrant vector DB integration
//!
//! This is currently a stub implementation. The actual Qdrant integration
//! requires a running Qdrant instance and will be implemented when the
//! infrastructure is ready.

use serde::{Deserialize, Serialize};
use tracing::warn;

/// A search result from Qdrant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub payload: serde_json::Value,
}

/// Qdrant vector store client.
///
/// Wraps the `qdrant_client::Qdrant` client for storing and querying
/// file embedding vectors.
pub struct QdrantStore {
    _url: String,
    _collection_name: String,
}

impl QdrantStore {
    /// Create a new Qdrant store (stub — does not actually connect).
    pub async fn new(url: &str, collection_name: &str) -> anyhow::Result<Self> {
        warn!("Qdrant not yet connected — using stub implementation");
        Ok(Self {
            _url: url.to_string(),
            _collection_name: collection_name.to_string(),
        })
    }

    /// Upsert a vector with payload (stub — logs and returns Ok).
    pub async fn upsert(
        &self,
        id: &str,
        _vector: Vec<f32>,
        _payload: serde_json::Value,
    ) -> anyhow::Result<()> {
        warn!(id = id, "Qdrant not yet connected — upsert is a no-op");
        Ok(())
    }

    /// Search for the top-k nearest vectors (stub — returns empty results).
    pub async fn search(
        &self,
        _vector: Vec<f32>,
        _top_k: usize,
    ) -> anyhow::Result<Vec<SearchResult>> {
        warn!("Qdrant not yet connected — search returns empty results");
        Ok(Vec::new())
    }
}
