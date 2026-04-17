pub mod engine;
pub mod loader;
pub mod suggestion;

// Re-export commonly used types.
pub use engine::RuleEngine;
pub use loader::{load_rules, load_rules_from_str, RuleSet};
