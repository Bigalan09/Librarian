use std::process::Command;

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

/// Fetch the latest release tag from GitHub.
async fn fetch_latest_release() -> anyhow::Result<GitHubRelease> {
    let url = format!(
        "https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
    );

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

    let release: GitHubRelease = resp.error_for_status()?.json().await?;
    Ok(release)
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
        println!(
            "\nRun `librarian update` to install the update."
        );
        return Ok(());
    }

    println!("\nInstalling {latest} via cargo...");

    let status = Command::new("cargo")
        .args([
            "install",
            "--git",
            &format!("https://github.com/{GITHUB_REPO}.git"),
            "--tag",
            &release.tag_name,
            "librarian-cli",
            "--force",
        ])
        .status()?;

    if status.success() {
        println!("\nUpdated to {latest} successfully.");
    } else {
        anyhow::bail!(
            "cargo install failed (exit code: {}). \
             You can install manually:\n  \
             cargo install --git https://github.com/{GITHUB_REPO}.git --tag {} librarian-cli",
            status.code().unwrap_or(-1),
            release.tag_name,
        );
    }

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
}
