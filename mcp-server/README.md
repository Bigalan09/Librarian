# Librarian MCP Server

An [MCP](https://modelcontextprotocol.io) server that exposes Librarian commands as tools for LLM agents like Claude.

## Prerequisites

- [Bun](https://bun.sh) installed
- `librarian` CLI installed and on your `$PATH`
- `~/.librarian/config.yaml` configured (run `librarian init`)

## Setup

```sh
cd mcp-server
bun install
```

## Usage with Claude Code

Add to your Claude Code MCP settings (`~/.claude/settings.json`):

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

## Usage with Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

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

## Available tools

| Tool | Description |
|------|-------------|
| `librarian_status` | Show status: plans, reviews, config |
| `librarian_process` | Scan and classify files, produce a plan |
| `librarian_plans_list` | List all saved plans |
| `librarian_plans_show` | Show plan details |
| `librarian_apply` | Execute a plan (move files) |
| `librarian_rollback` | Reverse an applied plan |
| `librarian_correct` | Record a correction for learning |
| `librarian_rules_validate` | Check rules.yaml for errors |
| `librarian_rules_suggest` | Suggest rules from correction history |
| `librarian_config_show` | Show current configuration |
| `librarian_plans_delete` | Delete a plan |
| `librarian_plans_clean` | Remove old plans |
| `librarian_suggest_structure` | AI-suggested folder structure |

## Example conversation

> "Scan my Downloads folder and show me what Librarian would do"

The agent calls `librarian_process` with `source: ["~/Downloads"]`, then `librarian_plans_show` with `name: "latest"` to display the results.

> "That looks good, apply it with a backup"

The agent calls `librarian_apply` with `plan: "latest"` and `backup: true`.
