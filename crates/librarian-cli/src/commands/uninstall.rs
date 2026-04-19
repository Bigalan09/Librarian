use std::io::{self, Write};
use std::path::PathBuf;

/// All paths that Librarian creates on the user's system.
fn librarian_data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".librarian")
}

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

pub async fn run(yes: bool) -> anyhow::Result<()> {
    let data_dir = librarian_data_dir();
    let binary = current_binary()?;

    // Also check the cargo-installed copy
    let cargo_binary = dirs::home_dir().map(|h| h.join(".cargo/bin/librarian"));

    println!("This will remove Librarian from your system:\n");

    println!("  Binary:  {}", binary.display());
    if let Some(ref cb) = cargo_binary
        && cb.exists()
        && cb.canonicalize().ok().as_ref() != Some(&binary)
    {
        println!("  Binary:  {} (cargo install)", cb.display());
    }

    if data_dir.exists() {
        println!("  Config:  {}/config.yaml", data_dir.display());
        println!("  Rules:   {}/rules.yaml", data_dir.display());
        println!("  History: {}/history/", data_dir.display());
        println!("  Cache:   {}/cache/", data_dir.display());
        println!("  Plans:   {}/plans/", data_dir.display());
        println!("  Backups: {}/backup/", data_dir.display());
    } else {
        println!("  Config:  (none found)");
    }

    println!();

    if !yes && !confirm("Are you sure you want to uninstall Librarian?") {
        println!("Aborted.");
        return Ok(());
    }

    // 1. Remove data directory
    if data_dir.exists() {
        print!("Removing {}...", data_dir.display());
        remove_path(&data_dir)?;
        println!(" done");
    }

    // 2. Remove cargo-installed binary (if different from current)
    if let Some(ref cb) = cargo_binary
        && cb.exists()
        && cb.canonicalize().ok().as_ref() != Some(&binary)
    {
        print!("Removing {}...", cb.display());
        remove_path(cb)?;
        println!(" done");
    }

    // 3. Remove the current binary last (this is the running process)
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
        let dir = librarian_data_dir();
        assert!(dir.ends_with(".librarian"));
    }

    #[test]
    fn current_binary_exists() {
        let path = current_binary().unwrap();
        assert!(path.exists());
    }
}
