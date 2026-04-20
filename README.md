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

```sh
librarian config edit   # opens in $EDITOR (defaults to vi)
```

Example configuration:

```yaml
inbox_folders:
  - ~/Downloads
  - ~/Desktop
destination_root: ~/Library-Managed

provider:
  provider_type: openai
  api_key: "sk-..."
  llm_model: "gpt-4o-mini"    # any OpenAI model

thresholds:
  filename_embedding: 0.80
  content_embedding: 0.75
  llm_confidence: 0.70
```

For local models via LM Studio:

```yaml
provider:
  provider_type: lmstudio
  base_url: http://localhost:1234/v1
  llm_model: "your-model-name"
```

## Commands

| Command | What it does |
|---------|-------------|
| `init` | Scaffold config and folder structure |
| `process --source <paths>` | Scan folders, classify files, produce a plan |
| `process --take <N>` | Only process the first N files (useful for testing) |
| `apply --plan <name> [--backup]` | Execute a plan |
| `rollback --plan <name>` | Reverse an applied plan |
| `status` | Show plans, recent runs, pending reviews |
| `plans list` | List all saved plans |
| `plans show <name>` | Inspect a plan (accepts ID, name, or `latest`) |
| `plans delete <name>` | Delete a plan |
| `plans clean --days 30` | Remove plans older than N days |
| `rules validate` | Check your rules.yaml for errors |
| `rules suggest` | Suggest new rules from correction history |
| `suggest-structure` | Suggest a folder structure and rules using AI |
| `correct <file> --to <path>` | Record a manual correction |
| `watch` | Watch destination for manual corrections (passive learning) |
| `review` | Walk through files that need human review |
| `config show` | Print current config |
| `config edit` | Open config in `$EDITOR` |
| `update` / `upgrade` | Check for and install updates from GitHub |
| `update --check` | Check for updates without installing |
| `uninstall` | Remove Librarian and all its files from the system |
| `completions <shell>` | Generate shell completions (bash, zsh, fish) |
| `-v` / `--version` | Print version |

## Uninstall

```sh
librarian uninstall
```

This removes everything Librarian has put on your system: the `~/.librarian` data directory (config, rules, history, cache, plans, backups), shell completions, any launchd agents or systemd units, and the binary itself. You'll be shown exactly what will be deleted and asked to confirm before anything is removed.

To skip the confirmation prompt:

```sh
librarian uninstall --yes
```

## MCP Server (for LLM agents)

Librarian ships an [MCP](https://modelcontextprotocol.io) server so LLM agents like Claude can manage your files through natural conversation.

### Install

```sh
cd mcp-server
bun install
```

### Add to Claude Code

In your project or user settings (`~/.claude/settings.json`):

```json
{
  "mcpServers": {
    "librarian": {
      "command": "bun",
      "args": ["run", "/path/to/Librarian/mcp-server/index.ts"]
    }
  }
}
```

### Add to Claude Desktop

In `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "librarian": {
      "command": "bun",
      "args": ["run", "/path/to/Librarian/mcp-server/index.ts"]
    }
  }
}
```

### Available tools

| Tool | Description |
|------|-------------|
| `librarian_status` | Show status: plans, reviews, config |
| `librarian_process` | Scan and classify files, produce a plan |
| `librarian_plans_list` | List all saved plans |
| `librarian_plans_show` | Show plan details |
| `librarian_apply` | Execute a plan (move files) |
| `librarian_rollback` | Reverse an applied plan |
| `librarian_correct` | Record a correction (files or folders) |
| `librarian_rules_validate` | Check rules.yaml for errors |
| `librarian_rules_suggest` | Suggest rules from correction history |
| `librarian_config_show` | Show current configuration |
| `librarian_plans_delete` | Delete a plan |
| `librarian_plans_clean` | Remove old plans |
| `librarian_suggest_structure` | AI-suggested folder structure |

See [`mcp-server/README.md`](mcp-server/README.md) for more details and example conversations.

## Providers

- **OpenAI** - uses the [Responses API](https://platform.openai.com/docs/api-reference/responses) (`/v1/responses`). Set `provider_type: openai` with an `api_key`
- **[LM Studio](https://lmstudio.ai)** - run models locally via OpenAI-compatible Chat Completions API, no API key needed. Set `provider_type: lmstudio`

## Licence

GPL-3.0 -- see [LICENSE](LICENSE) for details.
