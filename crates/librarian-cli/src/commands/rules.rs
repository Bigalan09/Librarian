//! `librarian rules` — validate or suggest rules.

pub async fn validate(_rules: Option<std::path::PathBuf>) -> anyhow::Result<()> {
    tracing::info!("librarian rules validate: not yet implemented");
    Ok(())
}

pub async fn suggest() -> anyhow::Result<()> {
    tracing::info!("librarian rules suggest: not yet implemented");
    Ok(())
}
