//! Confidence gating logic

use librarian_core::config::Thresholds;
use serde::{Deserialize, Serialize};

/// A confidence gate that decides whether a classification result is accepted,
/// should be escalated to the next tier, or needs human review.
#[derive(Debug, Clone)]
pub struct ConfidenceGate {
    thresholds: Thresholds,
}

/// The result of a confidence check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GateResult {
    /// Classification is accepted at the given confidence level.
    Accept {
        destination: String,
        confidence: f64,
    },
    /// Confidence was too low — escalate to the next classification tier.
    Escalate,
    /// Even the final tier (LLM) was below threshold — flag for human review.
    NeedsReview { reason: String },
}

impl ConfidenceGate {
    /// Create a new confidence gate with the given thresholds.
    pub fn new(thresholds: Thresholds) -> Self {
        Self { thresholds }
    }

    /// Check a filename embedding similarity score.
    ///
    /// Accepts if the similarity meets the filename embedding threshold,
    /// otherwise escalates to the next tier.
    pub fn check_filename_embedding(&self, similarity: f64, destination: &str) -> GateResult {
        if similarity >= self.thresholds.filename_embedding {
            GateResult::Accept {
                destination: destination.to_string(),
                confidence: similarity,
            }
        } else {
            GateResult::Escalate
        }
    }

    /// Check a content embedding similarity score.
    ///
    /// Accepts if the similarity meets the content embedding threshold,
    /// otherwise escalates to the next tier.
    pub fn check_content_embedding(&self, similarity: f64, destination: &str) -> GateResult {
        if similarity >= self.thresholds.content_embedding {
            GateResult::Accept {
                destination: destination.to_string(),
                confidence: similarity,
            }
        } else {
            GateResult::Escalate
        }
    }

    /// Check the LLM self-reported confidence score.
    ///
    /// Accepts if confidence meets the LLM threshold, otherwise returns
    /// `NeedsReview` with a reason.
    pub fn check_llm_confidence(&self, confidence: f64, destination: &str) -> GateResult {
        if confidence >= self.thresholds.llm_confidence {
            GateResult::Accept {
                destination: destination.to_string(),
                confidence,
            }
        } else {
            GateResult::NeedsReview {
                reason: format!(
                    "LLM confidence {confidence:.2} is below threshold {:.2}",
                    self.thresholds.llm_confidence
                ),
            }
        }
    }

    /// Get a reference to the thresholds.
    pub fn thresholds(&self) -> &Thresholds {
        &self.thresholds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_gate() -> ConfidenceGate {
        ConfidenceGate::new(Thresholds::default())
    }

    fn custom_gate(filename: f64, content: f64, llm: f64) -> ConfidenceGate {
        ConfidenceGate::new(Thresholds {
            filename_embedding: filename,
            content_embedding: content,
            llm_confidence: llm,
        })
    }

    #[test]
    fn filename_embedding_accept() {
        let gate = default_gate(); // threshold = 0.80
        match gate.check_filename_embedding(0.85, "Documents") {
            GateResult::Accept {
                destination,
                confidence,
            } => {
                assert_eq!(destination, "Documents");
                assert!((confidence - 0.85).abs() < 1e-6);
            }
            other => panic!("expected Accept, got {other:?}"),
        }
    }

    #[test]
    fn filename_embedding_escalate() {
        let gate = default_gate();
        match gate.check_filename_embedding(0.75, "Documents") {
            GateResult::Escalate => {}
            other => panic!("expected Escalate, got {other:?}"),
        }
    }

    #[test]
    fn filename_embedding_exact_threshold() {
        let gate = default_gate();
        match gate.check_filename_embedding(0.80, "Documents") {
            GateResult::Accept { .. } => {}
            other => panic!("expected Accept at exact threshold, got {other:?}"),
        }
    }

    #[test]
    fn content_embedding_accept() {
        let gate = default_gate(); // threshold = 0.75
        match gate.check_content_embedding(0.80, "Photos") {
            GateResult::Accept {
                destination,
                confidence,
            } => {
                assert_eq!(destination, "Photos");
                assert!((confidence - 0.80).abs() < 1e-6);
            }
            other => panic!("expected Accept, got {other:?}"),
        }
    }

    #[test]
    fn content_embedding_escalate() {
        let gate = default_gate();
        match gate.check_content_embedding(0.60, "Photos") {
            GateResult::Escalate => {}
            other => panic!("expected Escalate, got {other:?}"),
        }
    }

    #[test]
    fn llm_confidence_accept() {
        let gate = default_gate(); // threshold = 0.70
        match gate.check_llm_confidence(0.85, "Invoices") {
            GateResult::Accept {
                destination,
                confidence,
            } => {
                assert_eq!(destination, "Invoices");
                assert!((confidence - 0.85).abs() < 1e-6);
            }
            other => panic!("expected Accept, got {other:?}"),
        }
    }

    #[test]
    fn llm_confidence_needs_review() {
        let gate = default_gate();
        match gate.check_llm_confidence(0.50, "Invoices") {
            GateResult::NeedsReview { reason } => {
                assert!(reason.contains("0.50"), "reason should contain confidence");
                assert!(reason.contains("0.70"), "reason should contain threshold");
            }
            other => panic!("expected NeedsReview, got {other:?}"),
        }
    }

    #[test]
    fn custom_thresholds() {
        let gate = custom_gate(0.90, 0.85, 0.60);

        // Would pass default but fail custom
        match gate.check_filename_embedding(0.85, "Docs") {
            GateResult::Escalate => {}
            other => panic!("expected Escalate with high threshold, got {other:?}"),
        }

        // Would fail default but pass custom
        match gate.check_llm_confidence(0.65, "Docs") {
            GateResult::Accept { .. } => {}
            other => panic!("expected Accept with low threshold, got {other:?}"),
        }
    }
}
