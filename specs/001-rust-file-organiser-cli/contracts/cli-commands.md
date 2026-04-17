# CLI Command Contracts: Librarian

**Date**: 2026-04-17
**Scope**: v1 commands only

## Global Flags

All commands accept these flags:

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--verbose` | bool | false | DEBUG-level logging, human-readable |
| `--json` | bool | false | JSON lines output for scripting |
| `--quiet` | bool | false | Errors only |
| `--help` | bool | — | Show help text |
| `--version` | bool | — | Show version |

Mutual exclusion: `--verbose`, `--json`, `--quiet` are mutually exclusive.

---

## `librarian init`

Scaffold configuration and folder structure.

**Arguments**: None
**Flags**: None beyond global

**Behaviour**:
1. Create `~/.librarian/` directory structure
2. Write default `config.yaml` with commented examples
3. Write default `rules.yaml` with example rules
4. Write default `ignore` file with system defaults
5. Create destination root if configured

**Stdout**: Summary of created files and directories
**Stderr**: Warnings if files already exist (skip, do not overwrite)
**Exit codes**: 0 success, 1 error

---

## `librarian process`

Scan inbox folders, classify files, produce a plan.

**Arguments**: None (uses config for defaults)

**Flags**:

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--source` | PathBuf (repeatable) | config inbox_folders | Inbox folders to scan |
| `--destination` | PathBuf | config destination_root | Target root directory |
| `--provider` | enum | config provider | `lmstudio` or `openai` |
| `--llm-model` | String | config llm_model | Model for chat completions |
| `--embed-model` | String | config embed_model | Model for embeddings |
| `--rules` | PathBuf | ~/.librarian/rules.yaml | Rules file path |
| `--threshold` | f64 | config thresholds | Override all confidence thresholds |
| `--dry-run` | bool | true | Generate plan without applying |
| `--plan-name` | String | auto-generated | Name for the saved plan |
| `--rename` | bool | false | Also propose renames |

**Stdout**: Progress bars (scan, classify, plan), then summary table
**Stderr**: Warnings, debug logs
**Exit codes**: 0 success (plan created), 1 error (invalid config/rules), 2 provider unavailable

**Plan auto-naming**: `<source-name>-<YYYY-MM-DD>-<HHMM>` (e.g., `downloads-2026-04-17-1423`)

---

## `librarian apply`

Execute a previously generated plan.

**Flags**:

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--plan` | String | most recent | Plan name or path |
| `--backup` | bool | false | Copy originals to backup before moves |
| `--aggressive` | bool | false | Allow moves without keeping source copy (requires prior --backup for same plan) |
| `--dry-run` | bool | false | Show what would happen without executing |

**Gate**: `--aggressive` fails with error if `--backup` has not succeeded for the same plan.

**Stdout**: Progress bar, then summary of applied actions
**Stderr**: Warnings for skipped files (collisions, open files)
**Exit codes**: 0 all actions succeeded, 1 partial (some skipped), 2 fatal error

---

## `librarian rollback`

Reverse an applied plan.

**Flags**:

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--plan` | String | most recent applied | Plan to rollback |

**Behaviour**: Restore files to original locations, remove applied tags. If backup exists, restore from backup. If no backup, reverse moves directly.

**Stdout**: Progress bar, summary of restored files
**Exit codes**: 0 success, 1 partial rollback, 2 plan not found or not applied

---

## `librarian status`

Show current state: recent plans, pending reviews, active runs.

**Arguments**: None
**Flags**: None beyond global

**Stdout**:
```text
Recent plans:
  downloads-2026-04-17-1423  Applied  799 moves  2026-04-17 14:23
  desktop-2026-04-16-0900    Draft    45 moves   2026-04-16 09:00

Pending review: 224 files in NeedsReview
Last run: 2026-04-17 14:23 (1023 files processed)
```

**Exit codes**: 0 always

---

## `librarian plans`

List, show, or delete named plans.

**Subcommands**:
- `librarian plans` (no subcommand) — list all plans
- `librarian plans show <NAME>` — show plan details
- `librarian plans delete <NAME>` — delete a plan

**Exit codes**: 0 success, 1 plan not found

---

## `librarian rules validate`

Validate a rules file.

**Flags**:

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--rules` | PathBuf | ~/.librarian/rules.yaml | Rules file to validate |

**Stdout**: "Rules valid" or list of errors with line numbers
**Exit codes**: 0 valid, 1 invalid

---

## `librarian rules suggest`

Emit proposed rules from correction history.

**Flags**: None beyond global

**Stdout**: Proposed `rules.yaml` entries as YAML, with diff against current rules
**Exit codes**: 0 suggestions generated, 0 no suggestions (prints message)

---

## `librarian correct`

Record an explicit correction.

**Arguments**: `FILE` — path to the file to correct

**Flags**:

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--to` | PathBuf | — | Correct destination path |
| `--retag` | String (comma-separated) | — | Correct tags |

At least one of `--to` or `--retag` MUST be provided.

**Stdout**: Confirmation of recorded correction
**Exit codes**: 0 success, 1 file not found or not managed

---

## `librarian review`

Interactive review of needs-review folder.

**Arguments**: None
**Flags**: None beyond global

**Behaviour**: Present files in NeedsReview one at a time. For each file, show: filename, reason note, suggested destinations. User can: accept suggestion, choose different destination, skip, or quit.

**Stdin**: Interactive prompts (not a TUI in v1)
**Stdout**: File details and prompts
**Exit codes**: 0 completed or user quit

---

## `librarian config show`

Print current configuration.

**Stdout**: YAML dump of resolved config (defaults merged with user overrides)
**Exit codes**: 0 always

---

## `librarian config edit`

Open config in `$EDITOR`.

**Behaviour**: Launch `$EDITOR` (or `vi` fallback) on `~/.librarian/config.yaml`.
**Exit codes**: 0 editor exited cleanly, 1 editor not found
