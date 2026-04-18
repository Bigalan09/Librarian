//! blake3 file hashing.

use std::path::Path;

use tokio::io::AsyncReadExt;

/// Hash a file synchronously using blake3, returning the hex digest.
pub fn hash_file_sync(path: &Path) -> std::io::Result<String> {
    let data = std::fs::read(path)?;
    Ok(blake3::hash(&data).to_hex().to_string())
}

/// Hash a file asynchronously using blake3, returning the hex digest.
pub async fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024]; // 64 KiB buffer

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_known_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known.txt");
        std::fs::write(&path, b"hello world").unwrap();

        let digest = hash_file(&path).await.unwrap();

        // blake3 of "hello world"
        let expected = blake3::hash(b"hello world").to_hex().to_string();
        assert_eq!(digest, expected);
    }

    #[tokio::test]
    async fn hash_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.txt");
        std::fs::write(&path, b"").unwrap();

        let digest = hash_file(&path).await.unwrap();
        let expected = blake3::hash(b"").to_hex().to_string();
        assert_eq!(digest, expected);
    }

    #[tokio::test]
    async fn hash_large_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.bin");
        // 256 KiB of zeros — exercises multi-buffer reads
        let data = vec![0u8; 256 * 1024];
        std::fs::write(&path, &data).unwrap();

        let digest = hash_file(&path).await.unwrap();
        let expected = blake3::hash(&data).to_hex().to_string();
        assert_eq!(digest, expected);
    }

    #[tokio::test]
    async fn hash_nonexistent_file_errors() {
        let result = hash_file(Path::new("/nonexistent/file.txt")).await;
        assert!(result.is_err());
    }

    #[test]
    fn sync_hash_known_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.txt");
        std::fs::write(&path, b"hello world").unwrap();

        let digest = hash_file_sync(&path).unwrap();
        let expected = blake3::hash(b"hello world").to_hex().to_string();
        assert_eq!(digest, expected);
    }

    #[test]
    fn sync_hash_nonexistent_file_errors() {
        let result = hash_file_sync(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn sync_and_async_produce_same_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("both.bin");
        let data = vec![42u8; 128 * 1024];
        std::fs::write(&path, &data).unwrap();

        let sync_digest = hash_file_sync(&path).unwrap();
        let async_digest = hash_file(&path).await.unwrap();
        assert_eq!(sync_digest, async_digest);
    }
}
