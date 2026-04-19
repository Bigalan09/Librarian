//! LLM classifier with few-shot

use librarian_core::FileEntry;
use librarian_providers::router::ErasedProvider;
use librarian_providers::traits::{ChatMessage, Provider};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Result from the LLM classifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResult {
    pub destination: String,
    pub confidence: f64,
    pub tags: Vec<String>,
    pub reason: String,
}

/// LLM-based file classifier.
///
/// Uses the provider's chat completion endpoint to classify files based on
/// their metadata, existing bucket names, and optional few-shot examples.
pub struct LlmClassifier;

impl LlmClassifier {
    /// Classify a file entry using the LLM.
    ///
    /// Builds a structured prompt that includes the file's metadata, existing
    /// bucket/folder names for context, and optional few-shot examples. The LLM
    /// is expected to return JSON with `destination`, `confidence`, `tags`, and
    /// `reason` fields.
    pub async fn classify<P: Provider>(
        provider: &P,
        file_entry: &FileEntry,
        existing_buckets: &[String],
        few_shot_examples: &[String],
    ) -> anyhow::Result<LlmResult> {
        let system_prompt = build_system_prompt(existing_buckets, few_shot_examples);
        let user_prompt = build_user_prompt(file_entry);

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt,
            },
        ];

        let response = provider.chat(messages, 0.1, 512).await?;
        parse_llm_response(&response.content)
    }

    /// Classify using a dyn-compatible ErasedProvider.
    pub async fn classify_dyn(
        provider: &dyn ErasedProvider,
        file_entry: &FileEntry,
        existing_buckets: &[String],
        few_shot_examples: &[String],
    ) -> anyhow::Result<LlmResult> {
        let system_prompt = build_system_prompt(existing_buckets, few_shot_examples);
        let user_prompt = build_user_prompt(file_entry);

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt,
            },
        ];

        let response = provider.chat(messages, 0.1, 512).await?;
        parse_llm_response(&response.content)
    }
}

/// Build the system prompt for classification.
fn build_system_prompt(existing_buckets: &[String], few_shot_examples: &[String]) -> String {
    let mut prompt = String::from(
        "You are a file classification assistant. Your task is to determine the best \
         destination folder for a given file based on its name, extension, size, and \
         other metadata.\n\n\
         You MUST respond with valid JSON only, using this exact format:\n\
         {\n  \"destination\": \"FolderName\",\n  \"confidence\": 0.85,\n  \
         \"tags\": [\"tag1\", \"tag2\"],\n  \"reason\": \"Brief explanation\"\n}\n\n\
         Rules:\n\
         - \"destination\" must be a folder name (no path separators)\n\
         - \"confidence\" must be a float between 0.0 and 1.0\n\
         - \"tags\" must be an array of descriptive string tags\n\
         - \"reason\" must explain your classification decision\n",
    );

    if !existing_buckets.is_empty() {
        prompt.push_str("\nExisting destination folders:\n");
        for bucket in existing_buckets {
            prompt.push_str(&format!("- {bucket}\n"));
        }
        prompt.push_str(
            "\nPrefer placing files into existing folders when appropriate. \
             Only suggest a new folder name if no existing folder is a good fit.\n",
        );
    }

    if !few_shot_examples.is_empty() {
        prompt.push_str("\nExamples of previous classifications:\n");
        for example in few_shot_examples {
            prompt.push_str(&format!("- {example}\n"));
        }
    }

    prompt
}

/// Build the user prompt describing the file to classify.
fn build_user_prompt(entry: &FileEntry) -> String {
    let ext = entry.extension.as_deref().unwrap_or("(no extension)");
    let tags_str = if entry.tags.is_empty() {
        "(none)".to_string()
    } else {
        entry.tags.join(", ")
    };

    format!(
        "Classify this file:\n\
         - Name: {}\n\
         - Extension: {ext}\n\
         - Size: {} bytes\n\
         - Existing tags: {tags_str}\n\
         - Source inbox: {}",
        entry.name, entry.size_bytes, entry.source_inbox,
    )
}

/// Parse the LLM's JSON response into an `LlmResult`.
fn parse_llm_response(raw: &str) -> anyhow::Result<LlmResult> {
    // Try to find JSON in the response (LLMs sometimes wrap it in markdown
    // code fences).
    let json_str = extract_json(raw);

    match serde_json::from_str::<LlmResult>(json_str) {
        Ok(result) => Ok(result),
        Err(e) => {
            warn!("Failed to parse LLM response as JSON: {e}\nRaw: {raw}");
            Err(anyhow::anyhow!(
                "Failed to parse LLM classification response: {e}"
            ))
        }
    }
}

