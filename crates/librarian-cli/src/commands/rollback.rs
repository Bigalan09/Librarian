//! `librarian rollback` — reverse an applied plan.

use librarian_core::config;
use librarian_core::plan::{Plan, PlanStatus};

pub async fn run(plan_name: Option<String>) -> anyhow::Result<()> {
    let plans_dir = config::librarian_home().join("plans");
    let decision_log = config::librarian_home()
        .join("history")
        .join("decisions.jsonl");

    let plan_path = if let Some(name) = &plan_name {
        plans_dir.join(format!("{name}.json"))
    } else {
        // Find most recent applied plan
        most_recent_applied(&plans_dir)?
    };

    if !plan_path.exists() {
        anyhow::bail!(
            "Plan not found at {}. Run 'librarian plans list' to see available plans.",
            plan_path.display()
        );
    }

    let mut plan = Plan::load(&plan_path)?;

    if plan.status != PlanStatus::Applied {
        anyhow::bail!(
            "Plan '{}' is {:?}, not Applied. Only applied plans can be rolled back.",
            plan.name,
            plan.status
        );
    }

    plan.rollback(&decision_log)?;

    // Save updated plan
    plan.save(&plans_dir)?;

    println!("Rolled back plan '{}'", plan.name);
    Ok(())
}

fn most_recent_applied(plans_dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    if !plans_dir.exists() {
        anyhow::bail!(
            "No plans directory found at {}. Run 'librarian process' to create a plan first.",
            plans_dir.display()
        );
    }

    let mut plans: Vec<Plan> = std::fs::read_dir(plans_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .filter_map(|e| Plan::load(&e.path()).ok())
        .filter(|p| p.status == PlanStatus::Applied)
        .collect();

    plans.sort_by(|a, b| b.applied_at.cmp(&a.applied_at));

    plans
        .first()
        .map(|p| plans_dir.join(format!("{}.json", p.id)))
        .ok_or_else(|| anyhow::anyhow!(
            "No applied plans found in {}. Only plans with status 'Applied' can be rolled back.",
            plans_dir.display()
        ))
}
