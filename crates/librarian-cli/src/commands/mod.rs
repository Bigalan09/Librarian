pub mod apply;
pub mod config;
pub mod correct;
pub mod init;
pub mod plans;
pub mod process;
pub mod review;
pub mod rollback;
pub mod rules;
pub mod status;
pub mod suggest;
pub mod watch;

/// Resolve a plan name: if "latest", find the most recent plan file;
/// otherwise return `{name}.json` in the plans directory.
pub(crate) fn resolve_plan_path(
    plans_dir: &std::path::Path,
    name: &str,
) -> anyhow::Result<std::path::PathBuf> {
    if name == "latest" {
        most_recent_plan(plans_dir)
    } else {
        Ok(plans_dir.join(format!("{name}.json")))
    }
}

fn most_recent_plan(plans_dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    if !plans_dir.exists() {
        anyhow::bail!("No plans directory found. Run 'librarian process' first.");
    }

    let mut entries: Vec<_> = std::fs::read_dir(plans_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .collect();

    entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));

    entries.first().map(|e| e.path()).ok_or_else(|| {
        anyhow::anyhow!(
            "No plans found in {}. Run 'librarian process' to generate a plan first.",
            plans_dir.display()
        )
    })
}
