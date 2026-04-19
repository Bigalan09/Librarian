//! `librarian rollback` — reverse an applied plan.

use librarian_core::config;
use librarian_core::plan::{Plan, PlanStatus};

pub async fn run(plan_name: Option<String>) -> anyhow::Result<()> {
    let plans_dir = config::librarian_home().join("plans");
    let decision_log = config::librarian_home()
        .join("history")
        .join("decisions.jsonl");

    let plan_path = if let Some(name) = &plan_name {
        if name == super::LATEST_ALIAS {
            // "latest" for rollback means most recent *applied* plan
            most_recent_applied(&plans_dir)?
        } else {
            super::resolve_plan_path(&plans_dir, name)?
        }
    } else {
        most_recent_applied(&plans_dir)?
    };

    let mut plan = Plan::load(&plan_path).map_err(|_| {
        anyhow::anyhow!(
            "Plan not found at {}. Run 'librarian plans list' to see available plans.",
            plan_path.display()
        )
    })?;

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

    plans.sort_by_key(|p| std::cmp::Reverse(p.applied_at));

    plans
        .first()
        .map(|p| plans_dir.join(format!("{}.json", p.id)))
        .ok_or_else(|| anyhow::anyhow!(
            "No applied plans found in {}. Only plans with status 'Applied' can be rolled back.",
            plans_dir.display()
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_plan(name: &str) -> Plan {
        Plan::new(
            name,
            vec![PathBuf::from("/tmp/inbox")],
            PathBuf::from("/tmp/dest"),
        )
    }

    #[test]
    fn most_recent_applied_nonexistent_dir() {
        let result = most_recent_applied(std::path::Path::new("/nonexistent/plans"));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No plans directory")
        );
    }

    #[test]
    fn most_recent_applied_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let result = most_recent_applied(dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No applied plans"));
    }

    #[test]
    fn most_recent_applied_skips_draft_plans() {
        let dir = tempfile::tempdir().unwrap();
        // Draft plan should be skipped
        let plan = make_plan("draft-plan");
        plan.save(dir.path()).unwrap();

        let result = most_recent_applied(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn most_recent_applied_finds_applied_plan() {
        let dir = tempfile::tempdir().unwrap();

        let mut plan = make_plan("applied-plan");
        plan.status = PlanStatus::Applied;
        plan.save(dir.path()).unwrap();

        let result = most_recent_applied(dir.path()).unwrap();
        assert!(result.to_string_lossy().contains(&plan.id));
    }
}
