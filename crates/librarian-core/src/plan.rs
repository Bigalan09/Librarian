//! Plan data model, apply, rollback.
//!
//! A `Plan` is a named, serialised set of proposed file-organisation actions.
//! It supports a Draft -> Applied -> RolledBack lifecycle with full decision
//! logging and optional backup/restore.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::decision::{
    ClassificationMethod, Decision, DecisionOutcome, DecisionType, append_decision,
};
use crate::file_entry::FinderColour;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum directory depth from the destination root.
const MAX_DEPTH: usize = 3;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Plan status lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,
    Applied,
    RolledBack,
    Deleted,
}

/// Types of actions in a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    Move,
    Rename,
    Tag,
    Skip,
    NeedsReview,
    Collision,
    Ignored,
}

// ---------------------------------------------------------------------------
// PlannedAction
// ---------------------------------------------------------------------------

/// A single proposed action within a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedAction {
    pub file_hash: String,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub action_type: ActionType,
    pub classification_method: ClassificationMethod,
    pub confidence: Option<f64>,
    pub tags: Vec<String>,
    pub colour: Option<FinderColour>,
    pub rename_to: Option<String>,
    pub original_name: Option<String>,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// PlanStats
// ---------------------------------------------------------------------------

/// Summary statistics for a plan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanStats {
    pub total_files: usize,
    pub rule_matched: usize,
    pub ai_classified: usize,
    pub needs_review: usize,
    pub collisions: usize,
    pub ignored: usize,
    pub skipped: usize,
    pub limit_reached: bool,
}

impl PlanStats {
    /// Recompute stats from a slice of actions.
    pub fn from_actions(actions: &[PlannedAction]) -> Self {
        let mut stats = Self {
            total_files: actions.len(),
            ..Self::default()
        };
        for a in actions {
            match a.action_type {
                ActionType::Move | ActionType::Tag | ActionType::Rename => {
                    match a.classification_method {
                        ClassificationMethod::Rule => stats.rule_matched += 1,
                        ClassificationMethod::FilenameEmbedding
                        | ClassificationMethod::ContentEmbedding
                        | ClassificationMethod::Llm => stats.ai_classified += 1,
                        ClassificationMethod::None => {}
                    }
                }
                ActionType::NeedsReview => stats.needs_review += 1,
                ActionType::Collision => stats.collisions += 1,
                ActionType::Skip => stats.skipped += 1,
                ActionType::Ignored => stats.ignored += 1,
            }
        }
        stats
    }
}

// ---------------------------------------------------------------------------
// ApplyReport
// ---------------------------------------------------------------------------

/// Summary report returned after applying a plan.
#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    pub moved: usize,
    pub tagged: usize,
    pub skipped: usize,
    pub collisions: usize,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

/// A named, serialised set of proposed actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub source_folders: Vec<PathBuf>,
    pub destination_root: PathBuf,
    pub actions: Vec<PlannedAction>,
    pub status: PlanStatus,
    pub applied_at: Option<DateTime<Utc>>,
    pub backup_path: Option<PathBuf>,
    pub stats: PlanStats,
}

impl Plan {
    // ----- Creation --------------------------------------------------------

