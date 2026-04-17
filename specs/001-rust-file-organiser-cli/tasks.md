# Tasks: Librarian — File Organisation CLI

**Input**: Design documents from `specs/001-rust-file-organiser-cli/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

**Tests**: Included per constitution principle II (Test-First, NON-NEGOTIABLE).

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

**Model annotations**: Each task is tagged with the recommended Claude model:
- `{opus}` — Complex architecture, multi-file orchestration, nuanced algorithms
- `{sonnet}` — Standard implementation, well-defined patterns, moderate complexity
- `{haiku}` — Mechanical/boilerplate, config files, straightforward structs

## Format: `[ID] [P?] [Story] Description {model}`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions
- `{opus}` / `{sonnet}` / `{haiku}`: Recommended Claude model for the task

## Path Conventions

- Cargo workspace: `crates/<crate-name>/src/` for source
- Integration tests: `tests/integration/` at repository root
- Fixtures: `tests/fixtures/` at repository root

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Cargo workspace initialisation and shared project scaffolding

- [x] T001 {sonnet} Create Cargo workspace root `Cargo.toml` with 6 member crates (librarian-core, librarian-rules, librarian-providers, librarian-classifier, librarian-learning, librarian-cli)
- [x] T002 [P] {haiku} Create `crates/librarian-core/Cargo.toml` with dependencies: serde, serde_json, serde_yaml, blake3, chrono, thiserror, tracing, xattr, plist, ignore (plist for Finder tag binary plist encoding, ignore for gitignore-syntax .librarianignore parsing)
- [x] T003 [P] {haiku} Create `crates/librarian-rules/Cargo.toml` with dependencies: globset, regex, serde, serde_yaml, thiserror, librarian-core
- [x] T004 [P] {haiku} Create `crates/librarian-providers/Cargo.toml` with dependencies: reqwest, tokio, serde, serde_json, rmp-serde, thiserror, tracing, librarian-core
- [x] T005 [P] {haiku} Create `crates/librarian-classifier/Cargo.toml` with dependencies: qdrant-client, librarian-core, librarian-providers, librarian-rules, thiserror, tracing
- [x] T006 [P] {haiku} Create `crates/librarian-learning/Cargo.toml` with dependencies: notify, librarian-core, librarian-providers, thiserror, tracing
- [x] T007 [P] {haiku} Create `crates/librarian-cli/Cargo.toml` with dependencies: clap (derive), tokio, indicatif, tracing-subscriber, all librarian-* crates; dev-dependencies: assert_cmd, tempfile, mockito, predicates
- [x] T008 [P] {haiku} Create `tests/fixtures/sample_rules.yaml` with example rules covering glob and regex patterns per research R12
- [x] T009 [P] {haiku} Create `tests/fixtures/sample_config.yaml` with default config per AppConfig data model
- [x] T010 [P] {haiku} Create `tests/fixtures/sample_files/` directory with test files: 3 PDFs, 2 PNGs, 1 CSV, 1 markdown, 1 plain text
- [x] T011 {haiku} Create `.gitignore` with Rust defaults (target/, *.swp, .env) and Librarian-specific exclusions
- [x] T012 {sonnet} Create `crates/librarian-cli/src/main.rs` with clap App skeleton — all v1 subcommands defined with argument structs but stubbed handlers, plus global --verbose/--json/--quiet flags per `contracts/cli-commands.md`

**Checkpoint**: `cargo build` succeeds, `cargo test` passes (no tests yet), all crates compile

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**CRITICAL**: No user story work can begin until this phase is complete

### Tests for Foundational

- [x] T013 [P] {sonnet} Write unit tests for config loading in `crates/librarian-core/src/config.rs` — test default values, YAML parsing, missing fields, invalid paths
- [x] T014 [P] {sonnet} Write unit tests for blake3 hashing in `crates/librarian-core/src/hasher.rs` — test known digests, empty file, large file
- [x] T015 [P] {sonnet} Write unit tests for ignore engine in `crates/librarian-core/src/ignore.rs` — test system defaults, `.librarianignore`, global ignore, symlink detection
- [x] T016 [P] {sonnet} Write unit tests for directory walker in `crates/librarian-core/src/walker.rs` — test file enumeration, ignore integration, symlink handling, open file detection
- [x] T017 [P] {sonnet} Write unit tests for FileEntry in `crates/librarian-core/src/file_entry.rs` — test construction, hash population, metadata reading
- [x] T018 [P] {sonnet} Write unit tests for decision log in `crates/librarian-core/src/decision.rs` — test JSONL append, read-back, field completeness
- [x] T019 [P] {sonnet} Write unit tests for Finder tags in `crates/librarian-core/src/tags.rs` — test xattr read/write on macOS, sidecar fallback on other platforms, colour index mapping

### Implementation for Foundational

- [x] T020 [P] {sonnet} Implement AppConfig loading in `crates/librarian-core/src/config.rs` — parse `config.yaml`, merge defaults, validate paths, expose Thresholds and ProviderConfig per data model
- [x] T021 [P] {haiku} Implement FileEntry struct in `crates/librarian-core/src/file_entry.rs` — fields per data model (path, name, extension, size, hash, timestamps, tags, colour, source_inbox)
- [x] T022 [P] {sonnet} Implement blake3 hasher in `crates/librarian-core/src/hasher.rs` — async file hashing with streaming reads, hex digest output
- [x] T023 [P] {opus} Implement ignore engine in `crates/librarian-core/src/ignore.rs` — three-tier ignore (system defaults, per-folder `.librarianignore`, global `~/.librarian/ignore`), gitignore syntax via `ignore` crate, symlink-outside-source detection, open-file detection via `lsof`
- [x] T024 {opus} Implement directory walker in `crates/librarian-core/src/walker.rs` — async recursive scan with ignore integration, FileEntry construction, parallel hashing, respecting max-moves-per-run limit
- [x] T025 [P] {sonnet} Implement decision log in `crates/librarian-core/src/decision.rs` — Decision struct per data model, JSONL append with file locking, DecisionType and DecisionOutcome enums
- [x] T026 [P] {opus} Implement Finder tags in `crates/librarian-core/src/tags.rs` — xattr read/write via `xattr` crate for `com.apple.metadata:_kMDItemUserTags` (binary plist via `plist` crate), FinderColour via `com.apple.FinderInfo` byte 9, `#[cfg(target_os = "macos")]` gate, `.librarian-meta.json` sidecar fallback
- [x] T027 [P] {haiku} Implement lib.rs for librarian-core in `crates/librarian-core/src/lib.rs` — re-export all public types

