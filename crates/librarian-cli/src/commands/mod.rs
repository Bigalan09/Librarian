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

pub(crate) const LATEST_ALIAS: &str = "latest";

#[cfg(test)]
pub(crate) fn make_test_plan(name: &str) -> librarian_core::plan::Plan {
    librarian_core::plan::Plan::new(
        name,
        vec![std::path::PathBuf::from("/tmp/inbox")],
        std::path::PathBuf::from("/tmp/dest"),
    )
}

/// Resolve a plan name: if "latest", find the most recent plan file;
/// otherwise return `{name}.json` in the plans directory.
pub(crate) fn resolve_plan_path(
    plans_dir: &std::path::Path,
    name: &str,
) -> anyhow::Result<std::path::PathBuf> {
    if name == LATEST_ALIAS {
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
        .map(|e| {
            let mtime = e.metadata().ok().and_then(|m| m.modified().ok());
            (e.path(), mtime)
        })
        .collect();

    entries.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));

    entries
        .first()
        .map(|(path, _)| path.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No plans found in {}. Run 'librarian process' to generate a plan first.",
                plans_dir.display()
            )
        })
}
