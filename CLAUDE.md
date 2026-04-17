# Librarian Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-04-17

## Active Technologies

- Rust (stable, latest edition 2024) + clap (CLI parsing), serde + serde_json + serde_yaml (serialisation), blake3 (hashing), globset (glob matching), regex (opt-in patterns), reqwest (HTTP client for providers), qdrant-client (vector DB), rmp-serde (msgpack), indicatif (progress bars), tracing + tracing-subscriber (structured logging), notify (filesystem watching for corrections), pdf-extract (PDF text layer), xattr (macOS extended attributes) (001-rust-file-organiser-cli)

## Project Structure

```text
Cargo.toml                   # Workspace root
crates/
├── librarian-core/          # Shared types, config, file walker, hashing, plan, ignore
├── librarian-rules/         # Deterministic rules engine (glob + regex)
├── librarian-providers/     # AI provider abstraction (LM Studio, OpenAI)
├── librarian-classifier/    # Tiered AI classification pipeline
├── librarian-learning/      # Correction tracking and learning layer
└── librarian-cli/           # CLI entry point (clap)
tests/
├── integration/             # Cross-crate integration tests
└── fixtures/                # Test data
```

## Commands

```bash
cargo test                    # Run all tests
cargo clippy                  # Lint
cargo build --release         # Release build
cargo test -p librarian-core  # Test a single crate
```

## Code Style

Rust (stable, latest edition 2024): Follow standard conventions

## Recent Changes

- 001-rust-file-organiser-cli: Added Rust (stable, latest edition 2024) + clap (CLI parsing), serde + serde_json + serde_yaml (serialisation), blake3 (hashing), globset (glob matching), regex (opt-in patterns), reqwest (HTTP client for providers), qdrant-client (vector DB), rmp-serde (msgpack), indicatif (progress bars), tracing + tracing-subscriber (structured logging), notify (filesystem watching for corrections), pdf-extract (PDF text layer), xattr (macOS extended attributes)

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