**Checkpoint**: Foundation ready — `cargo test -p librarian-core` passes, all core types available for user stories

---

## Phase 3: User Story 1 — Deterministic Rule-Based File Organisation (Priority: P1) MVP

**Goal**: Scan inbox folders, match files against rules, generate/apply/rollback plans with Finder tags. Zero AI dependency.

**Independent Test**: Create temp directory with sample files, define rules, run `librarian process`, apply plan, verify files moved with correct tags, rollback, verify restored.

### Tests for User Story 1

- [x] T028 [P] {sonnet} [US1] Write unit tests for rules loader in `crates/librarian-rules/src/loader.rs` — test YAML parsing, glob patterns, regex patterns with `regex:` prefix, validation errors with line numbers, match precedence (first-match-wins)
- [x] T029 [P] {sonnet} [US1] Write unit tests for rules engine in `crates/librarian-rules/src/engine.rs` — test glob matching, regex opt-in, extension matching, path matching, content matching, AND logic across fields, min/max size filters
- [x] T030 [P] {sonnet} [US1] Write unit tests for plan model in `crates/librarian-core/src/plan.rs` — test Plan creation, PlannedAction construction, JSON serialisation round-trip, status transitions (Draft→Applied→RolledBack), stats calculation
- [ ] T031 [P] {opus} [US1] Write integration test for full process-apply-rollback lifecycle in `tests/integration/process_apply_rollback.rs` — create temp dir with fixtures, run process with rules, verify plan, apply, verify moves and tags, rollback, verify restoration
- [ ] T032 [P] {sonnet} [US1] Write integration test for collision handling in `tests/integration/process_apply_rollback.rs` — verify skip on filename collision, warning logged, decision log entry with Collision type

