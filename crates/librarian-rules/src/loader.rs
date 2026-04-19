//! Rules YAML loader and validator

use std::path::Path;

use globset::Glob;
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use librarian_core::file_entry::FinderColour;

/// Errors that can occur while loading or validating rules.
#[derive(Debug, Error)]
pub enum RuleError {
    #[error("IO error reading rules file: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Rule '{rule}' field '{field}': invalid glob pattern '{pattern}': {source}")]
    InvalidGlob {
        rule: String,
        field: String,
        pattern: String,
        source: globset::Error,
    },

    #[error("Rule '{rule}' field '{field}': invalid regex pattern '{pattern}': {source}")]
    InvalidRegex {
        rule: String,
        field: String,
        pattern: String,
        source: regex::Error,
    },
}

/// A pattern that can be either a glob or a regex.
#[derive(Debug, Clone)]
pub enum CompiledPattern {
    Glob(globset::GlobMatcher),
    Regex(Regex),
}

impl CompiledPattern {
    /// Compile a pattern string. If it starts with `regex:`, compile as regex;
    /// otherwise compile as a glob.
    fn compile(raw: &str, rule_name: &str, field: &str) -> Result<Self, RuleError> {
        if let Some(re_str) = raw.strip_prefix("regex:") {
            let re = Regex::new(re_str).map_err(|e| RuleError::InvalidRegex {
                rule: rule_name.to_owned(),
                field: field.to_owned(),
                pattern: re_str.to_owned(),
                source: e,
            })?;
            Ok(CompiledPattern::Regex(re))
        } else {
            let glob = Glob::new(raw).map_err(|e| RuleError::InvalidGlob {
                rule: rule_name.to_owned(),
                field: field.to_owned(),
                pattern: raw.to_owned(),
                source: e,
            })?;
            Ok(CompiledPattern::Glob(glob.compile_matcher()))
        }
    }

    /// Test whether the given text matches this pattern.
    pub fn is_match(&self, text: &str) -> bool {
        match self {
            CompiledPattern::Glob(m) => m.is_match(text),
            CompiledPattern::Regex(r) => r.is_match(text),
        }
    }
}

/// Raw YAML match criteria (deserialized directly from YAML).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawMatchCriteria {
    pub extension: Option<String>,
    pub filename: Option<String>,
    pub path: Option<String>,
    pub content: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
}

/// Compiled match criteria with pre-compiled patterns.
#[derive(Debug, Clone)]
pub struct MatchCriteria {
    pub extension: Option<String>,
    pub filename: Option<CompiledPattern>,
    pub path: Option<CompiledPattern>,
    pub content: Option<CompiledPattern>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
}

/// Raw rule as deserialized from YAML.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct RawRule {
    name: String,
    #[serde(rename = "match")]
    match_criteria: RawMatchCriteria,
    destination: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    colour: Option<FinderColour>,
    #[serde(default)]
    clean_name: bool,
}

/// Raw YAML root document.
#[derive(Debug, Deserialize, Serialize)]
struct RawRuleSet {
    rules: Vec<RawRule>,
}

/// A single validated and compiled rule.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub match_criteria: MatchCriteria,
    pub destination: String,
    pub tags: Vec<String>,
    pub colour: Option<FinderColour>,
    pub clean_name: bool,
}

/// A set of compiled rules ready for matching.
#[derive(Debug, Clone)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

/// Compile a raw rule into a validated Rule with compiled patterns.
fn compile_rule(raw: RawRule) -> Result<Rule, RuleError> {
    let mc = &raw.match_criteria;
    let name = &raw.name;

    let filename = mc
        .filename
        .as_deref()
        .map(|p| CompiledPattern::compile(p, name, "filename"))
        .transpose()?;

    let path = mc
        .path
        .as_deref()
        .map(|p| CompiledPattern::compile(p, name, "path"))
        .transpose()?;

    let content = mc
        .content
        .as_deref()
        .map(|p| CompiledPattern::compile(p, name, "content"))
        .transpose()?;

    Ok(Rule {
        name: raw.name,
        match_criteria: MatchCriteria {
            extension: mc.extension.clone(),
            filename,
            path,
            content,
            min_size: mc.min_size,
            max_size: mc.max_size,
        },
        destination: raw.destination,
        tags: raw.tags,
        colour: raw.colour,
        clean_name: raw.clean_name,
    })
}

