//! Plan data model, apply, rollback.
//! This module will be fully implemented in Phase 3 (US1).
//! Placeholder types are defined here so other modules can reference them.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::decision::ClassificationMethod;
use crate::file_entry::FinderColour;

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