### Implementation for User Story 1

- [x] T033 [P] {sonnet} [US1] Implement rules YAML loader in `crates/librarian-rules/src/loader.rs` — parse Rule and MatchCriteria per data model, validate patterns (compile glob/regex), report errors with line numbers
- [x] T034 {opus} [US1] Implement rules engine in `crates/librarian-rules/src/engine.rs` — evaluate FileEntry against Vec<Rule>, glob matching via `globset`, regex opt-in via `regex:` prefix detection, first-match-wins precedence, AND logic for multi-field criteria, content matching for text files only
- [x] T035 {sonnet} [US1] Implement Plan data model in `crates/librarian-core/src/plan.rs` — Plan, PlannedAction, PlanStatus, ActionType, PlanStats structs per data model, JSON serialisation/deserialisation, plan naming (auto-generate from source+timestamp)
- [x] T036 {opus} [US1] Implement plan apply logic in `crates/librarian-core/src/plan.rs` — execute PlannedActions (create dirs respecting 3-level max, move files, apply tags via tags.rs, handle collisions with skip+warn), update plan status, write decision log entries, backup support (copy originals to `~/.librarian/backup/<plan-id>/` preserving relative paths when `--backup` flag set), aggressive gate (refuse `--aggressive` unless `--backup` succeeded for the exact same plan)
- [x] T037 {opus} [US1] Implement plan rollback logic in `crates/librarian-core/src/plan.rs` — reverse applied moves, remove applied tags, restore from backup if available, update plan status, write decision log entries
- [x] T038 [P] {sonnet} [US1] Implement junk filename cleaner in `crates/librarian-core/src/plan.rs` — detect and clean patterns: `IMG_NNNN`, `Screenshot YYYY-MM-DD at HH.MM.SS`, `scan_NNNN`, applied during moves even without `--rename`
- [x] T039 [P] {sonnet} [US1] Implement rename logic in `crates/librarian-core/src/plan.rs` — `YYYY-MM-DD_descriptive-slug.ext` format, preserve original name in xattr `com.apple.metadata:LibrarianOriginalName`, only when `--rename` flag set
- [x] T040 {sonnet} [US1] Implement `librarian process` command in `crates/librarian-cli/src/commands/process.rs` — wire scanner, rules engine, plan generation; output progress bars via indicatif; save plan to `~/.librarian/plans/`; respect `--dry-run` (default true), `--source`, `--destination`, `--rules`, `--plan-name`, `--rename` flags
- [x] T041 [P] {sonnet} [US1] Implement `librarian apply` command in `crates/librarian-cli/src/commands/apply.rs` — load plan by name or most recent, execute apply logic from T036, wire `--backup`, `--aggressive`, and `--dry-run` flags per CLI contract
- [x] T042 [P] {sonnet} [US1] Implement `librarian rollback` command in `crates/librarian-cli/src/commands/rollback.rs` — load plan by name or most recent applied, execute rollback
- [x] T043 [P] {sonnet} [US1] Implement `librarian status` command in `crates/librarian-cli/src/commands/status.rs` — list recent plans with status, pending review count, last run info
- [x] T044 [P] {sonnet} [US1] Implement `librarian plans` command in `crates/librarian-cli/src/commands/plans.rs` — list/show/delete subcommands per CLI contract
- [x] T045 [P] {sonnet} [US1] Implement `librarian rules validate` command in `crates/librarian-cli/src/commands/rules.rs` — validate rules file, report errors with line numbers
- [x] T046 {sonnet} [US1] Implement output formatting in `crates/librarian-cli/src/output.rs` — `--verbose` (DEBUG human-readable), `--json` (JSON lines), `--quiet` (errors only), default (INFO + progress bars), mutual exclusion check
- [x] T047 {sonnet} [US1] Implement `librarian init` command in `crates/librarian-cli/src/commands/init.rs` — scaffold `~/.librarian/` directory structure, write default config.yaml, rules.yaml, ignore file per quickstart.md
- [x] T048 [P] {haiku} [US1] Implement `librarian config show` and `librarian config edit` in `crates/librarian-cli/src/commands/config.rs` — YAML dump of resolved config, launch $EDITOR
- [x] T049 [P] {haiku} [US1] Implement lib.rs for librarian-rules in `crates/librarian-rules/src/lib.rs` — re-export Engine, Rule, Loader