    /// Create a new Draft plan with no actions.
    pub fn new(name: &str, source_folders: Vec<PathBuf>, destination_root: PathBuf) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        let now = Utc::now();
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!("{}-{:04}", now.format("%Y%m%d-%H%M%S-%3f"), seq);
        Self {
            id,
            name: name.to_owned(),
            created_at: now,
            source_folders,
            destination_root,
            actions: Vec::new(),
            status: PlanStatus::Draft,
            applied_at: None,
            backup_path: None,
            stats: PlanStats::default(),
        }
    }

    /// Generate a plan name like `downloads-2026-04-17-1423`.
    pub fn auto_name(source_name: &str) -> String {
        let now = Utc::now();
        format!(
            "{}-{}",
            source_name.to_lowercase().replace(' ', "-"),
            now.format("%Y-%m-%d-%H%M")
        )
    }

    // ----- Serialisation ---------------------------------------------------

    /// Write this plan to `{plans_dir}/{id}.json`.
    pub fn save(&self, plans_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(plans_dir)
            .with_context(|| format!("creating plans directory {}", plans_dir.display()))?;
        let path = plans_dir.join(format!("{}.json", self.id));
        let json = serde_json::to_string_pretty(self).context("serialising plan to JSON")?;
        std::fs::write(&path, json)
            .with_context(|| format!("writing plan to {}", path.display()))?;
        Ok(())
    }

    /// Load a plan from a JSON file.
    pub fn load(path: &Path) -> Result<Plan> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading plan from {}", path.display()))?;
        let plan: Plan = serde_json::from_str(&content)
            .with_context(|| format!("parsing plan {}", path.display()))?;
        Ok(plan)
    }

    /// List all plans in a directory.
    pub fn list(plans_dir: &Path) -> Result<Vec<Plan>> {
        if !plans_dir.exists() {
            return Ok(Vec::new());
        }
        let mut plans = Vec::new();
        for entry in std::fs::read_dir(plans_dir)
            .with_context(|| format!("reading plans directory {}", plans_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match Plan::load(&path) {
                    Ok(plan) => plans.push(plan),
                    Err(e) => {
                        warn!("skipping invalid plan file {}: {}", path.display(), e);
                    }
                }
            }
        }
        // Sort by creation time, newest first.
        plans.sort_by_key(|p| std::cmp::Reverse(p.created_at));
        Ok(plans)
    }

    // ----- Backup ----------------------------------------------------------

    /// Copy all source files to `{backup_dir}/{plan.id}/` preserving relative
    /// paths from their respective source folders.
    pub fn backup(&mut self, backup_dir: &Path) -> Result<()> {
        let plan_backup = backup_dir.join(&self.id);
        std::fs::create_dir_all(&plan_backup)
            .with_context(|| format!("creating backup directory {}", plan_backup.display()))?;

        for action in &self.actions {
            if matches!(
                action.action_type,
                ActionType::Skip | ActionType::Ignored | ActionType::Collision
            ) {
                continue;
            }
            if !action.source_path.exists() {
                continue;
            }
            // Determine a relative path: try to strip each source folder prefix.
            let rel = self
                .source_folders
                .iter()
                .find_map(|sf| action.source_path.strip_prefix(sf).ok())
                .unwrap_or_else(|| {
                    action
                        .source_path
                        .file_name()
                        .map(Path::new)
                        .unwrap_or(Path::new("unknown"))
                });

            let dest = plan_backup.join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&action.source_path, &dest).with_context(|| {
                format!(
                    "backing up {} -> {}",
                    action.source_path.display(),
                    dest.display()
                )
            })?;
        }

        self.backup_path = Some(plan_backup);
        Ok(())
    }

    // ----- Apply -----------------------------------------------------------

    /// Apply all planned actions. Logs each decision to `decision_log_path`.
    ///
    /// When `aggressive` is true the plan must have a backup — returns an error
    /// if `self.backup_path` is `None`.
    pub fn apply(&mut self, decision_log_path: &Path, aggressive: bool) -> Result<ApplyReport> {
        if self.status != PlanStatus::Draft {
            bail!(
                "plan {} is not in Draft status (current: {:?})",
                self.id,
                self.status
            );
        }
        if aggressive && self.backup_path.is_none() {
            bail!("aggressive mode requires a backup — call plan.backup() before apply");
        }

        let mut report = ApplyReport::default();

        for action in &self.actions {
            match action.action_type {
                ActionType::Skip | ActionType::Ignored | ActionType::Collision => {
                    report.skipped += 1;
                    let decision = Decision::new(
                        DecisionType::Skip,
                        &action.file_hash,
                        action.source_path.clone(),
                        &format!("skipped: {:?}", action.action_type),
                        DecisionOutcome::Skipped,
                    );
                    let _ = append_decision(decision_log_path, &decision);
                    continue;
                }
                _ => {}
            }

            match action.action_type {
                ActionType::Move | ActionType::NeedsReview => {
                    // Enforce 3-level depth.
                    if depth_from_root(&action.destination_path, &self.destination_root) > MAX_DEPTH
                    {
                        let msg = format!(
                            "destination {} exceeds {}-level depth from root {}",
                            action.destination_path.display(),
                            MAX_DEPTH,
                            self.destination_root.display()
                        );
                        warn!("{}", msg);
                        report.errors.push(msg.clone());
                        report.skipped += 1;
                        let decision = Decision::new(
                            DecisionType::Skip,
                            &action.file_hash,
                            action.source_path.clone(),
                            &msg,
                            DecisionOutcome::Failed,
                        );
                        let _ = append_decision(decision_log_path, &decision);
                        continue;
                    }

                    // Check for collision at destination.
                    if action.destination_path.exists() {
                        warn!(
                            "collision: {} already exists, skipping",
                            action.destination_path.display()
                        );
                        report.collisions += 1;
                        let decision = Decision::new(
                            DecisionType::Collision,
                            &action.file_hash,
                            action.source_path.clone(),
                            &format!(
                                "collision: destination {} already exists",
                                action.destination_path.display()
                            ),
                            DecisionOutcome::Skipped,
                        );
                        let _ = append_decision(decision_log_path, &decision);
                        continue;
                    }

                    // Create destination directories.
                    if let Some(parent) = action.destination_path.parent()
                        && let Err(e) = std::fs::create_dir_all(parent)
                    {
                        let msg = format!("failed to create directory {}: {}", parent.display(), e);
                        report.errors.push(msg.clone());
                        report.skipped += 1;
                        continue;
                    }

                    // Move the file.
                    if action.source_path.exists() {
                        if let Err(e) =
                            std::fs::rename(&action.source_path, &action.destination_path)
                        {
                            let msg = format!(
                                "failed to move {} -> {}: {}",
                                action.source_path.display(),
                                action.destination_path.display(),
                                e
                            );
                            report.errors.push(msg);
                            report.skipped += 1;
                            continue;
                        }
                    } else {
                        let msg = format!(
                            "source file {} does not exist",
                            action.source_path.display()
                        );
                        report.errors.push(msg);
                        report.skipped += 1;
                        continue;
                    }

                    // Apply tags.
                    if !action.tags.is_empty()
                        && let Err(e) =
                            crate::tags::write_tags(&action.destination_path, &action.tags)
                    {
                        warn!("failed to tag {}: {}", action.destination_path.display(), e);
                    }

                    // Apply colour.
                    if let Some(colour) = action.colour
                        && let Err(e) = crate::tags::write_colour(&action.destination_path, colour)
                    {
                        warn!(
                            "failed to set colour on {}: {}",
                            action.destination_path.display(),
                            e
                        );
                    }

                    // Handle rename (save original name).
                    if let Some(ref rename) = action.rename_to {
                        let original = action
                            .source_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let _ =
                            crate::tags::save_original_name(&action.destination_path, &original);
                        info!("renamed {} -> {}", original, rename);
                    }

                    report.moved += 1;
                    let decision = Decision::new(
                        DecisionType::Move,
                        &action.file_hash,
                        action.source_path.clone(),
                        &format!("moved to {}", action.destination_path.display()),
                        DecisionOutcome::Success,
                    );
                    let _ = append_decision(decision_log_path, &decision);
                }

                ActionType::Tag => {
                    // Tag only — no file move.
                    if action.source_path.exists()
                        && !action.tags.is_empty()
                        && let Err(e) = crate::tags::write_tags(&action.source_path, &action.tags)
                    {
                        let msg = format!("failed to tag {}: {}", action.source_path.display(), e);
                        report.errors.push(msg);
                        report.skipped += 1;
                        continue;
                    }
                    if let Some(colour) = action.colour {
                        let _ = crate::tags::write_colour(&action.source_path, colour);
                    }
                    report.tagged += 1;
                    let decision = Decision::new(
                        DecisionType::Tag,
                        &action.file_hash,
                        action.source_path.clone(),
                        &format!("tagged with {:?}", action.tags),
                        DecisionOutcome::Success,
                    );
                    let _ = append_decision(decision_log_path, &decision);
                }

                ActionType::Rename => {
                    if action.source_path.exists() {
                        let original_name = action
                            .source_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        if let Err(e) =
                            std::fs::rename(&action.source_path, &action.destination_path)
                        {
                            let msg = format!(
                                "failed to rename {} -> {}: {}",
                                action.source_path.display(),
                                action.destination_path.display(),
                                e
                            );
                            report.errors.push(msg);
                            report.skipped += 1;
                            continue;
                        }
                        let _ = crate::tags::save_original_name(
                            &action.destination_path,
                            &original_name,
                        );
                    }
                    report.moved += 1;
                    let decision = Decision::new(
                        DecisionType::Rename,
                        &action.file_hash,
                        action.source_path.clone(),
                        &format!("renamed to {}", action.destination_path.display()),
                        DecisionOutcome::Success,
                    );
                    let _ = append_decision(decision_log_path, &decision);
                }

                // Already handled above.
                ActionType::Skip | ActionType::Ignored | ActionType::Collision => {}
            }
        }

        self.status = PlanStatus::Applied;
        self.applied_at = Some(Utc::now());
        Ok(report)
    }

    // ----- Soft-delete (_Trash/) -------------------------------------------

    /// Move `file_path` into a managed `_Trash/` directory, preserving the
    /// file's path relative to `source_root` (or just the filename if the
    /// file is not under `source_root`).
    ///
    /// The trash layout is: `{trash_dir}/{plan_id}/{relative_path}`.
    ///
    /// Logs a `DecisionType::Move` with action `"soft_delete"` to
    /// `decision_log_path`.  Returns the new path inside the trash directory.
    pub fn soft_delete(
        &self,
        file_path: &Path,
        source_root: &Path,
        trash_dir: &Path,
        decision_log_path: &Path,
    ) -> Result<PathBuf> {
        let rel = file_path.strip_prefix(source_root).unwrap_or_else(|_| {
            file_path
                .file_name()
                .map(Path::new)
                .unwrap_or(Path::new("unknown"))
        });

        let trash_dest = trash_dir.join(&self.id).join(rel);

        if let Some(parent) = trash_dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating trash directory {}", parent.display()))?;
        }

        std::fs::rename(file_path, &trash_dest).with_context(|| {
            format!(
                "moving {} to trash {}",
                file_path.display(),
                trash_dest.display()
            )
        })?;

        let decision = Decision::new(
            DecisionType::Move,
            "",
            file_path.to_path_buf(),
            &format!("soft_delete: moved to {}", trash_dest.display()),
            DecisionOutcome::Success,
        );
        let _ = append_decision(decision_log_path, &decision);

        Ok(trash_dest)
    }

    // ----- Rollback --------------------------------------------------------

    /// Reverse all applied actions. If a backup exists, restore from backup
    /// instead of reversing individual moves.
    pub fn rollback(&mut self, decision_log_path: &Path) -> Result<()> {
        if self.status != PlanStatus::Applied {
            bail!(
                "plan {} is not in Applied status (current: {:?})",
                self.id,
                self.status
            );
        }

        if let Some(ref backup_path) = self.backup_path {
            // Restore from backup.
            for action in &self.actions {
                if matches!(
                    action.action_type,
                    ActionType::Skip | ActionType::Ignored | ActionType::Collision
                ) {
                    continue;
                }

                // Find backup file.
                let rel = self
                    .source_folders
                    .iter()
                    .find_map(|sf| action.source_path.strip_prefix(sf).ok())
                    .unwrap_or_else(|| {
                        action
                            .source_path
                            .file_name()
                            .map(Path::new)
                            .unwrap_or(Path::new("unknown"))
                    });

                let backup_file = backup_path.join(rel);
                if backup_file.exists() {
                    // Ensure source parent exists.
                    if let Some(parent) = action.source_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(&backup_file, &action.source_path).with_context(|| {
                        format!(
                            "restoring {} from backup {}",
                            action.source_path.display(),
                            backup_file.display()
                        )
                    })?;

                    // Remove the destination file if it was moved.
                    if matches!(
                        action.action_type,
                        ActionType::Move | ActionType::NeedsReview | ActionType::Rename
                    ) && action.destination_path.exists()
                    {
                        let _ = std::fs::remove_file(&action.destination_path);
                    }
                }

                // Remove tags that were applied.
                let tag_target = if action.destination_path.exists() {
                    &action.destination_path
                } else {
                    &action.source_path
                };
                if tag_target.exists() {
                    let _ = crate::tags::remove_tags(tag_target);
                }

                let decision = Decision::new(
                    DecisionType::Correction,
                    &action.file_hash,
                    action.source_path.clone(),
                    "rollback: restored from backup",
                    DecisionOutcome::Corrected,
                );
                let _ = append_decision(decision_log_path, &decision);
            }
        } else {
            // No backup — reverse moves.
            for action in self.actions.iter().rev() {
                if matches!(
                    action.action_type,
                    ActionType::Skip | ActionType::Ignored | ActionType::Collision
                ) {
                    continue;
                }

                match action.action_type {
                    ActionType::Move | ActionType::NeedsReview | ActionType::Rename
                        if action.destination_path.exists() =>
                    {
                        if let Some(parent) = action.source_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::rename(&action.destination_path, &action.source_path)
                            .with_context(|| {
                                format!(
                                    "rollback move {} -> {}",
                                    action.destination_path.display(),
                                    action.source_path.display()
                                )
                            })?;
                    }
                    ActionType::Tag if action.source_path.exists() => {
                        let _ = crate::tags::remove_tags(&action.source_path);
                    }
                    _ => {}
                }

                // Remove tags from wherever the file ended up.
                if action.source_path.exists() {
                    let _ = crate::tags::remove_tags(&action.source_path);
                }

                let decision = Decision::new(
                    DecisionType::Correction,
                    &action.file_hash,
                    action.source_path.clone(),
                    "rollback: reversed action",
                    DecisionOutcome::Corrected,
                );
                let _ = append_decision(decision_log_path, &decision);
            }
        }

        self.status = PlanStatus::RolledBack;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Depth enforcement helper
// ---------------------------------------------------------------------------

/// Count directory levels between `path` and `root`. Returns 0 if path is
/// at root, 1 if one directory deep, etc. Counts only the *directory*
/// components between root and the file's parent.
fn depth_from_root(path: &Path, root: &Path) -> usize {
    match path.parent().and_then(|p| p.strip_prefix(root).ok()) {
        Some(rel) => rel.components().count(),
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// Junk filename cleaning (T038)
// ---------------------------------------------------------------------------

/// Clean common junk filenames. Returns `Some(cleaned)` if the name was
/// modified, `None` if no cleaning was needed.
///
/// Patterns handled:
/// - `IMG_NNNN.ext` → `img_NNNN.ext` (lowercase prefix)
/// - `Screenshot YYYY-MM-DD at HH.MM.SS.ext` → `screenshot-YYYY-MM-DD-HHMMSS.ext`
/// - `scan_NNNN.ext` → `scan-NNNN.ext`
pub fn clean_junk_filename(name: &str) -> Option<String> {
    // IMG_NNNN.ext pattern.
    if let Some(rest) = name.strip_prefix("IMG_") {
        // Check this is IMG_ followed by digits then an extension.
        if let Some(dot_pos) = rest.find('.') {
            let digits = &rest[..dot_pos];
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                return Some(format!("img_{}", rest));
            }
        }
    }

    // Screenshot YYYY-MM-DD at HH.MM.SS.ext pattern.
    if name.starts_with("Screenshot ") || name.starts_with("screenshot ") {
        // Try to parse: "Screenshot 2026-04-17 at 14.23.05.png"
        let after_prefix = &name["Screenshot ".len()..];
        // Expected: "YYYY-MM-DD at HH.MM.SS.ext"
        if after_prefix.len() >= 22 {
            let date_part = &after_prefix[..10]; // YYYY-MM-DD
            if after_prefix[10..].starts_with(" at ") {
                let time_and_ext = &after_prefix[14..]; // HH.MM.SS.ext
                if time_and_ext.len() >= 8 {
                    let time_part = &time_and_ext[..8]; // HH.MM.SS
                    let ext_part = &time_and_ext[8..]; // .ext (or more)
                    let cleaned_time = time_part.replace('.', "");
                    return Some(format!(
                        "screenshot-{}-{}{}",
                        date_part, cleaned_time, ext_part
                    ));
                }
            }
        }
    }

    // scan_NNNN.ext pattern.
    if let Some(rest) = name.strip_prefix("scan_")
        && let Some(dot_pos) = rest.find('.')
    {
        let digits = &rest[..dot_pos];
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("scan-{}", rest));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Rename logic (T039)
// ---------------------------------------------------------------------------

/// Build a standardised filename: `YYYY-MM-DD_descriptive-slug.ext`.
///
/// The caller decides whether to apply this (e.g. only with --rename).
pub fn rename_file(name: &str, date: &NaiveDate, topic: &str, ext: &str) -> String {
    let _ = name; // original name available for future heuristics
    let slug: String = topic
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let ext_clean = ext.trim_start_matches('.');
    format!("{}_{}.{}", date.format("%Y-%m-%d"), slug, ext_clean)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // Helper to build a simple action for testing.
    fn make_action(source: PathBuf, dest: PathBuf, action_type: ActionType) -> PlannedAction {
        PlannedAction {
            file_hash: "testhash".to_owned(),
            source_path: source,
            destination_path: dest,
            action_type,
            classification_method: ClassificationMethod::Rule,
            confidence: Some(1.0),
            tags: vec!["test-tag".to_owned()],
            colour: None,
            rename_to: None,
            original_name: None,
            reason: None,
        }
    }

    // ----- Plan creation and auto-naming -----------------------------------

    #[test]
    fn plan_creation() {
        let plan = Plan::new(
            "test-plan",
            vec![PathBuf::from("/tmp/src")],
            PathBuf::from("/tmp/dest"),
        );
        assert_eq!(plan.name, "test-plan");
        assert_eq!(plan.status, PlanStatus::Draft);
        assert!(plan.actions.is_empty());
        assert!(plan.applied_at.is_none());
        assert!(plan.backup_path.is_none());
    }

    #[test]
    fn auto_name_format() {
        let name = Plan::auto_name("Downloads");
        // Should look like "downloads-YYYY-MM-DD-HHMM"
        assert!(name.starts_with("downloads-"));
        assert!(name.len() >= 21); // "downloads-" + "YYYY-MM-DD-HHMM"
    }

    // ----- JSON round-trip -------------------------------------------------

    #[test]
    fn json_serialisation_round_trip() {
        let mut plan = Plan::new(
            "round-trip",
            vec![PathBuf::from("/src")],
            PathBuf::from("/dest"),
        );
        plan.actions.push(make_action(
            PathBuf::from("/src/a.pdf"),
            PathBuf::from("/dest/docs/a.pdf"),
            ActionType::Move,
        ));
        plan.stats = PlanStats::from_actions(&plan.actions);

        let json = serde_json::to_string_pretty(&plan).unwrap();
        let restored: Plan = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.id, plan.id);
        assert_eq!(restored.name, "round-trip");
        assert_eq!(restored.actions.len(), 1);
        assert_eq!(restored.stats.total_files, 1);
        assert_eq!(restored.stats.rule_matched, 1);
    }

    // ----- Status transitions ----------------------------------------------

    #[test]
    fn status_transitions_draft_applied_rolledback() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source");
        let dest = dir.path().join("dest");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        // Create a source file.
        let file = src.join("test.txt");
        std::fs::write(&file, b"hello").unwrap();

        let mut plan = Plan::new("status-test", vec![src.clone()], dest.clone());
        plan.actions.push(make_action(
            file.clone(),
            dest.join("test.txt"),
            ActionType::Move,
        ));

        assert_eq!(plan.status, PlanStatus::Draft);

        plan.apply(&log, false).unwrap();
        assert_eq!(plan.status, PlanStatus::Applied);
        assert!(plan.applied_at.is_some());

        plan.rollback(&log).unwrap();
        assert_eq!(plan.status, PlanStatus::RolledBack);
    }

    // ----- PlanStats calculation -------------------------------------------

    #[test]
    fn plan_stats_calculation() {
        let actions = vec![
            make_action(PathBuf::from("/a"), PathBuf::from("/b"), ActionType::Move),
            make_action(
                PathBuf::from("/c"),
                PathBuf::from("/d"),
                ActionType::NeedsReview,
            ),
            PlannedAction {
                action_type: ActionType::Move,
                classification_method: ClassificationMethod::Llm,
                ..make_action(PathBuf::from("/e"), PathBuf::from("/f"), ActionType::Move)
            },
            make_action(PathBuf::from("/g"), PathBuf::from("/h"), ActionType::Skip),
            make_action(
                PathBuf::from("/i"),
                PathBuf::from("/j"),
                ActionType::Collision,
            ),
            make_action(
                PathBuf::from("/k"),
                PathBuf::from("/l"),
                ActionType::Ignored,
            ),
        ];

        let stats = PlanStats::from_actions(&actions);
        assert_eq!(stats.total_files, 6);
        assert_eq!(stats.rule_matched, 1); // first Move with Rule
        assert_eq!(stats.ai_classified, 1); // Move with Llm
        assert_eq!(stats.needs_review, 1);
        assert_eq!(stats.skipped, 1);
        assert_eq!(stats.collisions, 1);
        assert_eq!(stats.ignored, 1);
    }

    // ----- Junk filename cleaning ------------------------------------------

    #[test]
    fn clean_img_pattern() {
        assert_eq!(
            clean_junk_filename("IMG_1234.jpg"),
            Some("img_1234.jpg".to_owned())
        );
        assert_eq!(
            clean_junk_filename("IMG_0001.png"),
            Some("img_0001.png".to_owned())
        );
    }

    #[test]
    fn clean_screenshot_pattern() {
        let result = clean_junk_filename("Screenshot 2026-04-17 at 14.23.05.png");
        assert_eq!(result, Some("screenshot-2026-04-17-142305.png".to_owned()));
    }

    #[test]
    fn clean_scan_pattern() {
        assert_eq!(
            clean_junk_filename("scan_0042.pdf"),
            Some("scan-0042.pdf".to_owned())
        );
    }

    #[test]
    fn clean_normal_name_noop() {
        assert_eq!(clean_junk_filename("report.pdf"), None);
        assert_eq!(clean_junk_filename("meeting-notes.md"), None);
        assert_eq!(clean_junk_filename("2024-budget.xlsx"), None);
    }

    // ----- 3-level depth check ---------------------------------------------

    #[test]
    fn depth_from_root_levels() {
        let root = Path::new("/archive");
        assert_eq!(depth_from_root(Path::new("/archive/file.txt"), root), 0);
        assert_eq!(depth_from_root(Path::new("/archive/a/file.txt"), root), 1);
        assert_eq!(depth_from_root(Path::new("/archive/a/b/file.txt"), root), 2);
        assert_eq!(
            depth_from_root(Path::new("/archive/a/b/c/file.txt"), root),
            3
        );
        assert_eq!(
            depth_from_root(Path::new("/archive/a/b/c/d/file.txt"), root),
            4
        );
    }

    // ----- PlanStats for Tag/Rename/Embedding variants ----------------------

    #[test]
    fn plan_stats_tag_and_rename_actions() {
        let actions = vec![
            PlannedAction {
                action_type: ActionType::Tag,
                classification_method: ClassificationMethod::Rule,
                ..make_action(PathBuf::from("/a"), PathBuf::from("/b"), ActionType::Tag)
            },
            PlannedAction {
                action_type: ActionType::Rename,
                classification_method: ClassificationMethod::FilenameEmbedding,
                ..make_action(PathBuf::from("/c"), PathBuf::from("/d"), ActionType::Rename)
            },
            PlannedAction {
                action_type: ActionType::Move,
                classification_method: ClassificationMethod::ContentEmbedding,
                ..make_action(PathBuf::from("/e"), PathBuf::from("/f"), ActionType::Move)
            },
        ];
        let stats = PlanStats::from_actions(&actions);
        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.rule_matched, 1); // Tag with Rule
        assert_eq!(stats.ai_classified, 2); // Rename+FilenameEmbedding, Move+ContentEmbedding
    }

    #[test]
    fn plan_stats_classification_method_none() {
        let actions = vec![PlannedAction {
            action_type: ActionType::Move,
            classification_method: ClassificationMethod::None,
            ..make_action(PathBuf::from("/a"), PathBuf::from("/b"), ActionType::Move)
        }];
        let stats = PlanStats::from_actions(&actions);
        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.rule_matched, 0);
        assert_eq!(stats.ai_classified, 0);
    }

    // ----- Apply with missing source file ---------------------------------

    #[test]
    fn apply_missing_source_reports_error() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("dest");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&dest).unwrap();

        let mut plan = Plan::new("missing-src", vec![dir.path().to_path_buf()], dest.clone());
        plan.actions.push(make_action(
            dir.path().join("nonexistent.txt"),
            dest.join("nonexistent.txt"),
            ActionType::Move,
        ));

        let report = plan.apply(&log, false).unwrap();
        assert_eq!(report.moved, 0);
        assert_eq!(report.skipped, 1);
        assert!(report.errors[0].contains("does not exist"));
    }

    // ----- Apply rejects non-Draft ----------------------------------------

    #[test]
    fn apply_rejects_non_draft() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("decisions.jsonl");

        let mut plan = Plan::new("double-apply", vec![], PathBuf::from("/dest"));
        plan.status = PlanStatus::Applied;

        let result = plan.apply(&log, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not in Draft"));
    }

    // ----- Rollback rejects non-Applied -----------------------------------

    #[test]
    fn rollback_rejects_non_applied() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("decisions.jsonl");

        let mut plan = Plan::new("bad-rollback", vec![], PathBuf::from("/dest"));
        // Still Draft
        let result = plan.rollback(&log);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not in Applied"));
    }

    // ----- Apply with Rename action type ----------------------------------

    #[test]
    fn apply_rename_action() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&src).unwrap();

        let file = src.join("old_name.txt");
        std::fs::write(&file, b"rename me").unwrap();

        let renamed = src.join("new_name.txt");

        let mut plan = Plan::new("rename-test", vec![src.clone()], src.clone());
        plan.actions.push(PlannedAction {
            rename_to: Some("new_name.txt".to_owned()),
            ..make_action(file.clone(), renamed.clone(), ActionType::Rename)
        });

        let report = plan.apply(&log, false).unwrap();
        assert_eq!(report.moved, 1);
        assert!(!file.exists());
        assert!(renamed.exists());
        assert_eq!(std::fs::read_to_string(&renamed).unwrap(), "rename me");
    }

    // ----- Apply with Tag-only action -------------------------------------

    #[test]
    fn apply_tag_only_action() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&src).unwrap();

        let file = src.join("document.pdf");
        std::fs::write(&file, b"content").unwrap();

        let mut plan = Plan::new("tag-test", vec![src.clone()], src.clone());
        plan.actions.push(PlannedAction {
            tags: vec!["work".to_owned(), "urgent".to_owned()],
            ..make_action(file.clone(), file.clone(), ActionType::Tag)
        });

        let report = plan.apply(&log, false).unwrap();
        assert_eq!(report.tagged, 1);
        assert!(file.exists(), "tag-only should not move the file");
    }

    // ----- Depth enforcement during apply ---------------------------------

    #[test]
    fn apply_depth_enforcement_skips_deep_paths() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source");
        let dest = dir.path().join("dest");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        let file = src.join("deep.txt");
        std::fs::write(&file, b"too deep").unwrap();

        // 4 levels deep exceeds MAX_DEPTH(3)
        let deep_dest = dest.join("a/b/c/d/deep.txt");

        let mut plan = Plan::new("depth-test", vec![src], dest);
        plan.actions
            .push(make_action(file.clone(), deep_dest, ActionType::Move));

        let report = plan.apply(&log, false).unwrap();
        assert_eq!(report.skipped, 1);
        assert!(report.errors[0].contains("exceeds"));
        assert!(file.exists(), "source should not be moved");
    }

    // ----- Plan::list skips malformed files --------------------------------

    #[test]
    fn list_plans_skips_malformed_json() {
        let dir = tempdir().unwrap();
        let plans_dir = dir.path().join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();

        // Write a valid plan
        let plan = Plan::new("valid", vec![], PathBuf::from("/d"));
        plan.save(&plans_dir).unwrap();

        // Write a malformed JSON file
        std::fs::write(plans_dir.join("broken.json"), "not valid json{{{").unwrap();

        let plans = Plan::list(&plans_dir).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].name, "valid");
    }

    // ----- Backup with rollback from backup --------------------------------

    #[test]
    fn rollback_from_backup_restores_files() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source");
        let dest = dir.path().join("dest");
        let backup_dir = dir.path().join("backups");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        let file = src.join("important.pdf");
        std::fs::write(&file, b"critical data").unwrap();

        let dest_file = dest.join("important.pdf");

        let mut plan = Plan::new("backup-rollback", vec![src], dest);
        plan.actions.push(make_action(
            file.clone(),
            dest_file.clone(),
            ActionType::Move,
        ));

        plan.backup(&backup_dir).unwrap();
        plan.apply(&log, false).unwrap();
        assert!(dest_file.exists());
        assert!(!file.exists());

        plan.rollback(&log).unwrap();
        assert!(file.exists());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "critical data");
    }

    // ----- Rename format ---------------------------------------------------

    #[test]
    fn rename_file_format() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 17).unwrap();
        let result = rename_file("IMG_1234.jpg", &date, "Tax Invoice", "jpg");
        assert_eq!(result, "2026-04-17_tax-invoice.jpg");
    }

    #[test]
    fn rename_file_strips_dot_prefix_from_ext() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let result = rename_file("test.pdf", &date, "Report", ".pdf");
        assert_eq!(result, "2026-01-01_report.pdf");
    }

    #[test]
    fn rename_file_special_characters_in_topic() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let result = rename_file("x.txt", &date, "Q2 2026 -- Final (v3)", "txt");
        assert_eq!(result, "2026-06-15_q2-2026-final-v3.txt");
    }

    // ----- Save/load round-trip to disk ------------------------------------

    #[test]
    fn plan_save_load_round_trip() {
        let dir = tempdir().unwrap();
        let plans_dir = dir.path().join("plans");

        let mut plan = Plan::new(
            "disk-test",
            vec![PathBuf::from("/src")],
            PathBuf::from("/dest"),
        );
        plan.actions.push(make_action(
            PathBuf::from("/src/file.pdf"),
            PathBuf::from("/dest/file.pdf"),
            ActionType::Move,
        ));
        plan.stats = PlanStats::from_actions(&plan.actions);

        plan.save(&plans_dir).unwrap();

        let plan_path = plans_dir.join(format!("{}.json", plan.id));
        let loaded = Plan::load(&plan_path).unwrap();

        assert_eq!(loaded.id, plan.id);
        assert_eq!(loaded.name, "disk-test");
        assert_eq!(loaded.actions.len(), 1);
    }

    // ----- Apply moves files -----------------------------------------------

    #[test]
    fn apply_moves_files_to_correct_destinations() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source");
        let dest = dir.path().join("dest");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        let file_a = src.join("a.pdf");
        let file_b = src.join("b.txt");
        std::fs::write(&file_a, b"pdf content").unwrap();
        std::fs::write(&file_b, b"txt content").unwrap();

        let dest_a = dest.join("docs").join("a.pdf");
        let dest_b = dest.join("notes").join("b.txt");

        let mut plan = Plan::new("apply-test", vec![src], dest);
        plan.actions.push(make_action(
            file_a.clone(),
            dest_a.clone(),
            ActionType::Move,
        ));
        plan.actions.push(make_action(
            file_b.clone(),
            dest_b.clone(),
            ActionType::Move,
        ));

        let report = plan.apply(&log, false).unwrap();

        assert_eq!(report.moved, 2);
        assert_eq!(report.skipped, 0);
        assert!(!file_a.exists());
        assert!(!file_b.exists());
        assert!(dest_a.exists());
        assert!(dest_b.exists());
        assert_eq!(std::fs::read_to_string(&dest_a).unwrap(), "pdf content");
        assert_eq!(std::fs::read_to_string(&dest_b).unwrap(), "txt content");
    }

    // ----- Rollback restores files -----------------------------------------

    #[test]
    fn rollback_restores_files() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source");
        let dest = dir.path().join("dest");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        let file = src.join("doc.pdf");
        std::fs::write(&file, b"important").unwrap();

        let dest_file = dest.join("filed").join("doc.pdf");

        let mut plan = Plan::new("rollback-test", vec![src], dest);
        plan.actions.push(make_action(
            file.clone(),
            dest_file.clone(),
            ActionType::Move,
        ));

        plan.apply(&log, false).unwrap();
        assert!(dest_file.exists());
        assert!(!file.exists());

        plan.rollback(&log).unwrap();
        assert!(file.exists());
        assert!(!dest_file.exists());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "important");
    }

    // ----- Collision handling -----------------------------------------------

    #[test]
    fn collision_skips_when_destination_exists() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source");
        let dest = dir.path().join("dest");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        let source_file = src.join("conflict.txt");
        std::fs::write(&source_file, b"new version").unwrap();

        let dest_file = dest.join("conflict.txt");
        std::fs::write(&dest_file, b"existing version").unwrap();

        let mut plan = Plan::new("collision-test", vec![src], dest);
        plan.actions.push(make_action(
            source_file.clone(),
            dest_file.clone(),
            ActionType::Move,
        ));

        let report = plan.apply(&log, false).unwrap();

        assert_eq!(report.collisions, 1);
        assert_eq!(report.moved, 0);
        // Source should still exist, destination unchanged.
        assert!(source_file.exists());
        assert_eq!(
            std::fs::read_to_string(&dest_file).unwrap(),
            "existing version"
        );
    }

    // ----- Backup copies files correctly -----------------------------------

    #[test]
    fn backup_copies_files_correctly() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source");
        let backup_dir = dir.path().join("backups");
        std::fs::create_dir_all(&src).unwrap();

        let file = src.join("important.pdf");
        std::fs::write(&file, b"critical data").unwrap();

        let mut plan = Plan::new("backup-test", vec![src.clone()], dir.path().join("dest"));
        plan.actions.push(make_action(
            file.clone(),
            dir.path().join("dest/important.pdf"),
            ActionType::Move,
        ));

        plan.backup(&backup_dir).unwrap();

        assert!(plan.backup_path.is_some());
        let backup_path = plan.backup_path.as_ref().unwrap();
        let backed_up = backup_path.join("important.pdf");
        assert!(backed_up.exists());
        assert_eq!(
            std::fs::read_to_string(&backed_up).unwrap(),
            "critical data"
        );
    }

    // ----- Aggressive gate -------------------------------------------------

    #[test]
    fn aggressive_requires_backup() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("decisions.jsonl");

        let mut plan = Plan::new(
            "aggressive-gate",
            vec![PathBuf::from("/src")],
            PathBuf::from("/dest"),
        );

        let result = plan.apply(&log, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("backup"));
    }

    // ----- List plans ------------------------------------------------------

    #[test]
    fn list_plans_in_directory() {
        let dir = tempdir().unwrap();
        let plans_dir = dir.path().join("plans");

        // No directory yet — should return empty.
        let empty = Plan::list(&plans_dir).unwrap();
        assert!(empty.is_empty());

        // Save two plans.
        let plan1 = Plan::new("first", vec![], PathBuf::from("/d"));
        plan1.save(&plans_dir).unwrap();

        // Small delay to ensure different timestamps.
        let plan2 = Plan::new("second", vec![], PathBuf::from("/d"));
        plan2.save(&plans_dir).unwrap();

        let plans = Plan::list(&plans_dir).unwrap();
        assert_eq!(plans.len(), 2);
    }

    // ----- Soft delete (_Trash/) -------------------------------------------

    #[test]
    fn soft_delete_moves_file_to_trash_and_can_be_restored() {
        let dir = tempdir().unwrap();
        let source_root = dir.path().join("inbox");
        let trash_dir = dir.path().join("_Trash");
        let log = dir.path().join("decisions.jsonl");
        std::fs::create_dir_all(&source_root).unwrap();

        // Write a file to soft-delete.
        let file = source_root.join("report.pdf");
        std::fs::write(&file, b"sensitive data").unwrap();
        assert!(file.exists());

        let plan = Plan::new(
            "trash-test",
            vec![source_root.clone()],
            dir.path().join("dest"),
        );

        let trash_path = plan
            .soft_delete(&file, &source_root, &trash_dir, &log)
            .unwrap();

        // Original should be gone; trash copy should exist.
        assert!(!file.exists(), "original file should have been moved");
        assert!(trash_path.exists(), "file should exist in trash");
        assert_eq!(
            std::fs::read_to_string(&trash_path).unwrap(),
            "sensitive data"
        );

        // Decision log should record the soft_delete.
        let decisions = crate::decision::read_decisions(&log).unwrap();
        assert_eq!(decisions.len(), 1);
        assert!(decisions[0].action.contains("soft_delete"));

        // Restore: move back from trash to original location.
        std::fs::rename(&trash_path, &file).unwrap();
        assert!(
            file.exists(),
            "file should be restored to original location"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "sensitive data");
    }
}
