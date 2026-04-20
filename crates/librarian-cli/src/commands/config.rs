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
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    let status = std::process::Command::new(program)
        .args(parts)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_serialises_to_json() {
        let cfg = config::AppConfig::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        assert!(json.contains("destination_root"));
        assert!(json.contains("inbox_folders"));
        assert!(json.contains("thresholds"));
    }

    #[test]
    fn config_roundtrips_through_yaml() {
        let cfg = config::AppConfig::default();
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let parsed: config::AppConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            cfg.destination_root.file_name(),
            parsed.destination_root.file_name()
        );
    }
}