**Checkpoint**: User Story 1 fully functional. `librarian init && librarian process --source <test-dir> && librarian apply --plan <name> --backup && librarian rollback` works end to end. Zero AI dependencies.

---

## Phase 4: User Story 2 — AI-Powered Classification with Confidence Gating (Priority: P2)

**Goal**: Files unmatched by rules go through tiered AI pipeline (filename embedding → content embedding → LLM) with confidence gating. Low-confidence files route to NeedsReview.

**Independent Test**: Place files with no matching rules, run `librarian process` with mocked LM Studio, verify high-confidence placement and low-confidence NeedsReview routing with yellow tag and reason note.

### Tests for User Story 2

- [x] T050 [P] {sonnet} [US2] Write unit tests for SSE parser in `crates/librarian-providers/src/sse.rs` — test line parsing, event boundary detection, `[DONE]` signal, malformed data handling
- [x] T051 [P] {sonnet} [US2] Write unit tests for LM Studio client in `crates/librarian-providers/src/lmstudio.rs` — test validate (mock GET /v1/models), chat completion (mock POST), embedding (mock POST), error handling (connection refused, invalid JSON)
- [x] T052 [P] {sonnet} [US2] Write unit tests for OpenAI client in `crates/librarian-providers/src/openai.rs` — test chat, embedding, rate limiting (token bucket), 401/429/500 error handling per provider-api contract
- [x] T053 [P] {sonnet} [US2] Write unit tests for embedding cache in `crates/librarian-providers/src/cache.rs` — test msgpack round-trip, blake3 key lookup, cache miss, cache corruption detection and rebuild
- [x] T054 [P] {opus} [US2] Write unit tests for classification pipeline in `crates/librarian-classifier/src/pipeline.rs` — test tiered escalation (filename→content→LLM), threshold gating, binary file skip (direct to LLM), NeedsReview routing
- [x] T055 [P] {sonnet} [US2] Write unit tests for content extraction in `crates/librarian-classifier/src/content.rs` — test plain text, markdown, CSV reading, PDF text extraction, binary file detection (return None), encrypted PDF fallback
- [x] T056 [P] {sonnet} [US2] Write unit tests for confidence gating in `crates/librarian-classifier/src/confidence.rs` — test threshold comparisons at each tier, configurable thresholds, NeedsReview decision with reason note
- [ ] T057 {opus} [US2] Write integration test for AI classification flow in `tests/integration/needs_review_flow.rs` — mock provider, test full pipeline with files at various confidence levels, verify NeedsReview routing with yellow tag and sidecar reason

### Implementation for User Story 2

