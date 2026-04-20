# Changelog

All notable changes to this project will be documented in this file.

## [0.2.1] - 2026-04-20

### Fixed
- `librarian correct` now works with directories (previously failed with "Is a directory" error)

## [0.2.0] - 2026-04-19

### Added
- MCP server for LLM agent integration (Claude Code, Claude Desktop)
- `.mcp.json` for project-level MCP server config

### Changed
- Updated README and AGENTS.md for recent changes

## [0.1.11] - 2026-04-19

### Fixed
- `plans list` subcommand added
- Plan name resolution fixed

## [0.1.10] - 2026-04-18

### Changed
- Version bump (internal)

## [0.1.9] - 2026-04-18

### Changed
- Version bump (internal)

## [0.1.8] - 2026-04-18

### Changed
- Migrated OpenAI provider to Responses API (`/v1/responses`)

## [0.1.7] - 2026-04-17

### Fixed
- Always use `max_completion_tokens` for OpenAI provider

## [0.1.6] - 2026-04-17

### Fixed
- Split `$EDITOR` into program and args for `config edit`

## [0.1.5] - 2026-04-17

### Fixed
- Cargo fmt CI failures resolved

## [0.1.4] - 2026-04-16

### Added
- Hierarchical classification with taxonomy config
- `--take` flag for `process` command
- Folders treated as atomic units during classification

### Fixed
- Rename collision bug

## [0.1.3] - 2026-04-15

### Added
- Treat folders as atomic units during classification

### Fixed
- Force-push bump branch to handle release reruns

## [0.1.2] - 2026-04-14

### Added
- `uninstall` command (removes completions, daemons, temp files, binary)
- Self-update via `update` / `upgrade` commands
- `-v` / `--version` flag
- `{ai_suggest}` template for rules
- GPL-3.0 licence

### Fixed
- Release pipeline for branch protection
- Use release artifacts for self-update
- Version sync

## [0.1.1] - 2026-04-13

### Added
- Comprehensive test coverage (344 to 504 tests)
- AI classification pipeline wired up
- `suggest-structure` command
- CI, integration tests, provider resilience

## [0.1.0] - 2026-04-12

### Added
- Initial release
- Rules engine with glob/regex patterns
- Filename and content embedding classification
- LLM classification with few-shot examples
- Plan-based file moves with apply/rollback
- Correction-based learning
- Progress bars and CLI polish
- OpenAI and LM Studio providers
