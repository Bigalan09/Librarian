# Data Model: Librarian — File Organisation CLI

**Date**: 2026-04-17
**Feature**: [spec.md](spec.md) | [plan.md](plan.md)

## Entities

### FileEntry

Represents a filesystem object discovered during scanning.

| Field | Type | Description |
|-------|------|-------------|
| path | PathBuf | Absolute path to the file |
| name | String | Filename with extension |
| extension | Option\<String\> | File extension (lowercase, no dot) |
| size_bytes | u64 | File size in bytes |
| hash | String | blake3 hex digest |
| created_at | DateTime\<Utc\> | File creation timestamp |
| modified_at | DateTime\<Utc\> | Last modification timestamp |
| tags | Vec\<String\> | Current Finder tags (read from xattr or sidecar) |
| colour | Option\<FinderColour\> | Current Finder colour label |
| source_inbox | String | Which inbox folder this file was found in |

**Identity**: `hash` (blake3 of file content). Two files with identical content share the same hash regardless of name or location.

**Lifecycle**: Created during scan → classified → included in plan → moved (or skipped) → logged to decision log.

### Rule

A deterministic classification rule loaded from `rules.yaml`.

| Field | Type | Description |
|-------|------|-------------|
| name | String | Human-readable rule name |
| match_criteria | MatchCriteria | Pattern fields to match against |
| destination | String | Destination path template (supports `{year}`, `{month}`, etc.) |
| tags | Vec\<String\> | Tags to apply on match |
| colour | Option\<FinderColour\> | Colour label to apply on match |
| clean_name | bool | Whether to clean junk filename patterns (default: false) |

**Match precedence**: Rules evaluated in definition order. First match wins.

### MatchCriteria

| Field | Type | Description |
|-------|------|-------------|
| extension | Option\<String\> | File extension to match (exact, case-insensitive) |
| filename | Option\<String\> | Filename glob pattern (or `regex:` prefixed regex) |
| path | Option\<String\> | Path glob pattern (or `regex:` prefixed regex) |
| content | Option\<String\> | Content pattern for text-based files (glob or `regex:` prefix) |
| min_size | Option\<u64\> | Minimum file size in bytes |
| max_size | Option\<u64\> | Maximum file size in bytes |

All fields are optional. A rule matches when ALL specified fields match (AND logic).

### Plan

A named, serialised set of proposed actions.

| Field | Type | Description |
|-------|------|-------------|
| id | String | Unique plan identifier (name or auto-generated) |
| name | String | Human-readable plan name |
| created_at | DateTime\<Utc\> | When the plan was generated |
| source_folders | Vec\<PathBuf\> | Inbox folders that were scanned |
| destination_root | PathBuf | Target root directory |
| actions | Vec\<PlannedAction\> | Ordered list of proposed actions |
| status | PlanStatus | Current plan state |
| applied_at | Option\<DateTime\<Utc\>\> | When the plan was applied |
| backup_path | Option\<PathBuf\> | Path to backup directory (if --backup was used) |
| stats | PlanStats | Summary statistics |

**State transitions**: `Draft` → `Applied` → `RolledBack`. Also `Draft` → `Deleted`.

### PlannedAction

| Field | Type | Description |
|-------|------|-------------|
| file_hash | String | blake3 hash of the source file |
| source_path | PathBuf | Original file location |
| destination_path | PathBuf | Proposed destination |
| action_type | ActionType | Move, Rename, Tag, Skip, NeedsReview |
| classification_method | ClassificationMethod | Rule, FilenameEmbedding, ContentEmbedding, LLM |
| confidence | Option\<f64\> | Confidence score (None for rules) |
| tags | Vec\<String\> | Tags to apply |
| colour | Option\<FinderColour\> | Colour label to apply |
| rename_to | Option\<String\> | New filename (if --rename and renaming applies) |
| original_name | Option\<String\> | Original filename (preserved for rollback) |
| reason | Option\<String\> | Explanation (especially for NeedsReview/Skip) |

### PlanStatus

Enum: `Draft`, `Applied`, `RolledBack`, `Deleted`

### ActionType

Enum: `Move`, `Rename`, `Tag`, `Skip`, `NeedsReview`, `Collision`, `Ignored`

### ClassificationMethod

Enum: `Rule`, `FilenameEmbedding`, `ContentEmbedding`, `LLM`, `None`

### Decision

An immutable audit record appended to `decisions.jsonl`.

| Field | Type | Description |
|-------|------|-------------|
| timestamp | DateTime\<Utc\> | When the decision was made |
| decision_type | DecisionType | Classification, Move, Tag, Skip, Collision, Correction, Reorganisation |
| file_hash | String | blake3 hash of the file |
| file_path | PathBuf | File path at the time of decision |
| classification_method | Option\<ClassificationMethod\> | How the file was classified |
| provider | Option\<String\> | AI provider used (if any) |
| model | Option\<String\> | Model name (if AI was used) |
| confidence | Option\<f64\> | Confidence score (if AI was used) |
| action | String | What was done (e.g., "moved to /2026/Work/Invoices/") |
| outcome | DecisionOutcome | Success, Skipped, Failed, Corrected |
| plan_id | Option\<String\> | Associated plan ID |
| metadata | Option\<serde_json::Value\> | Additional context (reason notes, error details) |