- [x] T058 [P] {sonnet} [US2] Implement SSE stream parser in `crates/librarian-providers/src/sse.rs` — line-based parsing, `data:` prefix handling, `[DONE]` detection, delta content accumulation per provider-api contract
- [x] T059 {sonnet} [US2] Implement Provider trait in `crates/librarian-providers/src/traits.rs` — define async trait with validate(), chat(), chat_stream(), embed() methods per provider-api contract
- [x] T060 {sonnet} [US2] Implement LM Studio client in `crates/librarian-providers/src/lmstudio.rs` — reqwest-based, configurable base URL (default localhost:1234), validate via GET /v1/models, chat completion, embedding, streaming via SSE parser
- [x] T061 {opus} [US2] Implement OpenAI client in `crates/librarian-providers/src/openai.rs` — reqwest-based, Bearer auth, rate limiting (token bucket at configurable rpm, default 20), retry on 429 (Retry-After header, once), chat completion, embedding, streaming
- [x] T062 {sonnet} [US2] Implement ProviderRouter in `crates/librarian-providers/src/router.rs` — hold both providers, select active by config or `--provider` flag override, validate active provider at startup
- [x] T063 {sonnet} [US2] Implement embedding cache in `crates/librarian-providers/src/cache.rs` — blake3-keyed HashMap in msgpack at `~/.librarian/cache/embeddings.msgpack`, cache hit/miss, corruption detection (invalid msgpack → clear and rebuild)
- [x] T064 {sonnet} [US2] Implement content extraction in `crates/librarian-classifier/src/content.rs` — text-based only: `read_to_string` for .txt/.md/.csv, `pdf-extract` for PDF text layer, return None for binary files, log extraction failures
- [x] T065 {opus} [US2] Implement embedding + cosine similarity in `crates/librarian-classifier/src/embedding.rs` — embed filenames via provider, embed content for text files, cosine similarity against bucket centroids, cache integration
- [x] T066 {opus} [US2] Implement LLM classifier in `crates/librarian-classifier/src/llm.rs` — construct classification prompt with file metadata and existing buckets, parse structured response (destination, confidence, tags, reason), extract self-reported confidence score
- [x] T067 {opus} [US2] Implement confidence gating in `crates/librarian-classifier/src/confidence.rs` — threshold comparison per tier (filename 0.80, content 0.75, LLM 0.70), configurable via AppConfig.thresholds, produce NeedsReview action with reason note for low confidence
- [x] T068 {opus} [US2] Implement tiered classification pipeline in `crates/librarian-classifier/src/pipeline.rs` — orchestrate: rules first (from US1), then filename embedding, then content embedding (text only, binary skip), then LLM, then confidence gate; produce PlannedAction for each file
- [x] T069 {sonnet} [US2] Implement Qdrant integration in `crates/librarian-classifier/src/qdrant.rs` — collection-per-source-root, upsert embeddings with metadata payload (filename, bucket, filetype), nearest-neighbour search, centroid storage in separate collection
- [x] T070 {sonnet} [US2] Update `librarian process` command in `crates/librarian-cli/src/commands/process.rs` — integrate classification pipeline after rules engine, add `--provider`, `--llm-model`, `--embed-model`, `--threshold` flags, route to NeedsReview for low confidence (yellow tag, `needs-review` tag, sidecar reason note)
- [x] T071 [P] {haiku} [US2] Implement lib.rs for librarian-providers in `crates/librarian-providers/src/lib.rs` — re-export Provider trait, LMStudio, OpenAI, ProviderRouter, Cache
- [x] T072 [P] {haiku} [US2] Implement lib.rs for librarian-classifier in `crates/librarian-classifier/src/lib.rs` — re-export Pipeline, Embedding, LLMClassifier, ConfidenceGate, ContentExtractor, Qdrant

**Checkpoint**: User Story 2 functional. `librarian process` classifies files via rules OR AI, low-confidence files go to NeedsReview with yellow tag and reason. Works with LM Studio and OpenAI.

---

## Phase 5: User Story 3 — Correction-Driven Learning (Priority: P3)

**Goal**: Corrections feed back into classifications via few-shot examples, auto-generated rule proposals, and embedding centroid drift. Per-folder/per-filetype learning isolation.

