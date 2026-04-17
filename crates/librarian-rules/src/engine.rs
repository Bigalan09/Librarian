//! Rule matching engine

use chrono::Datelike;
use librarian_core::file_entry::FileEntry;

use crate::loader::{MatchCriteria, Rule, RuleSet};

/// The rule matching engine. Holds a compiled `RuleSet` and evaluates
/// `FileEntry` values against the rules in definition order (first match wins).
#[derive(Debug, Clone)]
pub struct RuleEngine {
    rules: Vec<Rule>,
}

impl RuleEngine {
    /// Create a new engine from a loaded `RuleSet`.
    pub fn new(ruleset: RuleSet) -> Self {
        Self {
            rules: ruleset.rules,
        }
    }

    /// Evaluate a file entry against all rules. Returns the first matching rule, or `None`.
    pub fn evaluate(&self, entry: &FileEntry) -> Option<&Rule> {
        self.rules.iter().find(|rule| matches_rule(rule, entry))
    }

    /// Expand template variables in a destination string for a given file entry.
    ///
    /// Supported variables:
    /// - `{year}` — year from the file's `modified_at`
    /// - `{month}` — zero-padded month from `modified_at`
    /// - `{date}` — ISO date (YYYY-MM-DD) from `modified_at`
    /// - `{ext}` — file extension (or empty string)
    /// - `{source}` — source inbox name
    pub fn expand_destination(template: &str, entry: &FileEntry) -> String {
        let dt = entry.modified_at;
        let year = dt.year().to_string();
        let month = format!("{:02}", dt.month());
        let date = dt.format("%Y-%m-%d").to_string();
        let ext = entry.extension.as_deref().unwrap_or("");
        let source = &entry.source_inbox;

        template
            .replace("{year}", &year)
            .replace("{month}", &month)
            .replace("{date}", &date)
            .replace("{ext}", ext)
            .replace("{source}", source)
    }

    /// Get a reference to the internal rules slice.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
}

/// Check if a single rule matches a file entry (AND logic on all specified fields).
fn matches_rule(rule: &Rule, entry: &FileEntry) -> bool {
    let mc = &rule.match_criteria;

    if !matches_extension(mc, entry) {
        return false;
    }
    if !matches_filename(mc, entry) {
        return false;
    }
    if !matches_path(mc, entry) {
        return false;
    }
    if !matches_size(mc, entry) {
        return false;
    }
    if !matches_content(mc, entry) {
        return false;
    }

    true
}

/// Extension: exact match, case-insensitive.
fn matches_extension(mc: &MatchCriteria, entry: &FileEntry) -> bool {
    match (&mc.extension, &entry.extension) {
        (Some(required), Some(actual)) => required.eq_ignore_ascii_case(actual),
        (Some(_), None) => false,
        (None, _) => true,
    }
}

/// Filename: glob or regex pattern match against the file name.
fn matches_filename(mc: &MatchCriteria, entry: &FileEntry) -> bool {
    match &mc.filename {
        Some(pattern) => pattern.is_match(&entry.name),
        None => true,
    }
}

/// Path: glob or regex pattern match against the full path string.
fn matches_path(mc: &MatchCriteria, entry: &FileEntry) -> bool {
    match &mc.path {
        Some(pattern) => pattern.is_match(&entry.path.to_string_lossy()),
        None => true,
    }
}

/// Size: min_size and max_size bounds.
fn matches_size(mc: &MatchCriteria, entry: &FileEntry) -> bool {
    if let Some(min) = mc.min_size
        && entry.size_bytes < min
    {
        return false;
    }
    if let Some(max) = mc.max_size
        && entry.size_bytes > max
    {
        return false;
    }
    true
}

