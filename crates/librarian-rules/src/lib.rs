pub mod engine;
pub mod loader;
pub mod suggestion;

// Re-export commonly used types.
pub use engine::RuleEngine;
pub use loader::{RuleSet, load_rules, load_rules_from_str};
pub use suggestion::{CorrectionRecord, SuggestedRule, read_correction_records, suggest_rules};