**Independent Test**: Process-apply cycle, manually move a placed file, run process again on similar file, verify classification reflects correction. Also `librarian correct FILE --to PATH` then `librarian rules suggest` to verify rule proposals.

### Tests for User Story 3

- [x] T073 [P] {sonnet} [US3] Write unit tests for correction recording in `crates/librarian-learning/src/corrections.rs` — test Watched, Explicit, Review correction sources, JSONL append, correction window hard cutoff (14d default), reorganisation logging for post-window moves
- [x] T074 [P] {opus} [US3] Write unit tests for few-shot selection in `crates/librarian-learning/src/fewshot.rs` — test last-N filtering by folder and filetype, prompt formatting with correction examples, isolation (Downloads corrections do not appear for Desktop)
- [x] T075 [P] {sonnet} [US3] Write unit tests for centroid drift in `crates/librarian-learning/src/centroid.rs` — test centroid recalculation after correction, per-folder and per-filetype scoping, weighted update
- [x] T076 [P] {sonnet} [US3] Write unit tests for rule suggestion in `crates/librarian-rules/src/suggestion.rs` — test 3-correction threshold (same source, filetype, target), YAML generation, diff against current rules
- [ ] T077 {opus} [US3] Write integration test for correction feedback loop in `tests/integration/correction_feedback.rs` — full cycle: process, apply, simulate correction (move file), process again with similar file, verify classification reflects correction

### Implementation for User Story 3

- [x] T078 {opus} [US3] Implement correction recording in `crates/librarian-learning/src/corrections.rs` — Correction struct per data model, three sources (Watched, Explicit, Review), JSONL append to both `decisions.jsonl` (full audit log) and `corrections.jsonl` (subset for fast scanning by few-shot selection), correction window check with hard cutoff, `type: reorganisation` logging for post-window moves
- [x] T079 {opus} [US3] Implement few-shot example selection in `crates/librarian-learning/src/fewshot.rs` — scan corrections.jsonl, filter by source_inbox and filetype, select last N (default 20), format as prompt examples per PRD section 10 Layer A, enforce per-folder isolation
- [x] T080 {sonnet} [US3] Implement centroid drift in `crates/librarian-learning/src/centroid.rs` — recalculate bucket centroid when correction recorded, weighted running average, per-folder and per-filetype scoping, persist to `~/.librarian/state/centroids.msgpack`
- [x] T081 {sonnet} [US3] Implement filesystem watcher for corrections in `crates/librarian-learning/src/watcher.rs` — `notify` crate, watch destination directories, detect file hash reappearing at new path within correction window, record Watched correction
- [x] T082 {sonnet} [US3] Implement rule suggestion in `crates/librarian-rules/src/suggestion.rs` — scan corrections for 3x same pattern (source folder + filetype + target), generate Rule YAML entry, diff against current rules.yaml
- [ ] T083 {sonnet} [US3] Update LLM classifier in `crates/librarian-classifier/src/llm.rs` — integrate few-shot injection from librarian-learning, prepend correction examples to classification prompt
- [ ] T084 {sonnet} [US3] Update Qdrant integration in `crates/librarian-classifier/src/qdrant.rs` — on correction, update centroid vectors via centroid.rs, re-index affected bucket
- [x] T085 {sonnet} [US3] Implement `librarian correct` command in `crates/librarian-cli/src/commands/correct.rs` — `--to` and `--retag` flags, record Explicit correction, update centroids
- [x] T086 {sonnet} [US3] Implement `librarian review` command in `crates/librarian-cli/src/commands/review.rs` — one-file-at-a-time interactive review of NeedsReview folder (v1: sequential prompts, not TUI), accept/reject/skip, record Review corrections
- [x] T087 {sonnet} [US3] Implement `librarian rules suggest` command in `crates/librarian-cli/src/commands/rules.rs` — invoke suggestion engine, print proposed YAML entries with diff
- [x] T088 [P] {haiku} [US3] Implement lib.rs for librarian-learning in `crates/librarian-learning/src/lib.rs` — re-export Corrections, FewShot, Centroid, Watcher

