# Feature Specification: Librarian — File Organisation CLI

**Feature Branch**: `001-rust-file-organiser-cli`
**Created**: 2026-04-17
**Status**: Draft
**Input**: User description: Librarian PRD v0.1

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Deterministic Rule-Based File Organisation (Priority: P1)

As a user with an overflowing Downloads folder, I want Librarian to scan my inbox folders, match files against a set of deterministic rules I define, and propose a plan of moves and tag applications into a shallow year/category/bucket folder structure, so that I can review and apply the plan to organise my files without any AI dependency.

**Why this priority**: The rules engine is the foundation of the entire system. It delivers immediate value with zero external dependencies (no LM Studio, no OpenAI, no Qdrant). Every subsequent feature builds on top of it. A user can be productive with rules alone.

**Independent Test**: Can be fully tested by creating a temp directory with sample files, defining rules in `rules.yaml`, running `librarian process --source <temp>`, inspecting the generated plan, then running `librarian apply` and verifying files land in the correct destinations with correct Finder tags.

**Acceptance Scenarios**:

1. **Given** a `rules.yaml` with a rule matching `*.pdf` files containing "invoice" in the filename, **When** I run `librarian process --source ~/Downloads`, **Then** the generated plan proposes moving matching PDFs to `<destination>/2026/Work/Invoices/` with tag `invoice`.
2. **Given** a plan has been generated, **When** I run `librarian apply --plan <name>`, **Then** all proposed moves are executed, Finder tags are applied on macOS (or sidecar metadata on other platforms), and a decision log entry is written for each action.
3. **Given** an applied plan, **When** I run `librarian rollback --plan <name>`, **Then** all moved files are returned to their original locations and applied tags are removed.
4. **Given** a file at the destination already has the same name, **When** Librarian encounters the collision during apply, **Then** it skips that file with a warning and logs the collision — it never overwrites or silently renames.
5. **Given** hidden files, `.DS_Store`, `node_modules`, files matching `.librarianignore`, or files matching `~/.librarian/ignore`, **When** Librarian scans, **Then** those files are excluded and logged as ignored.

---

### User Story 2 - AI-Powered Classification with Confidence Gating (Priority: P2)

As a user with files that do not match any rule, I want Librarian to classify them using a tiered AI pipeline (filename embedding, content embedding, LLM classifier) with configurable confidence thresholds, so that well-understood files are placed automatically and uncertain files are parked in a needs-review folder rather than misplaced.

**Why this priority**: AI classification is the differentiating feature. Without it, Librarian is just a rule-based mover. However, it depends on the rules engine, plan model, and file operations from US1 being complete.

**Independent Test**: Can be tested by placing files with no matching rules in an inbox folder, running `librarian process` with a mocked or live LM Studio instance, and verifying: high-confidence files are proposed for their correct destination; low-confidence files are proposed for the NeedsReview folder with a yellow colour label, `needs-review` tag, and a sidecar reason note.

**Acceptance Scenarios**:

1. **Given** a file with no matching rule and a filename embedding cosine similarity above 0.80 against an existing bucket centroid, **When** Librarian classifies it, **Then** it proposes the file for that bucket without escalating to content extraction or LLM.
2. **Given** a file whose filename embedding scores below 0.80 but whose content embedding scores above 0.75, **When** Librarian classifies it, **Then** it proposes placement based on the content embedding match.
3. **Given** a file that falls below both embedding thresholds but the LLM classifier reports confidence above 0.70, **When** Librarian classifies it, **Then** it proposes placement at the LLM-suggested destination.
4. **Given** a file that falls below all confidence thresholds, **When** Librarian classifies it, **Then** it proposes moving the file to the NeedsReview folder with a yellow Finder colour label, a `needs-review` tag, and a sidecar reason note explaining why it was not placed.
5. **Given** the `--provider` flag is set to `openai`, **When** Librarian runs classification, **Then** it uses the OpenAI API with rate limiting (default 20 rpm) instead of LM Studio.

---

### User Story 3 - Correction-Driven Learning (Priority: P3)

