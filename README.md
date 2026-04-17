# Librarian

A command-line tool that organises your files using rules and AI. Point it at messy folders like `~/Downloads`, and it classifies files into a tidy hierarchy — learning from your corrections over time.

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
# Scaffold config and folder structure
librarian init

# Scan inbox folders and produce a plan
librarian process --source ~/Downloads

# Review what it wants to do
librarian plans show latest

# Apply the plan (with backup)
librarian apply --plan latest --backup

# Changed your mind? Roll it back
librarian rollback --plan latest
```

## How it works

Librarian uses a **tiered classification pipeline** — each tier either accepts (meets confidence threshold) or escalates to the next:

1. **Rules** — deterministic glob/regex patterns you define in `rules.yaml` (confidence: 1.0)
2. **Filename embeddings** — cosine similarity against known folder centroids (threshold: 0.80)
3. **Content embeddings** — for text and PDF files (threshold: 0.75)
4. **LLM classifier** — structured prompt with few-shot examples (threshold: 0.70)
5. **Needs review** — if nothing is confident enough, it flags the file for you

When you correct a classification, Librarian remembers — it updates its embedding centroids, feeds corrections into future LLM prompts as few-shot examples, and can even suggest new rules when it spots repeated patterns.

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

Define classification rules in `~/.librarian/rules.yaml`:

```yaml
rules:
  - name: "PDF Invoices"
    match:
      extension: "pdf"
      filename: "*invoice*"
    destination: "{year}/{month}/Work/Invoices"
    tags: [invoice, finance]
```

## Commands

| Command | Description |
|---------|-------------|
| `init` | Scaffold configuration and folder structure |
| `process` | Scan inbox folders, classify files, produce a plan |
| `apply` | Execute a previously generated plan |
| `rollback` | Reverse an applied plan |
| `status` | List plans, recent runs, pending reviews |
| `plans` | Inspect or delete named plans |
| `rules` | Validate rules or suggest new ones from corrections |
| `correct` | Record an explicit correction |
| `review` | Interactive review of flagged files |
| `config` | Show or edit configuration |

## Providers

Librarian works with any OpenAI-compatible API:

- **[LM Studio](https://lmstudio.ai)** — local models, no API key needed (default)
- **OpenAI** — set `provider_type: openai` and provide an `api_key`

## Licence

MIT
