//! Content extraction for text files

use std::path::Path;

use tracing::warn;

/// Extract text content from a file, if it is a supported text-based format.
///
/// Returns `None` for binary files (images, videos, audio, etc.) or if
/// extraction fails (e.g. encrypted PDFs, image-only PDFs).
pub async fn extract_content(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some("txt" | "md" | "csv") => match tokio::fs::read_to_string(path).await {
            Ok(content) => Some(content),
            Err(e) => {
                warn!("Failed to read text file {}: {e}", path.display());
                None
            }
        },
        Some("pdf") => {
            let path_buf = path.to_path_buf();
            let path_display = path_buf.display().to_string();
            match tokio::task::spawn_blocking(move || pdf_extract::extract_text(&path_buf)).await {
                Ok(Ok(text)) => Some(text),
                Ok(Err(e)) => {
                    warn!("PDF extraction failed for {path_display}: {e}");
                    None
                }
                Err(e) => {
                    warn!("spawn_blocking failed: {e}");
                    None
                }
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extract_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        tokio::fs::write(&file, "Hello, world!").await.unwrap();

        let content = extract_content(&file).await;
        assert_eq!(content.unwrap(), "Hello, world!");
    }

    #[tokio::test]
    async fn extract_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("readme.md");
        tokio::fs::write(&file, "# Title\n\nSome content")
            .await
            .unwrap();

        let content = extract_content(&file).await;
        assert_eq!(content.unwrap(), "# Title\n\nSome content");
    }

    #[tokio::test]
    async fn extract_csv() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data.csv");
        tokio::fs::write(&file, "a,b,c\n1,2,3").await.unwrap();

        let content = extract_content(&file).await;
        assert_eq!(content.unwrap(), "a,b,c\n1,2,3");
    }

    #[tokio::test]
    async fn binary_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("image.png");
        tokio::fs::write(&file, &[0x89, 0x50, 0x4E, 0x47])
            .await
            .unwrap();

        let content = extract_content(&file).await;
        assert!(content.is_none());
    }

    #[tokio::test]
    async fn video_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clip.mp4");
        tokio::fs::write(&file, &[0x00; 16]).await.unwrap();

        assert!(extract_content(&file).await.is_none());
    }

    #[tokio::test]
    async fn no_extension_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Makefile");
        tokio::fs::write(&file, "all: build").await.unwrap();

        assert!(extract_content(&file).await.is_none());
    }
}