**Checkpoint**: User Story 3 functional. Corrections feed back via all three channels. `librarian correct`, `librarian rules suggest`, and `librarian review` all work. Classification improves after corrections.

---

## Phase 6: User Story 4 — Move Limits, Managed Trash, and Audit (Priority: P4)

**Goal**: Move limit enforcement, managed `_Trash/` soft-delete, and decision log audit coverage. Backup and aggressive gate are implemented in US1 (T036).

**Independent Test**: Process with >500 files, verify move limit enforced and partial plan saved. Verify soft-delete to `_Trash/`. Audit decision log for completeness across all operation types.

### Tests for User Story 4

- [x] T089 [P] {sonnet} [US4] Write unit test for move limit in `crates/librarian-core/src/walker.rs` — test that scan stops proposing after max_moves_per_run (default 500), partial plan saved, limit-reached reported in PlanStats
- [x] T090 [P] {sonnet} [US4] Write unit test for managed `_Trash/` in `crates/librarian-core/src/plan.rs` — test soft-delete moves files to `<destination>/_Trash/<plan-id>/`, preserves relative paths, reversible via rollback, tracked in decision log
- [x] T091 [P] {sonnet} [US4] Write unit test for decision log completeness in `crates/librarian-core/src/decision.rs` — verify every operation type (Move, Tag, Skip, Collision, Correction, Reorganisation, Ignored) produces a log entry with all required fields
- [ ] T092 {opus} [US4] Write integration test for safety guarantees in `tests/integration/process_apply_rollback.rs` — full cycle: process >500 files (verify limit), apply with --backup, apply --aggressive (verify gate from T036), soft-delete to _Trash, rollback from backup, verify zero data loss

### Implementation for User Story 4

- [x] T093 {sonnet} [US4] Implement move limit enforcement in `crates/librarian-core/src/walker.rs` — count proposed moves during scan, stop after max_moves_per_run, mark plan as partial, include limit-reached in PlanStats
- [x] T094 {sonnet} [US4] Implement managed `_Trash/` in `crates/librarian-core/src/plan.rs` — soft-delete moves files to `<destination>/_Trash/<plan-id>/` preserving relative paths, reversible via rollback, tracked in decision log
- [x] T095 {sonnet} [US4] Audit decision log coverage across all crates — verify every code path that modifies files, tags, or state writes a Decision entry with all required fields per data model. Add missing log calls if any.

**Checkpoint**: User Story 4 functional. Move limit, managed trash, and audit coverage in place. Combined with US1 backup/aggressive gate, all safety guarantees are complete.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Output quality, documentation, configuration, and final validation

