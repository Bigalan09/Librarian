//! `librarian plans` — list, show, delete named plans.

pub async fn list() -> anyhow::Result<()> {
    tracing::info!("librarian plans: not yet implemented");
    Ok(())
}

pub async fn show(_name: &str) -> anyhow::Result<()> {
    tracing::info!("librarian plans show: not yet implemented");
    Ok(())
}

pub async fn delete(_name: &str) -> anyhow::Result<()> {
    tracing::info!("librarian plans delete: not yet implemented");
    Ok(())
}
