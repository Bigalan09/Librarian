//! `librarian correct` — record an explicit correction.

pub async fn run(
    _file: std::path::PathBuf,
    _to: Option<std::path::PathBuf>,
    _retag: Option<String>,
) -> anyhow::Result<()> {
    tracing::info!("librarian correct: not yet implemented");
    Ok(())
}