As a user who has corrected Librarian's placements, I want those corrections to feed back into future classifications through few-shot prompt examples, auto-generated rule proposals, and embedding centroid drift, so that Librarian improves over time and makes fewer mistakes on similar files.

**Why this priority**: Learning is what makes Librarian more than a one-shot organiser. However, it requires the classification pipeline (US2) and plan execution (US1) to already be working so corrections can be recorded and replayed.

**Independent Test**: Can be tested by running a full process-apply cycle, manually moving a placed file to a different destination (simulating a correction), running `librarian process` again on a similar file, and verifying the new classification reflects the correction. Also testable via `librarian correct FILE --to PATH` followed by `librarian rules suggest` to verify rule proposals.

**Acceptance Scenarios**:

1. **Given** I moved a file that Librarian placed within the correction window (default 14 days), **When** Librarian detects the move via file-system watching, **Then** it records a correction entry in `decisions.jsonl` with `type: correction`.
2. **Given** I run `librarian correct invoice.pdf --to ~/Library-Managed/2026/Work/Invoices/`, **When** Librarian processes the command, **Then** a correction is recorded and the last 20 relevant corrections are available as few-shot examples for future LLM classification prompts.
3. **Given** the same correction pattern has occurred three times (same source folder, same filetype, same target), **When** I run `librarian rules suggest`, **Then** Librarian proposes a new `rules.yaml` entry capturing that pattern.
4. **Given** multiple corrections moving PDFs from `Research/` to `Invoices/`, **When** Librarian recalculates bucket centroids, **Then** the `Invoices` centroid shifts toward the corrected file embeddings, and future similar PDFs score higher against `Invoices`.
5. **Given** corrections from the Downloads inbox, **When** Librarian classifies a file from the Desktop inbox, **Then** Downloads-specific corrections do not influence Desktop classification (per-folder learning isolation).

---

### User Story 4 - Plan Management and Safety (Priority: P4)

As a user who wants confidence that Librarian will not lose or damage my files, I want every operation to go through a reviewable plan with backup, rollback, and audit capabilities, so that I maintain full control and can undo any action.

**Why this priority**: Safety is non-negotiable but is enabled by the plan data model from US1. This story focuses on the advanced safety features: named plan management, backup before aggressive moves, and the append-only decision log.

**Independent Test**: Can be tested by running `librarian process`, reviewing the plan with `librarian plans show <name>`, applying with `--backup`, verifying originals are copied to `~/.librarian/backup/<plan-id>/`, rolling back, and confirming all files return to their original state.

**Acceptance Scenarios**:

1. **Given** I run `librarian process --plan-name my-plan`, **When** the scan completes, **Then** the plan is saved to `~/.librarian/plans/my-plan.json` and listed by `librarian plans`.
2. **Given** I run `librarian apply --plan my-plan --backup`, **When** the apply executes, **Then** original files are copied to `~/.librarian/backup/my-plan/` before any moves occur.
3. **Given** I run `librarian apply --plan my-plan --aggressive` without a preceding `--backup` for the same plan, **When** Librarian checks the gate, **Then** it refuses with an error explaining that `--aggressive` requires a successful `--backup` within the same plan.
4. **Given** any operation (move, tag, skip, collision, correction), **When** the operation completes, **Then** it is appended to `~/.librarian/history/decisions.jsonl` with timestamp, input hash, provider, model, confidence, and outcome.
5. **Given** a run exceeds the configured move limit (default 500), **When** Librarian hits the limit, **Then** it stops proposing additional moves, reports the limit was reached, and saves the partial plan.

---

### Edge Cases

- What happens when a file is open in another process during move? Librarian detects open files and skips them with a warning in the plan.
- What happens when the destination folder does not exist? Librarian creates it, respecting the three-level-max depth constraint.
- What happens when LM Studio is not running and `--provider lmstudio` is specified? Librarian fails the startup validation (`GET /v1/models`), reports the error, and exits gracefully without partial processing.
- What happens when the embedding cache is corrupted? Librarian detects invalid data, logs a warning, clears the cache, and rebuilds from scratch on the next run.
- What happens when `rules.yaml` has syntax errors? `librarian rules validate` reports all errors with line numbers. `librarian process` refuses to run with an invalid rules file.
- What happens when a symlink points outside the source directory? It is treated as an ignored file and logged.
- What happens when the decision log file is locked or inaccessible? Librarian fails loudly rather than silently dropping audit records.

