//! `librarian config` — show or edit configuration.

use librarian_core::config;

pub async fn show() -> anyhow::Result<()> {
    let cfg = config::load_default()?;
    let json = serde_json::to_string_pretty(&cfg)?;
    println!("{json}");
    Ok(())
}

pub async fn edit() -> anyhow::Result<()> {
    let config_path = config::librarian_home().join("config.yaml");
    if !config_path.exists() {
        anyhow::bail!(
            "Configuration file not found at {}. Run 'librarian init' first.",
            config_path.display()
        );
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_owned());
    let status = std::process::Command::new(&editor)
        .arg(&config_path)
        .status()?;

    if !status.success() {
        anyhow::bail!(
            "Editor '{}' exited with a non-zero status — the config may not have been saved. \
             Set a different editor with the EDITOR environment variable.",
            editor
        );
    }

    Ok(())
}