/// Extract JSON from a potentially markdown-wrapped response.
fn extract_json(raw: &str) -> &str {
    // Try to find ```json ... ``` blocks
    if let Some(start) = raw.find("```json") {
        let content = &raw[start + 7..];
        if let Some(end) = content.find("```") {
            return content[..end].trim();
        }
    }
    // Try to find ``` ... ``` blocks
    if let Some(start) = raw.find("```") {
        let content = &raw[start + 3..];
        if let Some(end) = content.find("```") {
            return content[..end].trim();
        }
    }
    // Try to find { ... } directly
    if let Some(start) = raw.find('{')
        && let Some(end) = raw.rfind('}')
    {
        return &raw[start..=end];
    }
    raw.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_includes_buckets() {
        let buckets = vec![
            "Documents".to_string(),
            "Photos".to_string(),
            "Invoices".to_string(),
        ];
        let prompt = build_system_prompt(&buckets, &[]);

        assert!(prompt.contains("Documents"));
        assert!(prompt.contains("Photos"));
        assert!(prompt.contains("Invoices"));
        assert!(prompt.contains("Existing destination folders"));
    }

    #[test]
    fn system_prompt_includes_few_shot_examples() {
        let examples = vec![
            "invoice_2024.pdf -> Invoices (confidence: 0.95)".to_string(),
            "photo_001.jpg -> Photos (confidence: 0.90)".to_string(),
        ];
        let prompt = build_system_prompt(&[], &examples);

        assert!(prompt.contains("invoice_2024.pdf"));
        assert!(prompt.contains("photo_001.jpg"));
        assert!(prompt.contains("Examples of previous classifications"));
    }

    #[test]
    fn system_prompt_includes_both() {
        let buckets = vec!["Documents".to_string()];
        let examples = vec!["readme.md -> Documents".to_string()];
        let prompt = build_system_prompt(&buckets, &examples);

        assert!(prompt.contains("Existing destination folders"));
        assert!(prompt.contains("Examples of previous classifications"));
    }

    #[test]
    fn system_prompt_empty_context() {
        let prompt = build_system_prompt(&[], &[]);
        assert!(prompt.contains("file classification assistant"));
        assert!(!prompt.contains("Existing destination folders"));
        assert!(!prompt.contains("Examples of previous classifications"));
    }

    #[test]
    fn parse_clean_json() {
        let json = r#"{"destination": "Invoices", "confidence": 0.92, "tags": ["finance", "pdf"], "reason": "Filename contains invoice"}"#;
        let result = parse_llm_response(json).unwrap();
        assert_eq!(result.destination, "Invoices");
        assert!((result.confidence - 0.92).abs() < 1e-6);
        assert_eq!(result.tags, vec!["finance", "pdf"]);
        assert!(result.reason.contains("invoice"));
    }

    #[test]
    fn parse_markdown_wrapped_json() {
        let raw = "Here is the classification:\n```json\n{\"destination\": \"Photos\", \"confidence\": 0.88, \"tags\": [\"image\"], \"reason\": \"Image file\"}\n```";
        let result = parse_llm_response(raw).unwrap();
        assert_eq!(result.destination, "Photos");
    }

    #[test]
    fn parse_invalid_json_returns_error() {
        let raw = "I'm not sure about this file.";
        assert!(parse_llm_response(raw).is_err());
    }

    #[test]
    fn extract_json_from_code_fence() {
        let raw = "```json\n{\"a\": 1}\n```";
        assert_eq!(extract_json(raw), "{\"a\": 1}");
    }

    #[test]
    fn extract_json_from_bare_braces() {
        let raw = "result: {\"a\": 1} done";
        assert_eq!(extract_json(raw), "{\"a\": 1}");
    }

    #[test]
    fn user_prompt_contains_file_info() {
        use chrono::{TimeZone, Utc};
        use std::path::PathBuf;

        let entry = FileEntry {
            path: PathBuf::from("/tmp/test.pdf"),
            name: "test.pdf".to_string(),
            extension: Some("pdf".to_string()),
            size_bytes: 1024,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            tags: vec!["important".to_string()],
            colour: None,
            source_inbox: "Downloads".to_string(),
        };

        let prompt = build_user_prompt(&entry);
        assert!(prompt.contains("test.pdf"));
        assert!(prompt.contains("pdf"));
        assert!(prompt.contains("1024"));
        assert!(prompt.contains("Downloads"));
        assert!(prompt.contains("important"));
    }

    #[test]
    fn user_prompt_no_extension() {
        use chrono::{TimeZone, Utc};
        use std::path::PathBuf;

        let entry = FileEntry {
            path: PathBuf::from("/tmp/Makefile"),
            name: "Makefile".to_string(),
            extension: None,
            size_bytes: 256,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            tags: Vec::new(),
            colour: None,
            source_inbox: "Desktop".to_string(),
        };

        let prompt = build_user_prompt(&entry);
        assert!(prompt.contains("(no extension)"));
        assert!(prompt.contains("(none)"));
    }

    #[test]
    fn extract_json_plain_text_fallback() {
        let raw = "no braces here";
        assert_eq!(extract_json(raw), "no braces here");
    }

    #[test]
    fn extract_json_generic_code_fence() {
        let raw = "```\n{\"a\": 2}\n```";
        assert_eq!(extract_json(raw), "{\"a\": 2}");
    }

    #[test]
    fn parse_llm_response_missing_fields() {
        let raw = r#"{"destination": "Docs"}"#;
        // Missing confidence, tags, reason - serde should fail
        assert!(parse_llm_response(raw).is_err());
    }

    // --- Mock provider for async classify tests ---

    use librarian_providers::traits::{ChatResponse, ModelInfo};

    struct MockChatProvider {
        response: String,
    }

    impl Provider for MockChatProvider {
        async fn validate(&self) -> anyhow::Result<ModelInfo> {
            Ok(ModelInfo {
                id: "mock".to_string(),
            })
        }
        async fn chat(
            &self,
            _messages: Vec<ChatMessage>,
            _temperature: f64,
            _max_tokens: u32,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                content: self.response.clone(),
                model: "mock".to_string(),
            })
        }
        async fn embed(&self, _texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(Vec::new())
        }
        fn name(&self) -> &str {
            "mock-chat"
        }
    }

    struct FailingChatProvider;

    impl Provider for FailingChatProvider {
        async fn validate(&self) -> anyhow::Result<ModelInfo> {
            unimplemented!()
        }
        async fn chat(
            &self,
            _messages: Vec<ChatMessage>,
            _temperature: f64,
            _max_tokens: u32,
        ) -> anyhow::Result<ChatResponse> {
            Err(anyhow::anyhow!("chat service down"))
        }
        async fn embed(&self, _texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(Vec::new())
        }
        fn name(&self) -> &str {
            "failing-chat"
        }
    }

    fn make_test_entry() -> FileEntry {
        use chrono::{TimeZone, Utc};
        use std::path::PathBuf;

        FileEntry {
            path: PathBuf::from("/tmp/report.pdf"),
            name: "report.pdf".to_string(),
            extension: Some("pdf".to_string()),
            size_bytes: 2048,
            hash: String::new(),
            created_at: Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap(),
            modified_at: Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap(),
            tags: Vec::new(),
            colour: None,
            source_inbox: "Downloads".to_string(),
        }
    }

    #[tokio::test]
    async fn classify_with_mock_provider() {
        let provider = MockChatProvider {
            response: r#"{"destination": "Work/Reports", "confidence": 0.88, "tags": ["work"], "reason": "Looks like a work report"}"#.to_string(),
        };
        let entry = make_test_entry();
        let result = LlmClassifier::classify(
            &provider,
            &entry,
            &["Work".to_string(), "Personal".to_string()],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.destination, "Work/Reports");
        assert!((result.confidence - 0.88).abs() < 1e-6);
        assert_eq!(result.tags, vec!["work"]);
    }

    #[tokio::test]
    async fn classify_dyn_with_mock_provider() {
        use librarian_providers::router::ErasedProvider;

        let provider = MockChatProvider {
            response: r#"{"destination": "Finance", "confidence": 0.92, "tags": ["invoice"], "reason": "Invoice document"}"#.to_string(),
        };
        let erased: &dyn ErasedProvider = &provider;
        let entry = make_test_entry();
        let result = LlmClassifier::classify_dyn(erased, &entry, &[], &[])
            .await
            .unwrap();

        assert_eq!(result.destination, "Finance");
        assert!((result.confidence - 0.92).abs() < 1e-6);
    }

    #[tokio::test]
    async fn classify_with_few_shot_examples() {
        let provider = MockChatProvider {
            response: r#"{"destination": "Personal", "confidence": 0.85, "tags": [], "reason": "Based on corrections"}"#.to_string(),
        };
        let entry = make_test_entry();
        let examples = vec!["report.pdf was moved to Personal".to_string()];
        let result = LlmClassifier::classify(&provider, &entry, &["Work".to_string()], &examples)
            .await
            .unwrap();

        assert_eq!(result.destination, "Personal");
    }

    #[tokio::test]
    async fn classify_propagates_chat_error() {
        let provider = FailingChatProvider;
        let entry = make_test_entry();
        let result = LlmClassifier::classify(&provider, &entry, &[], &[]).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("chat service down")
        );
    }

    #[tokio::test]
    async fn classify_dyn_propagates_chat_error() {
        use librarian_providers::router::ErasedProvider;

        let provider = FailingChatProvider;
        let erased: &dyn ErasedProvider = &provider;
        let entry = make_test_entry();
        let result = LlmClassifier::classify_dyn(erased, &entry, &[], &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn classify_returns_error_on_invalid_json_response() {
        let provider = MockChatProvider {
            response: "I cannot classify this file.".to_string(),
        };
        let entry = make_test_entry();
        let result = LlmClassifier::classify(&provider, &entry, &[], &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn classify_handles_markdown_wrapped_response() {
        let provider = MockChatProvider {
            response: "Here is the result:\n```json\n{\"destination\": \"Docs\", \"confidence\": 0.75, \"tags\": [], \"reason\": \"Document file\"}\n```".to_string(),
        };
        let entry = make_test_entry();
        let result = LlmClassifier::classify(&provider, &entry, &[], &[])
            .await
            .unwrap();
        assert_eq!(result.destination, "Docs");
    }
}