### DecisionType

Enum: `Classification`, `Move`, `Rename`, `Tag`, `Skip`, `Collision`, `Correction`, `Reorganisation`, `Ignored`

### DecisionOutcome

Enum: `Success`, `Skipped`, `Failed`, `Corrected`

### Correction

A specialised decision subtype.

| Field | Type | Description |
|-------|------|-------------|
| original_decision_hash | String | Hash linking to the original placement decision |
| original_path | PathBuf | Where Librarian placed the file |
| corrected_path | PathBuf | Where the user moved it |
| correction_source | CorrectionSource | How the correction was detected |
| corrected_tags | Option\<Vec\<String\>\> | Updated tags (if re-tagged) |

### CorrectionSource

Enum: `Watched`, `Explicit`, `Review`

### Bucket

A destination folder with learned embeddings.

| Field | Type | Description |
|-------|------|-------------|
| name | String | Bucket name (e.g., "Invoices") |
| path | PathBuf | Full path within managed structure |
| centroid | Option\<Vec\<f32\>\> | Learned centroid embedding vector |
| file_count | u64 | Number of files placed in this bucket |
| last_updated | DateTime\<Utc\> | When the centroid was last recalculated |
| source_inbox | String | Which inbox this bucket's learning is scoped to |
| filetype_scope | Option\<String\> | Optional filetype scope for learning isolation |

### FinderColour

Enum: `None` (0), `Grey` (1), `Green` (2), `Purple` (3), `Blue` (4), `Yellow` (5), `Red` (6), `Orange` (7)

Mapped to Finder colour index values for xattr storage.

### ProviderConfig

| Field | Type | Description |
|-------|------|-------------|
| provider_type | ProviderType | LMStudio or OpenAI |
| base_url | String | API base URL |
| api_key | Option\<String\> | API key (OpenAI only) |
| llm_model | Option\<String\> | Model name for chat completions |
| embed_model | Option\<String\> | Model name for embeddings |
| rate_limit_rpm | Option\<u32\> | Requests per minute limit (OpenAI only) |

### ProviderType

Enum: `LMStudio`, `OpenAI`

### AppConfig

Top-level configuration loaded from `~/.librarian/config.yaml`.

| Field | Type | Description |
|-------|------|-------------|
| inbox_folders | Vec\<PathBuf\> | Folders to scan (default: Downloads, Desktop) |
| destination_root | PathBuf | Managed folder root (default: ~/Library-Managed) |
| needs_review_path | PathBuf | NeedsReview folder (default: \<destination\>/NeedsReview) |
| trash_path | PathBuf | Managed trash (default: \<destination\>/_Trash) |
| provider | ProviderConfig | Default AI provider configuration |
| thresholds | Thresholds | Confidence thresholds per classification layer |
| correction_window_days | u32 | Days to consider moves as corrections (default: 14) |
| max_moves_per_run | u32 | Hard limit on moves per run (default: 500) |
| fewshot_count | u32 | Number of recent corrections for few-shot injection (default: 20) |
| rule_suggestion_threshold | u32 | Correction count before suggesting a rule (default: 3) |

### Thresholds

| Field | Type | Description |
|-------|------|-------------|
| filename_embedding | f64 | Cosine similarity to accept filename embedding (default: 0.80) |
| content_embedding | f64 | Cosine similarity to accept content embedding (default: 0.75) |
| llm_confidence | f64 | Self-reported LLM confidence to accept (default: 0.70) |

## Relationships

```text
AppConfig ──1:N──> InboxFolder (configured sources)
AppConfig ──1:1──> ProviderConfig
AppConfig ──1:1──> Thresholds

FileEntry ──N:1──> InboxFolder (source)
FileEntry ──1:N──> Decision (audit trail)
FileEntry ──1:N──> PlannedAction (proposed actions)

Rule ──1:1──> MatchCriteria
Rule ──1:N──> PlannedAction (when rule matches)

Plan ──1:N──> PlannedAction
Plan ──0:1──> BackupDirectory

Decision ──0:1──> Correction (if type is Correction)
Correction ──1:1──> Decision (original placement)

Bucket ──1:N──> FileEntry (files placed here)
Bucket ──0:1──> Centroid embedding (learned from corrections)
Bucket ──N:1──> InboxFolder (learning isolation scope)
```

## Storage Layout

```text
~/.librarian/
├── config.yaml           # AppConfig (YAML, human-editable)
├── rules.yaml            # Vec<Rule> (YAML, human-editable)
├── ignore                # Global ignore patterns (gitignore syntax, plain text)
├── plans/
│   └── <plan-id>.json    # Plan (JSON)
├── history/
│   ├── decisions.jsonl   # Vec<Decision> (JSONL, append-only)
│   └── corrections.jsonl # Vec<Correction> (JSONL, subset for fast scanning)
├── cache/
│   └── embeddings.msgpack # HashMap<blake3_hash, Vec<f32>> (msgpack)
├── backup/
│   └── <plan-id>/        # Original files before aggressive moves
├── state/
│   ├── manifest.json     # Current known state of managed folders
│   └── centroids.msgpack # HashMap<(inbox, filetype, bucket), Vec<f32>>
└── logs/
    └── librarian.log     # Tracing output (rotated)
```
