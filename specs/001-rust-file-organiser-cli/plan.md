# Implementation Plan: Librarian — File Organisation CLI

**Branch**: `001-rust-file-organiser-cli` | **Date**: 2026-04-17 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/001-rust-file-organiser-cli/spec.md`

## Summary

Librarian is a pure Rust CLI that organises files on macOS (with cross-platform fallback) using a deterministic rules engine and a tiered AI classification pipeline (LM Studio default, OpenAI opt-in). It proposes file moves via reviewable plans, applies Finder tags/colour labels, and learns from user corrections over time. The implementation follows a Cargo workspace with independent crates per domain, milestone-ordered from skeleton (M1) through learning layer (M5) to polish (M6).

## Technical Context

**Language/Version**: Rust (stable, latest edition 2024)
**Primary Dependencies**: clap (CLI parsing), serde + serde_json + serde_yaml (serialisation), blake3 (hashing), globset (glob matching), regex (opt-in patterns), reqwest (HTTP client for providers), qdrant-client (vector DB), rmp-serde (msgpack), indicatif (progress bars), tracing + tracing-subscriber (structured logging), notify (filesystem watching for corrections), pdf-extract (PDF text layer), xattr (macOS extended attributes)
**Storage**: YAML (config, rules), JSON (plans), JSONL (decision log, corrections), msgpack (embedding cache, centroids), Qdrant (vector storage via Docker)
**Testing**: `cargo test` + mockito (HTTP mocking) + tempfile (temp directories) + assert_cmd (CLI integration tests)
**Target Platform**: macOS primary (xattr support), Linux/Windows fallback (sidecar `.librarian-meta.json`)
**Project Type**: CLI (Cargo workspace with library crates)
**Performance Goals**: Scan+hash 10k files < 3s, rules engine 10k files < 300ms, embed 1k filenames < 60s, full process 1k files < 3min
**Constraints**: Zero compiler warnings, `unsafe` only for xattr and libloading (commented), Tokio only async runtime, British English strings, max 3-level folder depth
**Scale/Scope**: Single user, personal machine, up to ~50k managed files

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Evidence |
|-----------|--------|----------|
| I. Modular-First | PASS | Cargo workspace with 6 independent crates: `librarian-core`, `librarian-rules`, `librarian-providers`, `librarian-classifier`, `librarian-learning`, `librarian-cli`. Each crate is independently testable with a clear single purpose. |
| II. Test-First | PASS | Each milestone begins with test definitions. Unit tests per crate, integration tests for cross-crate flows, contract tests for provider HTTP shapes. TDD cycle enforced per constitution. |
| III. Simplicity & YAGNI | PASS | Milestones are ordered by dependency — M1 builds skeleton with zero AI, M2 adds rules only, M3 adds providers, etc. No speculative features. v2/v3 scope explicitly excluded. |
| IV. Content Integrity | PASS | Append-only decision log, never-overwrite collision policy, backup-before-aggressive gate, managed `_Trash/` for soft-deletes, rollback for all plans. |
| V. Observability | PASS | `tracing` crate for structured logging, `--verbose`/`--json`/`--quiet` output modes, decision log with full context per action. |

**Note on Technology Standards**: The constitution's Technology Standards section references Bun/TypeScript which was written before the Librarian PRD specified pure Rust. The constitution's core principles (I–V) all apply. The Technology Standards section should be amended via `/speckit-constitution` to reflect the Rust stack for this project.

## Project Structure

### Documentation (this feature)

```text
specs/001-rust-file-organiser-cli/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (CLI command schemas)
└── tasks.md             # Phase 2 output (/speckit-tasks)
```

### Source Code (repository root)

```text
Cargo.toml                          # Workspace root
crates/
├── librarian-core/                 # Shared types, config, file walker, hashing, plan model, ignore engine
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── config.rs               # Config loading (config.yaml)
│       ├── file_entry.rs           # File type with path, hash, metadata
│       ├── hasher.rs               # blake3 hashing
│       ├── ignore.rs               # .librarianignore, global ignore, system defaults
│       ├── plan.rs                 # Plan data model, serialisation, apply, rollback
│       ├── decision.rs             # Decision log types, JSONL append
│       ├── tags.rs                 # Finder tags + colour labels (xattr / sidecar)
│       └── walker.rs               # Directory scanner with ignore integration
│
├── librarian-rules/                # Deterministic rules engine
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── engine.rs               # Rule matching (glob default, regex opt-in)
│       ├── loader.rs               # rules.yaml parsing and validation
│       └── suggestion.rs           # Auto-generated rule proposals from corrections
│
├── librarian-providers/            # AI provider abstraction
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── traits.rs               # Provider trait (chat, embed)
│       ├── lmstudio.rs             # LM Studio client
│       ├── openai.rs               # OpenAI client with rate limiting
│       ├── router.rs               # ProviderRouter (provider selection)
│       ├── sse.rs                  # SSE stream parser (shared)
│       └── cache.rs                # Embedding cache (msgpack, blake3-keyed)
│
├── librarian-classifier/           # AI classification pipeline
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── pipeline.rs             # Tiered classification orchestrator
│       ├── embedding.rs            # Filename + content embedding, cosine sim
│       ├── llm.rs                  # LLM classifier with few-shot injection
│       ├── confidence.rs           # Confidence gating logic
│       ├── content.rs              # Text extraction (plain text, PDF, markdown, CSV)
│       └── qdrant.rs               # Qdrant client wrapper, bucket centroids
│
├── librarian-learning/             # Correction tracking and learning
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── corrections.rs          # Correction recording (watched, explicit, review)
│       ├── fewshot.rs              # Few-shot example selection (last N, filtered)
│       ├── centroid.rs             # Centroid drift calculation
│       └── watcher.rs              # File-system watcher for correction detection
│
└── librarian-cli/                  # CLI entry point (clap)
    ├── Cargo.toml
    └── src/
        ├── main.rs
        ├── commands/
        │   ├── mod.rs
        │   ├── init.rs             # librarian init
        │   ├── process.rs          # librarian process
        │   ├── apply.rs            # librarian apply
        │   ├── rollback.rs         # librarian rollback
        │   ├── status.rs           # librarian status
        │   ├── plans.rs            # librarian plans (list/show/delete)
        │   ├── rules.rs            # librarian rules (validate/suggest)
        │   ├── correct.rs          # librarian correct
        │   ├── review.rs           # librarian review
        │   └── config.rs           # librarian config (show/edit)
        └── output.rs               # Output formatting (text/json/quiet/verbose)

