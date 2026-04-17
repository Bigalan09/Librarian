# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo test                    # Run all tests
cargo test -p librarian-core  # Test a single crate
cargo test plan::tests        # Run tests matching a path
cargo clippy                  # Lint
cargo build --release         # Release build
```

## Architecture

Librarian is a Rust CLI that classifies and organises files from inbox folders (e.g. ~/Downloads) into a managed directory hierarchy using a tiered AI pipeline. Rust edition 2024, workspace of six crates.

### Crate dependency graph

```
librarian-cli  (orchestration, clap commands)
  ├── librarian-classifier  (tiered classification pipeline)
  │     ├── librarian-rules      (deterministic glob/regex rule engine)
  │     ├── librarian-providers  (AI provider abstraction: LM Studio, OpenAI)
  │     └── librarian-core
  ├── librarian-learning   (correction tracking, centroid drift, few-shot selection)
  │     └── librarian-core
  └── librarian-core       (config, types, file walker, hashing, plan, ignore, decision log)
```

### Data flow

1. **Process**: scan inbox folders → hash files (blake3) → classify via tiered pipeline → emit a Plan (JSON)
2. **Apply**: load Plan → optionally backup → execute moves/renames/tags → log decisions (JSONL)
3. **Rollback**: restore from backup or reverse moves in reverse order → log as corrections
4. **Correct**: user moves file manually (watched via notify) or explicitly → record correction → update centroids → feed into few-shot examples

### Classification pipeline (waterfall cascade)

Each tier either accepts (meets confidence threshold) or escalates to the next:

1. **Rule Engine** (confidence=1.0) - glob/regex pattern matching from rules.yaml
2. **Filename Embedding** - cosine similarity vs bucket centroids (threshold: 0.80)
3. **Content Embedding** - for text/PDF files, 8000 char truncation (threshold: 0.75)
4. **LLM Classifier** - structured JSON response with few-shot examples (threshold: 0.70)
5. **NeedsReview** - if all tiers below threshold, flagged for human review

Gate result is `Accept { destination, confidence }`, `Escalate`, or `NeedsReview { reason }`.

### Key types

- **AppConfig** (YAML `~/.librarian/config.yaml`): inbox_folders, destination_root, provider settings, thresholds
- **FileEntry**: path, blake3 hash, extension, size, timestamps, macOS Finder tags/colour (xattr)
- **Plan**: id (timestamp-based), status (Draft -> Applied -> RolledBack), Vec\<PlannedAction\>
- **PlannedAction**: action_type (Move/Rename/Tag/Skip/NeedsReview/Collision/Ignored), classification_method, confidence
- **Decision**: append-only JSONL audit log entry with outcome, hash, provider/model, confidence

### Learning system

- **Few-shot injection**: recent corrections (scoped per source_inbox + filetype) injected into LLM system prompt
- **Centroid drift**: running-average embeddings updated on correction, scoped by (source_inbox, filetype, bucket)
- **Rule suggestion**: mines corrections.jsonl for repeated patterns (≥3 occurrences) and proposes rules.yaml entries

### Non-obvious patterns

- Blake3 hash is the file identity key across plans, corrections, and dedup
- Decision logging is append-only JSONL (atomic single writes, no locking)
- Embedding cache is in-memory HashMap persisted as msgpack between runs
- Template variables in rule destinations: `{year}`, `{month}`, `{date}`, `{ext}`, `{source}`
- Plan status is a one-way state machine: Draft -> Applied -> RolledBack
- Learning is isolated per (source_inbox, filetype) tuple to prevent cross-folder data leakage
- `--aggressive` flag on apply requires `--backup` to have been used (safety gate)

## Error handling

- `anyhow::Result<T>` for recoverable errors with `.context()` enrichment
- `thiserror::Error` for domain-specific errors in librarian-rules
- `tracing` for structured logging throughout

## Config files

- `~/.librarian/config.yaml` - main config (provider, thresholds, inbox folders)
- `~/.librarian/rules.yaml` - deterministic classification rules (glob, regex, extension, size, content matchers)
- `~/.librarian/history/decisions.jsonl` - audit log
- `~/.librarian/history/corrections.jsonl` - learning records
- `~/.librarian/cache/embeddings.msgpack` - embedding cache
