//! `librarian status` — list plans, recent runs, pending reviews.

use librarian_core::config;
use librarian_core::plan::Plan;

pub async fn run() -> anyhow::Result<()> {
    let home = config::librarian_home();
    let plans_dir = home.join("plans");

    if !plans_dir.exists() {
        println!("No plans directory found. Run 'librarian init' first.");
        return Ok(());
    }

    let plans = Plan::list(&plans_dir)?;

    if plans.is_empty() {
        println!("No plans found.");
    } else {
        println!("Recent plans:");
        for plan in plans.iter().take(10) {
            println!(
                "  {:<40} {:?}  {} files  {}",
                plan.name,
                plan.status,
                plan.stats.total_files,
                plan.created_at.format("%Y-%m-%d %H:%M"),
            );
        }
    }

    // Check NeedsReview folder
    let cfg = config::load_default()?;
    if cfg.needs_review_path.exists() {
        let count = std::fs::read_dir(&cfg.needs_review_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .count();
        if count > 0 {
            println!("\nPending review: {} file(s) in NeedsReview", count);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use librarian_core::plan::{Plan, PlanStats};

    fn make_plan(name: &str) -> Plan {
        Plan::new(
            name,
            vec![PathBuf::from("/tmp/inbox")],
            PathBuf::from("/tmp/dest"),
        )
    }

    #[test]
    fn list_plans_returns_multiple() {
        let dir = tempfile::tempdir().unwrap();
        let plans_dir = dir.path().join("plans");

        for i in 0..5 {
            let p = make_plan(&format!("plan-{i}"));
            p.save(&plans_dir).unwrap();
        }

        let plans = Plan::list(&plans_dir).unwrap();
        assert_eq!(plans.len(), 5);
    }

    #[test]
    fn plan_stats_default_zero() {
        let stats = PlanStats::default();
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.rule_matched, 0);
        assert_eq!(stats.ai_classified, 0);
        assert_eq!(stats.needs_review, 0);
        assert_eq!(stats.collisions, 0);
        assert_eq!(stats.ignored, 0);
    }

    #[test]
    fn needs_review_folder_counts_only_files() {
        let dir = tempfile::tempdir().unwrap();
        let nr = dir.path().join("NeedsReview");
        std::fs::create_dir_all(&nr).unwrap();

        // Create files and a subdirectory
        std::fs::write(nr.join("a.txt"), "a").unwrap();
        std::fs::write(nr.join("b.pdf"), "b").unwrap();
        std::fs::create_dir_all(nr.join("subdir")).unwrap();

        let count = std::fs::read_dir(&nr)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .count();
        assert_eq!(count, 2);
    }

    #[test]
    fn empty_needs_review_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        let nr = dir.path().join("NeedsReview");
        std::fs::create_dir_all(&nr).unwrap();

        let count = std::fs::read_dir(&nr)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .count();
        assert_eq!(count, 0);
    }
}