tests/
├── integration/
│   ├── process_apply_rollback.rs   # Full plan lifecycle
│   ├── rules_matching.rs           # Rules engine against real files
│   ├── correction_feedback.rs      # Correction loop end-to-end
│   └── needs_review_flow.rs        # Low-confidence routing
└── fixtures/
    ├── sample_rules.yaml
    ├── sample_config.yaml
    └── sample_files/               # Test file fixtures
```

**Structure Decision**: Cargo workspace with 6 crates. Each crate maps to a domain boundary from the spec: core infrastructure, rules engine, AI providers, classification pipeline, learning layer, and CLI. This aligns with the Modular-First principle and the milestone ordering (M1 uses core+cli, M2 adds rules, M3 adds providers, M4 adds classifier, M5 adds learning).

## Complexity Tracking

> No Constitution Check violations to justify.

| Aspect | Decision | Rationale |
|--------|----------|-----------|
| 6 crates in workspace | Justified | Maps 1:1 to domain boundaries and milestones. Each crate is independently compilable and testable. No crate exists for organisational grouping alone. |
| Qdrant external dependency | Justified | Vector similarity search at the scale of 10k+ embeddings needs an indexed store. In-memory cosine sim would not meet the <60s embedding performance target. Qdrant runs locally in Docker, consistent with local-first principle. |
