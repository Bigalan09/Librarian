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
pub mod uninstall;
pub mod update;
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
        return most_recent_plan(plans_dir);
    }

    let name = name.strip_suffix(".json").unwrap_or(name);

    // Try direct ID match first.
    let direct = plans_dir.join(format!("{name}.json"));
    if direct.exists() {
        return Ok(direct);
    }

    // Fall back to matching by plan name field inside JSON files.
    if plans_dir.exists() {
        use librarian_core::plan::Plan;
        if let Ok(plans) = Plan::list(plans_dir)
            && let Some(plan) = plans.iter().find(|p| p.name == name)
        {
            let path = plans_dir.join(format!("{}.json", plan.id));
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // Return the direct path anyway (will error downstream with a clear message).
    Ok(direct)
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

#[cfg(test)]
mod resolve_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn resolve_named_plan() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();

        let path = resolve_plan_path(&plans_dir, "my-plan").unwrap();
        assert_eq!(path, plans_dir.join("my-plan.json"));
    }

    #[test]
    fn resolve_latest_with_one_plan() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();
        std::fs::write(plans_dir.join("first.json"), "{}").unwrap();

        let path = resolve_plan_path(&plans_dir, "latest").unwrap();
        assert_eq!(path, plans_dir.join("first.json"));
    }

    #[test]
    fn resolve_latest_picks_most_recent() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();

        // Create two plan files with different mtimes
        std::fs::write(plans_dir.join("old.json"), "{}").unwrap();
        // Sleep briefly to ensure different mtime
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(plans_dir.join("new.json"), "{}").unwrap();

        let path = resolve_plan_path(&plans_dir, "latest").unwrap();
        assert_eq!(path, plans_dir.join("new.json"));
    }

    #[test]
    fn resolve_latest_nonexistent_dir_errors() {
        let result = resolve_plan_path(&PathBuf::from("/nonexistent/plans"), "latest");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No plans directory")
        );
    }

    #[test]
    fn resolve_latest_empty_dir_errors() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();

        let result = resolve_plan_path(&plans_dir, "latest");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No plans found"));
    }

    #[test]
    fn resolve_latest_ignores_non_json_files() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();

        // Only non-json files
        std::fs::write(plans_dir.join("readme.md"), "# Plans").unwrap();
        std::fs::write(plans_dir.join("backup.bak"), "data").unwrap();

        let result = resolve_plan_path(&plans_dir, "latest");
        assert!(result.is_err());
    }
}
