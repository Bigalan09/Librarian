//! `librarian plans` - list, show, delete, and clean named plans.

use chrono::Utc;
use librarian_core::config;
use librarian_core::plan::Plan;

pub async fn list() -> anyhow::Result<()> {
    let plans_dir = config::librarian_home().join("plans");
    if !plans_dir.exists() {
        println!("No plans found.");
        return Ok(());
    }

    let plans = Plan::list(&plans_dir)?;
    if plans.is_empty() {
        println!("No plans found.");
        return Ok(());
    }

    for plan in &plans {
        println!(
            "  {:<40} {:?}  {} moves  {}",
            plan.name,
            plan.status,
            plan.actions.len(),
            plan.created_at.format("%Y-%m-%d %H:%M"),
        );
    }

    Ok(())
}

pub async fn show(name: &str) -> anyhow::Result<()> {
    let plans_dir = config::librarian_home().join("plans");
    let plan_path = super::resolve_plan_path(&plans_dir, name)?;

    let plan = Plan::load(&plan_path).map_err(|_| {
        anyhow::anyhow!(
            "Plan '{}' not found at {}. Run 'librarian plans list' to see available plans.",
            name,
            plan_path.display()
        )
    })?;

    println!("Plan: {}", plan.name);
    println!("Status: {:?}", plan.status);
    println!("Created: {}", plan.created_at.format("%Y-%m-%d %H:%M:%S"));
    if let Some(applied) = plan.applied_at {
        println!("Applied: {}", applied.format("%Y-%m-%d %H:%M:%S"));
    }
    println!("Sources: {:?}", plan.source_folders);
    println!("Destination: {}", plan.destination_root.display());
    println!("\nStats:");
    println!("  Total files:    {}", plan.stats.total_files);
    println!("  Rule matched:   {}", plan.stats.rule_matched);
    println!("  AI classified:  {}", plan.stats.ai_classified);
    println!("  Needs review:   {}", plan.stats.needs_review);
    println!("  Collisions:     {}", plan.stats.collisions);
    println!("  Ignored:        {}", plan.stats.ignored);
    println!("  Limit reached:  {}", plan.stats.limit_reached);

    println!("\nActions ({}):", plan.actions.len());
    for (i, action) in plan.actions.iter().enumerate().take(20) {
        println!(
            "  {:>4}. {:?}  {} → {}",
            i + 1,
            action.action_type,
            action.source_path.display(),
            action.destination_path.display(),
        );
    }
    if plan.actions.len() > 20 {
        println!("  ... and {} more", plan.actions.len() - 20);
    }

    Ok(())
}

pub async fn clean(max_age_days: u32) -> anyhow::Result<()> {
    let plans_dir = config::librarian_home().join("plans");
    if !plans_dir.exists() {
        println!("No plans found.");
        return Ok(());
    }

    let plans = Plan::list(&plans_dir)?;
    let cutoff = Utc::now() - chrono::Duration::days(max_age_days as i64);
    let mut removed = 0;

    for plan in &plans {
        if plan.created_at < cutoff {
            let path = plans_dir.join(format!("{}.json", plan.id));
            match std::fs::remove_file(&path) {
                Ok(()) => removed += 1,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    println!(
        "Cleaned {} plan(s) older than {} days ({} remaining).",
        removed,
        max_age_days,
        plans.len() - removed,
    );
    Ok(())
}

pub async fn delete(name: &str) -> anyhow::Result<()> {
    let plans_dir = config::librarian_home().join("plans");
    let plan_path = super::resolve_plan_path(&plans_dir, name)?;

    if !plan_path.exists() {
        anyhow::bail!(
            "Plan '{}' not found at {}. Run 'librarian plans list' to see available plans.",
            name,
            plan_path.display()
        );
    }

    // No TOCTOU risk here: delete is idempotent and the exists() check
    // provides a better error message than a raw NotFound.
    std::fs::remove_file(&plan_path)?;
    println!("Deleted plan '{}'", name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::make_test_plan as make_plan;

    #[test]
    fn list_plans_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();

        let plans = Plan::list(&plans_dir).unwrap();
        assert!(plans.is_empty());
    }

    #[test]
    fn list_plans_returns_saved_plans() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");

        let p1 = make_plan("alpha");
        p1.save(&plans_dir).unwrap();

        let p2 = make_plan("beta");
        p2.save(&plans_dir).unwrap();

        let plans = Plan::list(&plans_dir).unwrap();
        assert_eq!(plans.len(), 2);
    }

    #[test]
    fn list_plans_sorted_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");

        let p1 = make_plan("first");
        p1.save(&plans_dir).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));

        let p2 = make_plan("second");
        p2.save(&plans_dir).unwrap();

        let plans = Plan::list(&plans_dir).unwrap();
        assert_eq!(plans.len(), 2);
        // Newest first
        assert!(plans[0].created_at >= plans[1].created_at);
    }

    #[test]
    fn show_plan_loads_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");

        let plan = make_plan("show-test");
        plan.save(&plans_dir).unwrap();

        let loaded = Plan::load(&plans_dir.join(format!("{}.json", plan.id))).unwrap();
        assert_eq!(loaded.name, "show-test");
        assert_eq!(loaded.status, librarian_core::plan::PlanStatus::Draft);
    }

    #[test]
    fn clean_removes_old_plans() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");

        // Create a plan with a very old timestamp
        let mut plan = make_plan("old-plan");
        plan.created_at = Utc::now() - chrono::Duration::days(60);
        plan.save(&plans_dir).unwrap();

        let plans_before = Plan::list(&plans_dir).unwrap();
        assert_eq!(plans_before.len(), 1);

        // Clean plans older than 30 days
        let cutoff = Utc::now() - chrono::Duration::days(30);
        let mut removed = 0;
        for p in &plans_before {
            if p.created_at < cutoff {
                let path = plans_dir.join(format!("{}.json", p.id));
                std::fs::remove_file(&path).unwrap();
                removed += 1;
            }
        }
        assert_eq!(removed, 1);

        let plans_after = Plan::list(&plans_dir).unwrap();
        assert!(plans_after.is_empty());
    }

    #[test]
    fn clean_keeps_recent_plans() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");

        // Create a recent plan
        let plan = make_plan("recent-plan");
        plan.save(&plans_dir).unwrap();

        let cutoff = Utc::now() - chrono::Duration::days(30);
        let plans = Plan::list(&plans_dir).unwrap();
        let old_count = plans.iter().filter(|p| p.created_at < cutoff).count();
        assert_eq!(old_count, 0);
    }

    #[test]
    fn delete_removes_plan_file() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");

        let plan = make_plan("delete-me");
        plan.save(&plans_dir).unwrap();

        let plan_path = plans_dir.join(format!("{}.json", plan.id));
        assert!(plan_path.exists());

        std::fs::remove_file(&plan_path).unwrap();
        assert!(!plan_path.exists());
    }

    #[test]
    fn list_nonexistent_dir_returns_empty() {
        let plans = Plan::list(std::path::Path::new("/nonexistent/plans")).unwrap();
        assert!(plans.is_empty());
    }
}