## Clarifications

### Session 2026-04-17

- Q: What matching semantics does the rules engine use? → A: Glob patterns by default (gitignore-style); regex opt-in via `regex:` prefix on any pattern field.
- Q: Which file types support content extraction for embeddings? → A: Text-based files only — plain text, PDF text layer, markdown, CSV. No OCR, no office document parsing, no binary content extraction. Binary files (images, videos, audio) classify on filename embedding and LLM only.
- Q: Soft-delete mechanism — managed `_Trash/` or macOS Trash? → A: Managed `_Trash/` folder in the destination root. Portable, observable, simple rollback. No macOS Trash API integration.
- Q: Should moves after the correction window count as corrections? → A: Hard cutoff at the configured window (default 14 days). Moves after the window are logged as `type: reorganisation` but do not feed the learning layer.
- Q: Should `--aggressive` require backup within the same plan or within a time window? → A: Same plan only. `--aggressive` requires `--backup` to have succeeded for the exact same plan. No time-based or cross-plan gate.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST scan configured inbox folders (Downloads, Desktop, Documents, and user-configured paths) and enumerate all non-ignored files for classification.
- **FR-002**: System MUST evaluate files against a deterministic rules engine (`rules.yaml`) using glob pattern matching by default, with regex opt-in via `regex:` prefix on any pattern field. Rules MUST always take precedence over AI.
- **FR-003**: System MUST classify rule-unmatched files through a tiered pipeline: filename embedding lookup, content embedding lookup for text-based files only (plain text, PDF text layer, markdown, CSV) if filename confidence below 0.80, then LLM classifier (if content confidence below 0.75). Binary files (images, videos, audio) skip content extraction and escalate directly from filename embedding to LLM.
- **FR-004**: System MUST route files with LLM confidence below 0.70 to a NeedsReview folder with a yellow Finder colour label, `needs-review` tag, and a sidecar reason note.
- **FR-005**: System MUST generate a named plan (serialised set of proposed moves, renames, and tag applications) for every `process` run, saved to disk and listed by `librarian status`.
- **FR-006**: System MUST apply plans via `librarian apply`, executing proposed moves, applying Finder tags on macOS (or `.librarian-meta.json` sidecar on other platforms), and logging every action to the decision log.
- **FR-007**: System MUST rollback applied plans via `librarian rollback`, restoring files to original locations and removing applied tags.
- **FR-008**: System MUST record every decision (classification, move, rename, tag, collision, skip, correction) to an append-only JSONL file at `~/.librarian/history/decisions.jsonl`.
- **FR-009**: System MUST support three correction sources: watched file-system moves within a configurable window (default 14 days) with a hard cutoff (moves after the window are logged as `type: reorganisation` but excluded from the learning layer), explicit `librarian correct` commands, and interactive review via `librarian review`.
- **FR-010**: System MUST inject the last N relevant corrections (default 20) as few-shot examples into LLM classification prompts, filtered by folder and filetype.
- **FR-011**: System MUST propose auto-generated rules when the same correction pattern occurs three times (same source folder, same filetype, same target).
- **FR-012**: System MUST update per-bucket centroid embeddings when corrections are recorded, with per-folder and per-filetype isolation.
- **FR-013**: System MUST honour ignore rules from three sources: system defaults (hidden files, `.git`, `node_modules`, `.DS_Store`, external symlinks, open files), per-folder `.librarianignore`, and global `~/.librarian/ignore`.
- **FR-014**: System MUST enforce a configurable hard limit on moves per run (default 500).
- **FR-015**: System MUST never overwrite files at the destination. Collisions are skipped with a warning and logged.
- **FR-016**: System MUST support two AI providers: LM Studio (default, local) and OpenAI (opt-in, with rate limiting at default 20 rpm), switchable via `--provider` flag or config.
- **FR-017**: System MUST validate provider availability at startup and fail gracefully if unavailable.
- **FR-018**: System MUST only rename files when the `--rename` flag is explicitly provided, using format `YYYY-MM-DD_descriptive-slug.ext`, preserving original names in extended attributes.
- **FR-019**: System MUST clean junk filename patterns (IMG_1234, Screenshot timestamps, scan_0001) during moves even without `--rename`.
- **FR-020**: System MUST scaffold all required configuration and folder structure via `librarian init`.
- **FR-021**: System MUST support `--backup` on apply to copy originals before moves, and `--aggressive` MUST require a successful `--backup` within the exact same plan (no time-based or cross-plan gate).
- **FR-022**: System MUST support `--dry-run` (default for `process`), `--verbose`, `--json`, and `--quiet` output modes.
- **FR-023**: System MUST use a managed `_Trash/` folder in the destination root for soft-deletes (no macOS Trash API integration). Soft-deleted files remain observable and auditable within the managed structure.

