pub mod engine;
pub mod loader;
pub mod suggestion;

// Re-export commonly used types.
pub use engine::RuleEngine;
pub use loader::{load_rules, load_rules_from_str, RuleSet};
pub use suggestion::{
    read_correction_records, suggest_rules, CorrectionRecord, SuggestedRule,
};