/// Content: pattern match against file contents (for text-based files).
/// Reads the file from disk; if the file can't be read, returns false.
fn matches_content(mc: &MatchCriteria, entry: &FileEntry) -> bool {
    match &mc.content {
        Some(pattern) => {
            match std::fs::read_to_string(&entry.path) {
                Ok(contents) => pattern.is_match(&contents),
                Err(_) => false, // binary or unreadable file
            }
        }
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::load_rules_from_str;
    use chrono::{TimeZone, Utc};
    use std::path::PathBuf;

    /// Helper to build a test FileEntry without touching the filesystem.
    fn make_entry(name: &str, ext: Option<&str>, size: u64, path: &str, source: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            name: name.to_owned(),
            extension: ext.map(|s| s.to_owned()),
            size_bytes: size,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 3, 15, 10, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 3, 15, 10, 0, 0).unwrap(),
            tags: Vec::new(),
            colour: None,
            source_inbox: source.to_owned(),
        }
    }

    fn build_engine(yaml: &str) -> RuleEngine {
        let ruleset = load_rules_from_str(yaml).unwrap();
        RuleEngine::new(ruleset)
    }

    #[test]
    fn glob_filename_matching() {
        let engine = build_engine(
            r#"
rules:
  - name: "Invoices"
    match:
      filename: "*invoice*"
    destination: "Invoices"
"#,
        );

        let entry = make_entry(
            "my_invoice_2025.pdf",
            Some("pdf"),
            100,
            "/tmp/my_invoice_2025.pdf",
            "Downloads",
        );
        assert_eq!(engine.evaluate(&entry).unwrap().name, "Invoices");

        let entry2 = make_entry(
            "readme.txt",
            Some("txt"),
            50,
            "/tmp/readme.txt",
            "Downloads",
        );
        assert!(engine.evaluate(&entry2).is_none());
    }

    #[test]
    fn regex_filename_matching() {
        let engine = build_engine(
            r#"
rules:
  - name: "Screenshots"
    match:
      filename: "regex:^Screenshot \\d{4}-\\d{2}-\\d{2}"
    destination: "Screenshots"
"#,
        );

        let entry = make_entry(
            "Screenshot 2025-03-15 at 10.00.00.png",
            Some("png"),
            5000,
            "/tmp/Screenshot 2025-03-15 at 10.00.00.png",
            "Desktop",
        );
        assert_eq!(engine.evaluate(&entry).unwrap().name, "Screenshots");

        let entry2 = make_entry("photo.png", Some("png"), 5000, "/tmp/photo.png", "Desktop");
        assert!(engine.evaluate(&entry2).is_none());
    }

    #[test]
    fn extension_matching_case_insensitive() {
        let engine = build_engine(
            r#"
rules:
  - name: "PDFs"
    match:
      extension: "PDF"
    destination: "docs"
"#,
        );

        // FileEntry stores extension lowercase, but the rule says "PDF"
        let entry = make_entry("file.pdf", Some("pdf"), 100, "/tmp/file.pdf", "Downloads");
        assert_eq!(engine.evaluate(&entry).unwrap().name, "PDFs");

        let entry2 = make_entry("file.txt", Some("txt"), 100, "/tmp/file.txt", "Downloads");
        assert!(engine.evaluate(&entry2).is_none());
    }

    #[test]
    fn path_matching() {
        let engine = build_engine(
            r#"
rules:
  - name: "Downloads"
    match:
      path: "*/Downloads/*"
    destination: "from-downloads"
"#,
        );

        let entry = make_entry(
            "file.txt",
            Some("txt"),
            100,
            "/Users/me/Downloads/file.txt",
            "Downloads",
        );
        assert_eq!(engine.evaluate(&entry).unwrap().name, "Downloads");

        let entry2 = make_entry(
            "file.txt",
            Some("txt"),
            100,
            "/Users/me/Desktop/file.txt",
            "Desktop",
        );
        assert!(engine.evaluate(&entry2).is_none());
    }

    #[test]
    fn and_logic_multiple_fields() {
        let engine = build_engine(
            r#"
rules:
  - name: "PDF invoices"
    match:
      extension: "pdf"
      filename: "*invoice*"
    destination: "invoices"
"#,
        );

        // Both match
        let entry = make_entry(
            "invoice_01.pdf",
            Some("pdf"),
            100,
            "/tmp/invoice_01.pdf",
            "Downloads",
        );
        assert_eq!(engine.evaluate(&entry).unwrap().name, "PDF invoices");

        // Extension matches but filename doesn't
        let entry2 = make_entry(
            "report.pdf",
            Some("pdf"),
            100,
            "/tmp/report.pdf",
            "Downloads",
        );
        assert!(engine.evaluate(&entry2).is_none());

        // Filename matches but extension doesn't
        let entry3 = make_entry(
            "invoice.txt",
            Some("txt"),
            100,
            "/tmp/invoice.txt",
            "Downloads",
        );
        assert!(engine.evaluate(&entry3).is_none());
    }

    #[test]
    fn min_max_size_filters() {
        let engine = build_engine(
            r#"
rules:
  - name: "Medium files"
    match:
      min_size: 100
      max_size: 1000
    destination: "medium"
"#,
        );

        let too_small = make_entry("a.txt", Some("txt"), 50, "/tmp/a.txt", "D");
        assert!(engine.evaluate(&too_small).is_none());

        let just_right = make_entry("b.txt", Some("txt"), 500, "/tmp/b.txt", "D");
        assert_eq!(engine.evaluate(&just_right).unwrap().name, "Medium files");

        let too_big = make_entry("c.txt", Some("txt"), 2000, "/tmp/c.txt", "D");
        assert!(engine.evaluate(&too_big).is_none());
    }

    #[test]
    fn no_match_returns_none() {
        let engine = build_engine(
            r#"
rules:
  - name: "PDFs only"
    match:
      extension: "pdf"
    destination: "pdfs"
"#,
        );

        let entry = make_entry("readme.md", Some("md"), 100, "/tmp/readme.md", "Downloads");
        assert!(engine.evaluate(&entry).is_none());
    }

    #[test]
    fn first_match_wins() {
        let engine = build_engine(
            r#"
rules:
  - name: "First"
    match:
      extension: "pdf"
    destination: "first"
  - name: "Second"
    match:
      extension: "pdf"
    destination: "second"
"#,
        );

        let entry = make_entry("doc.pdf", Some("pdf"), 100, "/tmp/doc.pdf", "Downloads");
        assert_eq!(engine.evaluate(&entry).unwrap().name, "First");
    }

    #[test]
    fn template_variable_expansion() {
        let entry = FileEntry {
            path: PathBuf::from("/tmp/file.pdf"),
            name: "file.pdf".to_owned(),
            extension: Some("pdf".to_owned()),
            size_bytes: 100,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 3, 15, 10, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 3, 15, 10, 0, 0).unwrap(),
            tags: Vec::new(),
            colour: None,
            source_inbox: "Downloads".to_owned(),
        };

        let result = RuleEngine::expand_destination("{year}/{month}/{date}/{ext}/{source}", &entry);
        assert_eq!(result, "2025/03/2025-03-15/pdf/Downloads");
    }

    #[test]
    fn template_expansion_missing_extension() {
        let entry = FileEntry {
            path: PathBuf::from("/tmp/Makefile"),
            name: "Makefile".to_owned(),
            extension: None,
            size_bytes: 50,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            tags: Vec::new(),
            colour: None,
            source_inbox: "Desktop".to_owned(),
        };

        let result = RuleEngine::expand_destination("{year}/{ext}", &entry);
        assert_eq!(result, "2025/");
    }

    #[test]
    fn content_matching() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("notes.txt");
        std::fs::write(&file_path, "TODO: finish the report\nDone: other stuff").unwrap();

        let engine = build_engine(
            r#"
rules:
  - name: "Has TODOs"
    match:
      content: "regex:TODO"
    destination: "todos"
"#,
        );

        let entry = FileEntry {
            path: file_path,
            name: "notes.txt".to_owned(),
            extension: Some("txt".to_owned()),
            size_bytes: 42,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            tags: Vec::new(),
            colour: None,
            source_inbox: "Downloads".to_owned(),
        };

        assert_eq!(engine.evaluate(&entry).unwrap().name, "Has TODOs");
    }
}
