//! Embedding and cosine similarity

use librarian_providers::router::ErasedProvider;
use librarian_providers::traits::Provider;

/// Compute cosine similarity between two vectors.
///
/// Returns `dot(a, b) / (||a|| * ||b||)`. Returns 0.0 if either vector has
/// zero magnitude.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "vectors must have the same dimension");

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Embed a single text string using the provider's embedding model.
///
/// Calls `provider.embed` with a single-element batch and returns the first
/// (and only) embedding vector.
pub async fn embed_text<P: Provider>(provider: &P, text: &str) -> anyhow::Result<Vec<f32>> {
    let mut results = provider.embed(vec![text.to_string()]).await?;
    results
        .pop()
        .ok_or_else(|| anyhow::anyhow!("provider returned no embeddings"))
}

/// Embed multiple texts in a single batch call.
pub async fn embed_batch<P: Provider>(
    provider: &P,
    texts: Vec<String>,
) -> anyhow::Result<Vec<Vec<f32>>> {
    provider.embed(texts).await
}

/// Embed a single text using a dyn-compatible ErasedProvider.
pub async fn embed_text_dyn(
    provider: &dyn ErasedProvider,
    text: &str,
) -> anyhow::Result<Vec<f32>> {
    let mut results = provider.embed(vec![text.to_string()]).await?;
    results
        .pop()
        .ok_or_else(|| anyhow::anyhow!("provider returned no embeddings"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_similarity_is_one() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6, "expected ~1.0, got {sim}");
    }

    #[test]
    fn orthogonal_vectors_similarity_is_zero() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "expected ~0.0, got {sim}");
    }

    #[test]
    fn known_values() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        // dot = 4+10+18 = 32
        // ||a|| = sqrt(14), ||b|| = sqrt(77)
        let expected = 32.0_f32 / (14.0_f32.sqrt() * 77.0_f32.sqrt());
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim - expected).abs() < 1e-5,
            "expected {expected}, got {sim}"
        );
    }

    #[test]
    fn opposite_vectors_similarity_is_negative_one() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6, "expected ~-1.0, got {sim}");
    }

    #[test]
    fn zero_vector_returns_zero() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "expected 0.0, got {sim}");
    }
}
