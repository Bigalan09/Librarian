use std::io::{self, Write};
use std::path::PathBuf;

use librarian_core::config::librarian_home;

/// Prompt the user for yes/no confirmation.
fn confirm(prompt: &str) -> bool {
    print!("{prompt} [y/N] ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Find the currently running binary path.
fn current_binary() -> anyhow::Result<PathBuf> {
    std::env::current_exe()?.canonicalize().map_err(Into::into)
}

/// Remove a file or directory, using sudo if needed.
fn remove_path(path: &PathBuf) -> anyhow::Result<()> {
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };

    match result {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            println!("  Requires elevated permissions...");
            let status = std::process::Command::new("sudo")
                .args(["rm", "-rf", &path.to_string_lossy()])
                .status()?;
            if status.success() {
                Ok(())
            } else {
                anyhow::bail!("Failed to remove {} (sudo failed)", path.display())
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Silently remove a path if it exists. Returns true if something was removed.
fn try_remove(path: &PathBuf, label: &str) -> bool {
    if path.exists() {
        print!("Removing {label}...");
        match remove_path(path) {
            Ok(()) => {
                println!(" done");
                true
            }
            Err(e) => {
                println!(" failed: {e}");
                false
            }
        }
    } else {
        false
    }
}

/// Discover shell completion files that may have been installed.
fn find_completion_files() -> Vec<(PathBuf, &'static str)> {
    let mut found = Vec::new();

    let Some(home) = dirs::home_dir() else {
        return found;
    };

    // Zsh completions
    for p in [
        home.join(".zfunc/_librarian"),
        home.join(".zsh/completions/_librarian"),
        home.join(".oh-my-zsh/completions/_librarian"),
        PathBuf::from("/usr/local/share/zsh/site-functions/_librarian"),
        PathBuf::from("/opt/homebrew/share/zsh/site-functions/_librarian"),
    ] {
        if p.exists() {
            found.push((p, "zsh completion"));
        }
    }

    // Bash completions
    for p in [
        home.join(".bash_completion.d/librarian"),
        home.join(".local/share/bash-completion/completions/librarian"),
        PathBuf::from("/usr/local/etc/bash_completion.d/librarian"),
        PathBuf::from("/opt/homebrew/etc/bash_completion.d/librarian"),
        PathBuf::from("/etc/bash_completion.d/librarian"),
    ] {
        if p.exists() {
            found.push((p, "bash completion"));
        }
    }

    // Fish completions
    let fish_path = home.join(".config/fish/completions/librarian.fish");
    if fish_path.exists() {
        found.push((fish_path, "fish completion"));
    }

    found
}

/// Discover launchd agents or systemd units for librarian.
fn find_daemon_files() -> Vec<(PathBuf, &'static str)> {
    let mut found = Vec::new();

    if let Some(home) = dirs::home_dir() {
        // macOS launchd user agents
        let agents_dir = home.join("Library/LaunchAgents");
        if agents_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&agents_dir)
        {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().contains("librarian") {
                    found.push((entry.path(), "launchd agent"));
                }
            }
        }

        // Linux systemd user units
        let systemd_dir = home.join(".config/systemd/user");
        if systemd_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&systemd_dir)
        {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().contains("librarian") {
                    found.push((entry.path(), "systemd unit"));
                }
            }
        }
    }

    found
}

pub async fn run(yes: bool) -> anyhow::Result<()> {
    let data_dir = librarian_home();
    let binary = current_binary()?;
    let cargo_binary = dirs::home_dir().map(|h| h.join(".cargo/bin/librarian"));
    let completions = find_completion_files();
    let daemons = find_daemon_files();
    let update_staging = std::env::temp_dir().join("librarian-update");

    println!("This will remove Librarian from your system:\n");

    // Binaries
    println!("  Binary:     {}", binary.display());
    if let Some(ref cb) = cargo_binary
        && cb.exists()
        && cb.canonicalize().ok().as_ref() != Some(&binary)
    {
        println!("  Binary:     {} (cargo install)", cb.display());
    }

    // Data directory
    if data_dir.exists() {
        println!("  Data:       {}/", data_dir.display());
    } else {
        println!("  Data:       (none found)");
    }

    // Completions
    for (path, kind) in &completions {
        println!("  Completion: {} ({kind})", path.display());
    }

    // Daemons
    for (path, kind) in &daemons {
        println!("  Daemon:     {} ({kind})", path.display());
    }

    // Staging
    if update_staging.exists() {
        println!("  Temp:       {}", update_staging.display());
    }

    println!();

    if !yes && !confirm("Are you sure you want to uninstall Librarian?") {
        println!("Aborted.");
        return Ok(());
    }

    // 1. Stop and unload daemons before removing files
    for (path, kind) in &daemons {
        if *kind == "launchd agent" {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", &path.to_string_lossy()])
                .status();
        } else if *kind == "systemd unit"
            && let Some(name) = path.file_name()
        {
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "stop", &name.to_string_lossy()])
                .status();
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "disable", &name.to_string_lossy()])
                .status();
        }
        try_remove(&path.to_path_buf(), kind);
    }

    // 2. Remove shell completions
    for (path, kind) in &completions {
        try_remove(&path.to_path_buf(), kind);
    }

    // 3. Remove data directory
    if data_dir.exists() {
        print!("Removing {}...", data_dir.display());
        remove_path(&data_dir)?;
        println!(" done");
    }

    // 4. Remove update staging file
    try_remove(&update_staging, "update staging file");

    // 5. Remove cargo-installed binary (if different from current)
    if let Some(ref cb) = cargo_binary
        && cb.exists()
        && cb.canonicalize().ok().as_ref() != Some(&binary)
    {
        print!("Removing {}...", cb.display());
        remove_path(cb)?;
        println!(" done");
    }

    // 6. Remove the current binary last (this is the running process)
    print!("Removing {}...", binary.display());
    remove_path(&binary)?;
    println!(" done");

    println!("\nLibrarian has been uninstalled.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_is_under_home() {
        let dir = librarian_home();
        assert!(dir.ends_with(".librarian"));
    }

    #[test]
    fn current_binary_exists() {
        let path = current_binary().unwrap();
        assert!(path.exists());
    }

    #[test]
    fn find_completions_returns_vec() {
        // Should not panic even if no completions exist
        let result = find_completion_files();
        assert!(result.iter().all(|(p, _)| p.is_absolute()));
    }

    #[test]
    fn find_daemons_returns_vec() {
        // Should not panic even if no daemons exist
        let _ = find_daemon_files();
    }
}
