//! `librarian rollback` — reverse an applied plan.

pub async fn run(_plan: Option<String>) -> anyhow::Result<()> {
    tracing::info!("librarian rollback: not yet implemented");
    Ok(())
}