/// Load and validate rules from a YAML string.
pub fn load_rules_from_str(yaml: &str) -> Result<RuleSet, RuleError> {
    let raw: RawRuleSet = serde_yaml::from_str(yaml)?;
    let mut rules = Vec::with_capacity(raw.rules.len());
    for raw_rule in raw.rules {
        rules.push(compile_rule(raw_rule)?);
    }
    Ok(RuleSet { rules })
}

/// Load and validate rules from a YAML file.
pub fn load_rules(path: &Path) -> Result<RuleSet, RuleError> {
    let contents = std::fs::read_to_string(path)?;
    load_rules_from_str(&contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_yaml_with_glob_and_regex() {
        let yaml = r#"
rules:
  - name: "Work invoices"
    match:
      extension: "pdf"
      filename: "*invoice*"
      path: "*/Downloads/*"
    destination: "{year}/Work/Invoices"
    tags: ["invoice", "work"]

  - name: "Screenshots"
    match:
      filename: "regex:^Screenshot \\d{4}-\\d{2}-\\d{2}"
      extension: "png"
    destination: "{year}/Personal/Screenshots"
    tags: ["screenshot"]
    clean_name: true
"#;
        let ruleset = load_rules_from_str(yaml).unwrap();
        assert_eq!(ruleset.rules.len(), 2);
        assert_eq!(ruleset.rules[0].name, "Work invoices");
        assert_eq!(ruleset.rules[1].name, "Screenshots");
        assert!(ruleset.rules[1].clean_name);
        assert!(!ruleset.rules[0].clean_name);
    }

    #[test]
    fn validation_error_for_invalid_regex() {
        let yaml = r#"
rules:
  - name: "Bad regex"
    match:
      filename: "regex:*invalid["
    destination: "dest"
"#;
        let err = load_rules_from_str(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Bad regex"),
            "error should name the rule: {msg}"
        );
        assert!(
            msg.contains("filename"),
            "error should name the field: {msg}"
        );
    }

    #[test]
    fn validation_error_for_invalid_glob() {
        let yaml = r#"
rules:
  - name: "Bad glob"
    match:
      filename: "[invalid"
    destination: "dest"
"#;
        let err = load_rules_from_str(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Bad glob"),
            "error should name the rule: {msg}"
        );
    }

    #[test]
    fn parse_rules_with_all_optional_fields() {
        let yaml = r#"
rules:
  - name: "Minimal"
    match: {}
    destination: "somewhere"

  - name: "Full"
    match:
      extension: "txt"
      filename: "*.txt"
      path: "*/docs/*"
      content: "regex:TODO"
      min_size: 100
      max_size: 10000
    destination: "{year}/{ext}"
    tags: ["docs"]
    colour: "blue"
    clean_name: true
"#;
        let ruleset = load_rules_from_str(yaml).unwrap();
        assert_eq!(ruleset.rules.len(), 2);

        let minimal = &ruleset.rules[0];
        assert!(minimal.match_criteria.extension.is_none());
        assert!(minimal.match_criteria.filename.is_none());
        assert!(minimal.match_criteria.min_size.is_none());
        assert!(minimal.colour.is_none());

        let full = &ruleset.rules[1];
        assert_eq!(full.match_criteria.extension.as_deref(), Some("txt"));
        assert!(full.match_criteria.filename.is_some());
        assert!(full.match_criteria.content.is_some());
        assert_eq!(full.match_criteria.min_size, Some(100));
        assert_eq!(full.match_criteria.max_size, Some(10000));
        assert_eq!(full.colour, Some(FinderColour::Blue));
        assert!(full.clean_name);
    }

    #[test]
    fn first_match_wins_precedence() {
        let yaml = r#"
rules:
  - name: "First"
    match:
      extension: "pdf"
    destination: "first"
  - name: "Second"
    match:
      extension: "pdf"
    destination: "second"
"#;
        let ruleset = load_rules_from_str(yaml).unwrap();
        // Both rules match pdf, but first should be at index 0
        assert_eq!(ruleset.rules[0].name, "First");
        assert_eq!(ruleset.rules[1].name, "Second");
    }

    #[test]
    fn load_rules_io_error_for_missing_file() {
        let result = load_rules(std::path::Path::new(
            "/nonexistent_librarian_test/rules.yaml",
        ));
        assert!(result.is_err());
    }

    #[test]
    fn load_rules_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("rules.yaml");
        std::fs::write(
            &file_path,
            r#"
rules:
  - name: "Test"
    match:
      extension: "txt"
    destination: "dest"
"#,
        )
        .unwrap();

        let ruleset = load_rules(&file_path).unwrap();
        assert_eq!(ruleset.rules.len(), 1);
        assert_eq!(ruleset.rules[0].name, "Test");
    }
}
