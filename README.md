# Librarian

Tired of your Downloads folder being a graveyard of `IMG_4382.jpg`, `invoice_final_v3.pdf`, and that screenshot from six months ago? Librarian sorts it out for you.

It scans folders like `~/Downloads` and `~/Desktop`, figures out where each file should go using a mix of rules you define and AI classification, then moves everything into a clean folder structure. When it gets something wrong, you correct it, and it learns from the mistake.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/Bigalan09/Librarian/main/scripts/install.sh | sh
```

Or build from source:

```sh
cargo install --git https://github.com/Bigalan09/Librarian.git librarian-cli
```

## Quick start

```sh
librarian init
librarian process --source ~/Downloads
librarian plans show latest
librarian apply --plan latest --backup
```

## Use cases

### Taming ~/Downloads

You've got 200+ files in Downloads. PDFs from work, memes, app installers, bank statements, photos. Run `librarian process` and it'll sort them into folders like `2026/04/Work/Invoices/`, `Personal/Photos/`, `Software/Installers/`.

```sh
librarian process --source ~/Downloads
librarian plans show latest    # check it looks right
librarian apply --plan latest --backup
```

Don't like where something ended up? Move it yourself and Librarian will remember next time:

```sh
librarian correct ~/Library-Managed/Work/report.pdf --to ~/Library-Managed/Personal/report.pdf
```

### Writing rules for predictable files

Some files always go to the same place. Bank statements are always PDFs with "statement" in the name. Invoices follow a pattern. Write rules in `~/.librarian/rules.yaml` and they'll match instantly without touching the AI:

```yaml
rules:
  - name: "Bank statements"
    match:
      extension: "pdf"
      filename: "*statement*"
    destination: "{year}/{month}/Finance/Statements"
    tags: [finance, bank]

  - name: "Screenshots"
    match:
      filename: "Screenshot*"
    destination: "{year}/{month}/Screenshots"
```

If you want the AI to decide where a file goes but still apply tags and colours from a rule, use `{ai_suggest}` as the destination:

```yaml
rules:
  - name: "PDFs"
    match:
      extension: "pdf"
    destination: "{ai_suggest}"
    tags: [document, pdf]
    colour: green
```

The rule still matches (tagging and colouring the file), but the folder placement is delegated to the AI classification pipeline.

Validate your rules are correct:

```sh
librarian rules validate
```

After a few corrections, Librarian can suggest rules for you:

```sh
librarian rules suggest
```

### Keeping Desktop clean on a schedule

Point Librarian at your Desktop as well and run it periodically:

```sh
librarian process --source ~/Desktop ~/Downloads
librarian apply --plan latest
```

### Reviewing uncertain files

When Librarian isn't confident enough to classify something, it flags it for review instead of guessing:

```sh
librarian review
```

This walks you through each flagged file interactively so you can decide where it goes.

### Undoing a bad run

Applied a plan and it made a mess? Roll it back:

```sh
librarian rollback --plan latest
```

If you used `--backup` when applying, the original files are restored from the backup. Otherwise it reverses the moves.

## How it works

Librarian classifies files through four tiers, stopping as soon as one is confident enough:

1. **Rules** - your glob/regex patterns from `rules.yaml` (always confident, instant)
2. **Filename embeddings** - compares the filename against known folder centroids (threshold: 0.80)
3. **Content embeddings** - reads text/PDF content and compares (threshold: 0.75)
4. **LLM** - asks a language model with few-shot examples from your past corrections (threshold: 0.70)

If nothing passes, the file gets flagged as "needs review" rather than being moved somewhere wrong.

The learning bit: when you correct a file, Librarian shifts its embedding centroids towards the right answer and injects the correction as a few-shot example for the LLM. Corrections are scoped per folder and file type, so fixing a PDF in Downloads won't affect how it handles PNGs from Desktop.

## Configuration

After `librarian init`, edit `~/.librarian/config.yaml`:

```yaml
inbox_folders:
  - ~/Downloads
  - ~/Desktop
destination_root: ~/Library-Managed

provider:
  provider_type: lmstudio    # or openai
  base_url: http://localhost:1234/v1

thresholds:
  filename_embedding: 0.80
  content_embedding: 0.75
  llm_confidence: 0.70
```

## Commands

| Command | What it does |
|---------|-------------|
| `init` | Scaffold config and folder structure |
| `process --source <paths>` | Scan folders, classify files, produce a plan |
| `apply --plan <name> [--backup]` | Execute a plan |
| `rollback --plan <name>` | Reverse an applied plan |
| `status` | Show plans, recent runs, pending reviews |
| `plans show <name>` | Inspect a plan |
| `plans delete <name>` | Delete a plan |
| `plans clean --days 30` | Remove plans older than N days |
| `rules validate` | Check your rules.yaml for errors |
| `rules suggest` | Suggest new rules from correction history |
| `correct <file> --to <path>` | Record a manual correction |
| `watch` | Watch destination for manual corrections (passive learning) |
| `review` | Walk through files that need human review |
| `config show` | Print current config |
| `update` / `upgrade` | Check for and install updates from GitHub |
| `update --check` | Check for updates without installing |
| `uninstall` | Remove Librarian, its config, cache, and data |
| `completions <shell>` | Generate shell completions (bash, zsh, fish) |
| `-v` / `--version` | Print version |

## Providers

Librarian works with any OpenAI-compatible API:

- **[LM Studio](https://lmstudio.ai)** - run models locally, no API key needed (default)
- **OpenAI** - set `provider_type: openai` and provide an `api_key`

## Licence

GPL-3.0 -- see [LICENSE](LICENSE) for details.
