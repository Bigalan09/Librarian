# Quickstart: Librarian

## Prerequisites

- Rust stable toolchain (`rustup default stable`)
- Docker (for Qdrant, needed for AI classification only)
- LM Studio running locally (optional, for AI classification)

## Build

```bash
# Clone and build
cd Librarian
cargo build --release

# Binary at target/release/librarian
```

## First Run

```bash
# 1. Scaffold configuration
librarian init

# 2. Edit config to set your inbox folders and destination
librarian config edit

# 3. Add some rules (or use the defaults)
# Edit ~/.librarian/rules.yaml

# 4. Validate your rules
librarian rules validate

# 5. Scan and classify (dry-run by default)
librarian process --source ~/Downloads

# 6. Review the plan
librarian plans show downloads-2026-04-17-1423

# 7. Apply with backup (safe)
librarian apply --plan downloads-2026-04-17-1423 --backup

# 8. Check status
librarian status
```

## With AI Classification

```bash
# 1. Start Qdrant
docker run -p 6333:6333 -p 6334:6334 qdrant/qdrant

# 2. Start LM Studio and load a model
# (LM Studio GUI → load model → start server on port 1234)

# 3. Process with AI
librarian process --source ~/Downloads --provider lmstudio

# 4. Review low-confidence files
librarian review
```

## Correcting Mistakes

```bash
# Explicit correction
librarian correct invoice.pdf --to ~/Library-Managed/2026/Work/Invoices/

# After several corrections, check for suggested rules
librarian rules suggest
```

## Rolling Back

```bash
# Undo the most recent applied plan
librarian rollback

# Or rollback a specific plan
librarian rollback --plan downloads-2026-04-17-1423
```

## Verification Checklist

- [ ] `librarian init` creates `~/.librarian/` with config.yaml, rules.yaml, ignore
- [ ] `librarian rules validate` reports no errors on default rules
- [ ] `librarian process --source <test-dir>` generates a plan without errors
- [ ] `librarian apply --plan <name> --backup` moves files to correct destinations
- [ ] `librarian rollback --plan <name>` restores all files to original locations
- [ ] Finder tags are visible in macOS Finder after apply
- [ ] Decision log at `~/.librarian/history/decisions.jsonl` contains entries
- [ ] `librarian status` shows the plan and its state
- [ ] `--json` flag produces valid JSON lines output
- [ ] `--quiet` flag suppresses all non-error output
