//! Lightweight line-based SSE (Server-Sent Events) parser.
//!
//! Parses individual lines from an SSE stream and classifies them as
//! data payloads, done signals, comments, or empty lines.

/// Parsed SSE event from a single line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// A `data: <json>` line containing the JSON payload.
    Data(String),
    /// The `data: [DONE]` end-of-stream signal.
    Done,
    /// A comment line (starts with `:`).
    Comment,
    /// An empty or whitespace-only line.
    Empty,
}

/// Parse a single SSE line into an [`SseEvent`].
///
/// Handles:
/// - `data: [DONE]` -> `SseEvent::Done`
/// - `data: <json>` -> `SseEvent::Data(json)`
/// - `: <comment>` -> `SseEvent::Comment`
/// - empty / whitespace -> `SseEvent::Empty`
pub fn parse_sse_line(line: &str) -> SseEvent {
    let trimmed = line.trim_end_matches(['\r', '\n']);

    if trimmed.is_empty() {
        return SseEvent::Empty;
    }

    if trimmed.starts_with(':') {
        return SseEvent::Comment;
    }

    if let Some(data) = trimmed.strip_prefix("data:") {
        let payload = data.trim_start();
        if payload == "[DONE]" {
            return SseEvent::Done;
        }
        return SseEvent::Data(payload.to_string());
    }

    // Lines that don't match any known prefix are treated as empty/ignored.
    SseEvent::Empty
}

/// Extract the delta content from a streaming chat completion SSE data payload.
///
/// Looks for `choices[0].delta.content` in the JSON.
/// Returns `None` if the field is absent or the JSON is malformed.
pub fn extract_delta_content(json_data: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json_data).ok()?;
    value
        .get("choices")?
        .get(0)?
        .get("delta")?
        .get("content")?
        .as_str()
        .map(|s| s.to_string())
}

/// Accumulate all delta content from a series of SSE lines.
///
/// Processes lines, extracting and concatenating `choices[0].delta.content` values
/// until `[DONE]` is encountered or the iterator is exhausted.
pub fn accumulate_sse_content<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    let mut result = String::new();
    for line in lines {
        match parse_sse_line(line) {
            SseEvent::Data(json) => {
                if let Some(content) = extract_delta_content(&json) {
                    result.push_str(&content);
                }
            }
            SseEvent::Done => break,
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_data_line() {
        let line = r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#;
        match parse_sse_line(line) {
            SseEvent::Data(payload) => {
                assert!(payload.contains("Hello"));
            }
            other => panic!("Expected Data, got {:?}", other),
        }
    }

    #[test]
    fn parse_done_signal() {
        assert_eq!(parse_sse_line("data: [DONE]"), SseEvent::Done);
        assert_eq!(parse_sse_line("data:[DONE]"), SseEvent::Done);
    }

    #[test]
    fn parse_comment_line() {
        assert_eq!(parse_sse_line(": this is a comment"), SseEvent::Comment);
        assert_eq!(parse_sse_line(":keepalive"), SseEvent::Comment);
    }

    #[test]
    fn parse_empty_line() {
        assert_eq!(parse_sse_line(""), SseEvent::Empty);
        assert_eq!(parse_sse_line("\r\n"), SseEvent::Empty);
    }

    #[test]
    fn event_boundary_detection() {
        let lines = [
            r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#,
            "",
            r#"data: {"choices":[{"delta":{"content":" world"}}]}"#,
            "",
            "data: [DONE]",
        ];
        let content = accumulate_sse_content(lines.iter().copied());
        assert_eq!(content, "Hello world");
    }

    #[test]
    fn extract_delta_content_works() {
        let json = r#"{"choices":[{"delta":{"content":"test"}}]}"#;
        assert_eq!(extract_delta_content(json), Some("test".to_string()));
    }

    #[test]
    fn extract_delta_content_missing_field() {
        let json = r#"{"choices":[{"delta":{}}]}"#;
        assert_eq!(extract_delta_content(json), None);
    }

    #[test]
    fn malformed_data_returns_none() {
        assert_eq!(extract_delta_content("not json at all"), None);
        assert_eq!(extract_delta_content("{]"), None);
    }

    #[test]
    fn unknown_line_treated_as_empty() {
        assert_eq!(parse_sse_line("event: message"), SseEvent::Empty);
    }

    #[test]
    fn accumulate_stops_at_done_ignores_trailing() {
        let lines = [
            r#"data: {"choices":[{"delta":{"content":"A"}}]}"#,
            "data: [DONE]",
            r#"data: {"choices":[{"delta":{"content":"B"}}]}"#, // after DONE
        ];
        let content = accumulate_sse_content(lines.iter().copied());
        assert_eq!(content, "A");
    }

    #[test]
    fn extract_delta_content_null_value() {
        let json = r#"{"choices":[{"delta":{"content":null}}]}"#;
        assert_eq!(extract_delta_content(json), None);
    }

    #[test]
    fn data_line_without_space_after_colon() {
        // "data:payload" with no space
        let line = r#"data:{"choices":[{"delta":{"content":"ok"}}]}"#;
        match parse_sse_line(line) {
            SseEvent::Data(payload) => {
                assert_eq!(extract_delta_content(&payload), Some("ok".to_string()));
            }
            other => panic!("Expected Data, got {:?}", other),
        }
    }

    #[test]
    fn done_with_crlf_trailing() {
        assert_eq!(parse_sse_line("data: [DONE]\r\n"), SseEvent::Done);
    }
}