### Key Entities

- **File**: A filesystem object to be classified — has path, name, extension, size, hash (blake3), creation/modification dates, and current tags.
- **Rule**: A deterministic classification entry in `rules.yaml` �� matches on filename patterns, extensions, paths, or content patterns using glob syntax by default (regex opt-in via `regex:` prefix); specifies destination, tags, and optional colour.
- **Plan**: A named, serialised set of proposed actions (moves, renames, tag applications) — can be saved, listed, applied, rolled back, and deleted.
- **Decision**: An immutable audit record — captures timestamp, file hash, classification method (rule/embedding/LLM), provider, model, confidence, proposed action, and outcome.
- **Correction**: A decision subtype recording that a previous placement was wrong — includes original placement, corrected destination, and source (watched/explicit/review).
- **Bucket**: A destination folder within the managed structure — has a name, path, and a learned centroid embedding updated by corrections.
- **Provider**: An AI backend (LM Studio or OpenAI) — exposes chat completion and embedding endpoints with provider-specific configuration (base URL, API key, rate limits).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: User can organise 1,000 files from inbox folders into the correct folder structure in under 3 minutes of wall-clock processing time.
- **SC-002**: Files matched by deterministic rules are classified with 100% accuracy (rules are exact matches, no false positives).
- **SC-003**: AI-classified files achieve at least 80% correct placement (measured by the proportion not subsequently corrected within the correction window).
- **SC-004**: Fewer than 5% of files are placed in the wrong destination without being flagged as low confidence — the system prefers NeedsReview over guessing.
- **SC-005**: After 50 corrections, classification accuracy on similar file types improves measurably (fewer corrections needed per run).
- **SC-006**: Zero files are lost or overwritten during any operation — every destructive action is gated behind explicit flags and backups.
- **SC-007**: Any applied plan can be fully rolled back, restoring files to their original locations and removing applied metadata, within 30 seconds for a 500-file plan.
- **SC-008**: User can review and understand every decision Librarian made via the decision log, with sufficient context to diagnose any unexpected placement.
- **SC-009**: The system runs entirely locally by default with no data sent to external services unless the user explicitly opts into OpenAI.
- **SC-010**: All user-facing output uses British English spelling and grammar.

## Assumptions

- User has macOS as the primary platform with Finder tag and colour label support via extended attributes.
- LM Studio is installed and running locally when AI features are used; if not, rule-based classification still functions independently.
- Qdrant is available via Docker for vector storage when AI classification is enabled.
- User has sufficient disk space for backup copies when using `--backup` with `--aggressive`.
- The managed folder root (e.g., `~/Library-Managed`) is on a local filesystem, not a network mount.
- Files in inbox folders are not being actively written to during a `process` run (Librarian skips open files but does not wait for writes to complete).
- The three-level folder depth limit (year/category/bucket) is sufficient for all personal file organisation needs.
- British English is the only required locale for user-facing strings.
- v1 does not include daemon/watcher mode — users run `librarian process` manually or via cron.
- The correction window default of 14 days is reasonable for distinguishing intentional reorganisation from corrections.
