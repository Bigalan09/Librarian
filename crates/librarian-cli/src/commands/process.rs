//! `librarian process` — scan, classify, produce plan.

pub async fn run(
    _source: Vec<std::path::PathBuf>,
    _destination: Option<std::path::PathBuf>,
    _provider: Option<String>,
    _llm_model: Option<String>,
    _embed_model: Option<String>,
    _rules: Option<std::path::PathBuf>,
    _threshold: Option<f64>,
    _dry_run: bool,
    _plan_name: Option<String>,
    _rename: bool,
) -> anyhow::Result<()> {
    tracing::info!("librarian process: not yet implemented");
    Ok(())
}
