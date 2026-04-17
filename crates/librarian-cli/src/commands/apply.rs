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
    let decision_log = config::librarian_home().join("history").join("decisions.jsonl");

    let plan_path = if let Some(name) = &plan_name {
        plans_dir.join(format!("{name}.json"))
    } else {
        // Find most recent plan
        most_recent_plan(&plans_dir)?
    };

    if !plan_path.exists() {
        anyhow::bail!("Plan not found: {}", plan_path.display());
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

    entries
        .first()
        .map(|e| e.path())
        .ok_or_else(|| anyhow::anyhow!("No plans found in {}", plans_dir.display()))
}
