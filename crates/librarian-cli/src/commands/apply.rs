//! `librarian apply` — execute a plan.

use librarian_core::config;
use librarian_core::plan::Plan;

pub async fn run(
    plan_name: Option<String>,
    backup: bool,
    aggressive: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let plans_dir = config::librarian_home().join("plans");
    let decision_log = config::librarian_home()
        .join("history")
        .join("decisions.jsonl");

    let plan_path = super::resolve_plan_path(
        &plans_dir,
        plan_name.as_deref().unwrap_or(super::LATEST_ALIAS),
    )?;

    if !plan_path.exists() {
        anyhow::bail!(
            "Plan not found at {}. Run 'librarian plans list' to see available plans.",
            plan_path.display()
        );
    }

    let mut plan = Plan::load(&plan_path)?;

    if dry_run {
        println!("Dry run — showing what would happen:");
        for action in &plan.actions {
            if action.action_type == librarian_core::plan::ActionType::Move {
                println!(
                    "  MOVE {} → {}",
                    action.source_path.display(),
                    action.destination_path.display()
                );
            }
        }
        println!("\n{} action(s) would be executed.", plan.actions.len());
        return Ok(());
    }

    // Backup if requested
    if backup {
        let backup_dir = config::librarian_home().join("backup");
        std::fs::create_dir_all(&backup_dir)?;
        plan.backup(&backup_dir)?;
        println!("Backup created at {}", backup_dir.join(&plan.id).display());
    }

    // Aggressive gate
    if aggressive && plan.backup_path.is_none() {
        anyhow::bail!(
            "--aggressive requires --backup to have succeeded for this plan. \
             Run 'librarian apply --plan {} --backup' first.",
            plan.name
        );
    }

    let report = plan.apply(&decision_log, aggressive)?;

    // Save updated plan (status is now Applied)
    plan.save(&plans_dir)?;

    println!("\nApply complete:");
    println!("  Moved:      {}", report.moved);
    println!("  Tagged:     {}", report.tagged);
    println!("  Skipped:    {}", report.skipped);
    println!("  Collisions: {}", report.collisions);
    if !report.errors.is_empty() {
        println!("  Errors:     {}", report.errors.len());
        for err in &report.errors {
            eprintln!("    {err}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::commands::{make_test_plan as make_plan, resolve_plan_path};

    #[test]
    fn most_recent_plan_nonexistent_dir() {
        let result = resolve_plan_path(std::path::Path::new("/nonexistent/plans"), "latest");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No plans directory")
        );
    }

    #[test]
    fn most_recent_plan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_plan_path(dir.path(), "latest");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No plans found"));
    }

    #[test]
    fn most_recent_plan_ignores_non_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a plan").unwrap();
        let result = resolve_plan_path(dir.path(), "latest");
        assert!(result.is_err());
    }

    #[test]
    fn most_recent_plan_returns_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let plan = make_plan("test-plan");
        plan.save(dir.path()).unwrap();

        let result = resolve_plan_path(dir.path(), "latest").unwrap();
        assert!(result.extension().unwrap() == "json");
    }

    #[test]
    fn most_recent_plan_picks_newest() {
        let dir = tempfile::tempdir().unwrap();

        let p1 = make_plan("plan-a");
        p1.save(dir.path()).unwrap();

        // Small sleep to ensure different mtime
        std::thread::sleep(std::time::Duration::from_millis(50));

        let p2 = make_plan("plan-b");
        p2.save(dir.path()).unwrap();

        let result = resolve_plan_path(dir.path(), "latest").unwrap();
        let filename = result.file_name().unwrap().to_string_lossy();
        // Should be the most recently saved (p2)
        assert!(filename.contains(&p2.id));
    }

    #[test]
    fn resolve_explicit_name() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_plan_path(dir.path(), "my-plan").unwrap();
        assert_eq!(result, dir.path().join("my-plan.json"));
    }
}