- [ ] T099 {sonnet} Implement progress bar output in `crates/librarian-cli/src/output.rs` — multi-phase progress (scan, classify, plan) via indicatif, integrate with tracing (pause bars during log output), summary table at end per PRD section 13
- [ ] T100 [P] {sonnet} Implement JSON output mode in `crates/librarian-cli/src/output.rs` — JSON lines for all commands, machine-parseable, no progress bars in JSON mode
- [ ] T101 [P] {haiku} Add British English strings throughout all user-facing output — audit all crates for American English spellings (organize→organise, color→colour, behavior→behaviour, etc.)
- [ ] T102 [P] {sonnet} Add comprehensive error messages with context in all crates — every error should include: what failed, why, what to do about it, per Observability principle
- [ ] T103 [P] {haiku} Write example `config.yaml` with all options commented at `examples/config.yaml`
- [ ] T104 [P] {haiku} Write example `rules.yaml` with common patterns at `examples/rules.yaml`
- [ ] T105 {sonnet} Validate quickstart.md checklist end-to-end — run through every step in `specs/001-rust-file-organiser-cli/quickstart.md`, fix any issues
- [ ] T109 {sonnet} Add performance benchmark in `tests/integration/benchmarks.rs` — generate 1,000 fixture files, run full process pipeline, assert wall-clock < 180s (SC-001); run apply+rollback on 500-file plan, assert rollback < 30s (SC-007)
- [ ] T106 {sonnet} Run `cargo clippy -- -D warnings` and fix all warnings across workspace
- [ ] T107 {sonnet} Run `cargo test` full suite and verify all tests pass, fix any failures
- [ ] T108 [P] {haiku} Verify zero `unsafe` blocks outside xattr — audit all crates, document any `unsafe` with comments explaining why it's needed

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion — BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational — no other story dependencies
- **User Story 2 (Phase 4)**: Depends on Foundational + US1 (rules engine integration, plan model)
- **User Story 3 (Phase 5)**: Depends on US2 (classification pipeline for few-shot and centroid integration)
- **User Story 4 (Phase 6)**: Depends on US1 (plan model) — can run in parallel with US2/US3 for safety-only tasks
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) — No dependencies on other stories. **This is the MVP.**
- **User Story 2 (P2)**: Depends on US1 plan model and rules engine. Can start after US1 Phase 3 completes.
- **User Story 3 (P3)**: Depends on US2 classification pipeline. Can start after US2 Phase 4 completes.
- **User Story 4 (P4)**: Move limit and managed trash can start after US1. Audit task (T095) requires all stories complete. Backup/aggressive gate already in US1 (T036).

### Within Each User Story

- Tests MUST be written and FAIL before implementation (constitution principle II)
- Data model structs before service logic
- Service logic before CLI command wiring
- Core implementation before integration points
- Story complete before moving to next priority

### Parallel Opportunities

**Phase 1**: T002–T011 all run in parallel (independent Cargo.toml and fixture files)
**Phase 2**: T013–T019 (tests) all parallel; T020–T027 (implementation) mostly parallel except T024 depends on T023
**Phase 3**: T028–T032 (tests) all parallel; T033–T049 (impl) — T033+T038+T039+T043+T044+T045+T048+T049 parallel, then T034→T035→T036→T037→T040 sequential
**Phase 4**: T050–T057 (tests) all parallel; T058–T072 (impl) — T058+T064+T069+T071+T072 parallel, then T059→T060→T061→T062→T063 sequential, then T065→T066→T067→T068→T070
**Phase 5**: T073–T077 (tests) all parallel; T078–T088 (impl) — T078+T080+T081+T082+T088 parallel, then T079→T083→T084 sequential
**Phase 6**: T089–T091 (tests) all parallel; T092 (integration); T093–T095 mostly parallel
**Phase 7**: T099+T100+T101+T102+T103+T104+T108 parallel, then T105→T109→T106→T107 sequential

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL — blocks all stories)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: Test US1 independently — rules-only file organisation works end to end
5. Deploy/use immediately (no AI needed)

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. Add US1 → Test independently → **MVP! Usable rules-based organiser**
3. Add US2 → Test independently → AI classification with confidence gating
4. Add US3 → Test independently → Learning from corrections
5. Add US4 → Test independently → Full safety guarantees
6. Polish → Production-quality output, docs, validation

### Model Cost Optimisation

For budget-conscious execution, the model breakdown (106 tasks total):
- **{haiku}** tasks (19 tasks): ~$low — boilerplate, config, re-exports
- **{sonnet}** tasks (63 tasks): ~$moderate — standard implementation with clear patterns
- **{opus}** tasks (24 tasks): ~$higher — architectural decisions, multi-concern orchestration, complex algorithms

Run {haiku} tasks first to scaffold structure, then {sonnet} for bulk implementation, reserving {opus} for critical architectural tasks.

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- {opus}/{sonnet}/{haiku} indicates recommended Claude model
- Each user story should be independently completable and testable
- Verify tests fail before implementing (constitution principle II)
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
