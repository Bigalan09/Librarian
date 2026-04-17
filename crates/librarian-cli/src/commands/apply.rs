//! `librarian apply` — execute a plan.

pub async fn run(
    _plan: Option<String>,
    _backup: bool,
    _aggressive: bool,
    _dry_run: bool,
) -> anyhow::Result<()> {
    tracing::info!("librarian apply: not yet implemented");
    Ok(())
}
