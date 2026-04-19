use std::path::PathBuf;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_REPO: &str = "Bigalan09/Librarian";

#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

/// Strip a leading 'v' from a version tag (e.g. "v0.2.0" -> "0.2.0").
fn strip_v(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Build the Rust target triple for the current platform.
fn current_target() -> anyhow::Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        (os, arch) => anyhow::bail!("Unsupported platform: {os}/{arch}"),
    }
}

/// Determine where the currently running binary lives.
fn current_binary_path() -> anyhow::Result<PathBuf> {
    std::env::current_exe()?.canonicalize().map_err(Into::into)
}

/// Fetch the latest release from GitHub.
async fn fetch_latest_release() -> anyhow::Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", format!("librarian/{CURRENT_VERSION}"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!(
            "No releases found for {GITHUB_REPO}. \
             Publish a GitHub release to enable updates."
        );
    }

    resp.error_for_status()?.json().await.map_err(Into::into)
}

/// Download a release tarball and extract the binary.
async fn download_binary(tag: &str, target: &str) -> anyhow::Result<PathBuf> {
    let url = format!(
        "https://github.com/{GITHUB_REPO}/releases/download/{tag}/librarian-{target}.tar.gz"
    );

    println!("Downloading librarian-{target}.tar.gz...");

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", format!("librarian/{CURRENT_VERSION}"))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "Failed to download release artifact (HTTP {}). \
             No prebuilt binary for {target}?",
            resp.status()
        );
    }

    let bytes = resp.bytes().await?;

    // Extract to a temp directory
    let tmp_dir = tempfile::tempdir()?;
    let tarball_path = tmp_dir.path().join("librarian.tar.gz");
    std::fs::write(&tarball_path, &bytes)?;

    let status = std::process::Command::new("tar")
        .args(["xzf", &tarball_path.to_string_lossy(), "-C"])
        .arg(tmp_dir.path())
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to extract tarball");
    }

    let extracted = tmp_dir.path().join("librarian");
    if !extracted.exists() {
        anyhow::bail!("Expected binary not found in tarball");
    }

    // Copy to a stable temp location (tempdir would delete on drop)
    let staging = std::env::temp_dir().join("librarian-update");
    std::fs::copy(&extracted, &staging)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(staging)
}

/// Replace the current binary with the new one.
fn replace_binary(new_binary: &PathBuf, install_path: &PathBuf) -> anyhow::Result<()> {
    // Try direct copy first
    match std::fs::copy(new_binary, install_path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Fall back to sudo
            println!("Requires elevated permissions...");
            let status = std::process::Command::new("sudo")
                .args([
                    "cp",
                    &new_binary.to_string_lossy(),
                    &install_path.to_string_lossy(),
                ])
                .status()?;
            if !status.success() {
                anyhow::bail!("Failed to install binary (sudo cp failed)");
            }
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Check for updates and optionally install the latest version.
pub async fn run(check_only: bool) -> anyhow::Result<()> {
    println!("Current version: {CURRENT_VERSION}");
    println!("Checking for updates...");

    let release = fetch_latest_release().await?;
    let latest = strip_v(&release.tag_name);

    if latest == CURRENT_VERSION {
        println!("Already up to date.");
        return Ok(());
    }

    println!("New version available: {latest} (current: {CURRENT_VERSION})");
    println!("Release: {}", release.html_url);

    if check_only {
        println!("\nRun `librarian update` to install the update.");
        return Ok(());
    }

    let target = current_target()?;
    let install_path = current_binary_path()?;
    println!("Target: {target}");
    println!("Binary: {}", install_path.display());

    let new_binary = download_binary(&release.tag_name, target).await?;
    replace_binary(&new_binary, &install_path)?;

    // Clean up staging file
    let _ = std::fs::remove_file(&new_binary);

    println!("\nUpdated to {latest} successfully.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_v_prefix() {
        assert_eq!(strip_v("v0.2.0"), "0.2.0");
        assert_eq!(strip_v("0.2.0"), "0.2.0");
        assert_eq!(strip_v("v1.0.0-rc1"), "1.0.0-rc1");
    }

    #[test]
    fn current_version_is_set() {
        assert!(!CURRENT_VERSION.is_empty());
    }

    #[test]
    fn current_target_returns_valid_triple() {
        let target = current_target().unwrap();
        assert!(
            target.contains("apple-darwin") || target.contains("unknown-linux-gnu"),
            "unexpected target: {target}"
        );
    }

    #[test]
    fn current_binary_path_exists() {
        let path = current_binary_path().unwrap();
        assert!(path.exists());
    }
}
