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
