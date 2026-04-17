pub mod config;
pub mod decision;
pub mod file_entry;
pub mod hasher;
pub mod ignore;
pub mod plan;
pub mod tags;
pub mod walker;

// Re-export commonly used types.
pub use config::{AppConfig, ProviderConfig, ProviderType, Thresholds};
pub use decision::{
    append_decision, read_decisions, ClassificationMethod, Decision, DecisionOutcome, DecisionType,
};
pub use file_entry::{FileEntry, FinderColour};
pub use ignore::IgnoreEngine;
