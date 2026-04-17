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
    let plan_path = plans_dir.join(format!("{name}.json"));

    if !plan_path.exists() {
        anyhow::bail!(
            "Plan '{}' not found at {}. Run 'librarian plans list' to see available plans.",
            name,
            plan_path.display()
        );
    }

    let plan = Plan::load(&plan_path)?;

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
            if path.exists() {
                std::fs::remove_file(&path)?;
                removed += 1;
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
    let plan_path = plans_dir.join(format!("{name}.json"));

    if !plan_path.exists() {
        anyhow::bail!(
            "Plan '{}' not found at {}. Run 'librarian plans list' to see available plans.",
            name,
            plan_path.display()
        );
    }

    std::fs::remove_file(&plan_path)?;
    println!("Deleted plan '{}'", name);
    Ok(())
}
