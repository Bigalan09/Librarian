# Research: Librarian — File Organisation CLI

**Date**: 2026-04-17
**Feature**: [spec.md](spec.md) | [plan.md](plan.md)

## R1: Rust CLI Framework

**Decision**: `clap` v4 with derive macros for command parsing.

**Rationale**: clap is the de facto standard for Rust CLIs. Derive macros reduce boilerplate. Subcommand support maps directly to Librarian's command structure (`process`, `apply`, `rollback`, etc.). Built-in help generation, shell completion, and argument validation.

**Alternatives considered**:
- `argh` — lighter but less feature-rich, no shell completions.
- `structopt` — merged into clap v3+, no longer maintained separately.
- Manual `std::env::args` — too much boilerplate for 15+ commands with complex flags.

## R2: File Hashing

**Decision**: `blake3` crate for primary file hashing.

**Rationale**: blake3 is the fastest cryptographic hash available in Rust. Single-threaded throughput exceeds 1 GB/s on modern hardware, easily meeting the 10k files < 3s target. The crate is pure Rust with SIMD acceleration. Used for: file identity in decision logs, embedding cache keys, plan integrity.

**Alternatives considered**:
- `sha256` — 5-10x slower than blake3, no benefit for non-cryptographic use.
- `xxhash` — faster but not cryptographic; collision risk higher at scale.

## R3: Glob Pattern Matching

**Decision**: `globset` crate from the ripgrep ecosystem.

**Rationale**: globset compiles glob patterns into a single automaton for efficient multi-pattern matching. Supports gitignore-style patterns, which aligns with `.librarianignore` and the clarified glob-first rules engine. The `ignore` crate (also from ripgrep) can be used for `.librarianignore` file walking.

**Alternatives considered**:
- `glob` crate — single-pattern only, no compiled set.
- `gitignore` crate — less maintained, fewer features.

## R4: macOS Extended Attributes (xattr)

**Decision**: `xattr` crate for reading/writing Finder tags and colour labels.

**Rationale**: The `xattr` crate provides safe Rust bindings to `fgetxattr`/`fsetxattr`. Finder tags are stored in `com.apple.metadata:_kMDItemUserTags` (binary plist format). Colour labels map to specific index values in `com.apple.FinderInfo`. The crate is `#[cfg(target_os = "macos")]` compatible.

**Implementation notes**:
- Tags: read/write binary plist via `plist` crate, stored in `com.apple.metadata:_kMDItemUserTags`.
- Colours: set via `com.apple.FinderInfo` byte 9 (colour index 0-7).
- Original name preservation: custom xattr `com.apple.metadata:LibrarianOriginalName`.
- Cross-platform fallback: `.librarian-meta.json` sidecar with `#[cfg(not(target_os = "macos"))]`.

**Alternatives considered**:
- Direct `libc` calls — more control but verbose and error-prone.
- `core-foundation` — heavier dependency for just xattr access.

## R5: AI Provider HTTP Client

**Decision**: `reqwest` with `tokio` async runtime.

**Rationale**: reqwest is the most mature async HTTP client in Rust. Both LM Studio and OpenAI expose OpenAI-compatible REST APIs. reqwest supports streaming responses (needed for SSE parsing), TLS, timeouts, and connection pooling. Tokio is the mandated async runtime per the PRD.

**SSE parsing**: Implement a lightweight line-based SSE parser rather than pulling in a dedicated SSE crate. The protocol is simple (`data:` lines, `\n\n` event boundaries) and a custom parser avoids dependency bloat.

**Rate limiting (OpenAI)**: Token bucket implemented with `tokio::time::sleep` and an `Arc<Mutex<TokenBucket>>`. Default 20 rpm, configurable. No external crate needed for this simple pattern.

**Alternatives considered**:
- `hyper` directly — lower-level, more boilerplate for the same outcome.
- `ureq` — synchronous only, doesn't support streaming.
- `eventsource-client` — adds dependency for minimal SSE needs.

## R6: Vector Storage (Qdrant)

**Decision**: `qdrant-client` crate, Qdrant running in Docker.

**Rationale**: Qdrant provides indexed vector similarity search with filtering. At 10k+ embeddings, in-memory brute-force cosine similarity becomes slow. Qdrant runs locally in Docker (consistent with local-first), has a well-maintained Rust client, and supports collection-per-source-root as specified.

**Collection strategy**: One collection per source root (e.g., `downloads`, `desktop`). Points store: file hash as ID, embedding as vector, metadata (filename, bucket, filetype) as payload. Centroid vectors stored in a separate `centroids` collection.

**Alternatives considered**:
- In-memory `ndarray` cosine sim — fine for <1k vectors, too slow for 10k+.
- `tantivy` — full-text search, not vector-native.
- SQLite with vector extension — less mature in Rust ecosystem.

## R7: Content Extraction (Text-Based Only)

**Decision**: `pdf-extract` for PDF text layer, `std::fs::read_to_string` for plain text/markdown/CSV.

**Rationale**: Per clarification, content extraction is limited to text-based files only. PDF is the most complex case — `pdf-extract` handles text layer extraction without OCR. Plain text, markdown, and CSV are read as UTF-8 strings directly. No office document parsing, no OCR, no binary content.

**Fallback for PDF**: If `pdf-extract` fails (encrypted PDF, image-only PDF), treat as a binary file — classify on filename embedding + LLM only. Log the extraction failure.

**Alternatives considered**:
- `lopdf` — lower-level PDF manipulation, requires manual text extraction.
- `poppler` bindings — C dependency, harder to cross-compile.
- `pdfium` bindings — Google's PDF library, heavy dependency.

## R8: Structured Logging

**Decision**: `tracing` + `tracing-subscriber` with JSON and human-readable formatters.

**Rationale**: `tracing` is the Rust ecosystem standard for structured, async-aware logging. Supports spans (useful for per-file classification tracing), structured fields, and multiple output formats. Maps directly to the Observability constitution principle and `--verbose`/`--json`/`--quiet` flags.

**Configuration**: `--verbose` enables `DEBUG` level with human-readable output. `--json` switches to JSON lines format. `--quiet` sets `ERROR` only. Default is `INFO` with human-readable.

**Alternatives considered**:
- `log` + `env_logger` — simpler but no structured fields or spans.
- `slog` — structured but less ecosystem adoption than tracing.

## R9: Filesystem Watching (Corrections)

**Decision**: `notify` crate v6 for detecting post-placement file moves.

**Rationale**: The correction watcher needs to detect when a user moves a file that Librarian previously placed. `notify` provides cross-platform filesystem event watching with debouncing. On macOS it uses FSEvents, which is efficient for watching entire directory trees.

**Strategy**: After `librarian apply`, the watcher monitors destination directories. If a file hash from the manifest reappears at a different path within the correction window, record a correction. The watcher runs as a background check during `librarian process` (not a daemon in v1).

**Alternatives considered**:
- Polling with `std::fs::metadata` — works but wastes CPU and misses rapid moves.
- `kqueue` directly — macOS only, `notify` abstracts this.

## R10: Serialisation Formats

**Decision**: `serde` ecosystem throughout.

| Format | Crate | Usage |
|--------|-------|-------|
| YAML | `serde_yaml` | config.yaml, rules.yaml |
| JSON | `serde_json` | plans, decision log (JSONL), sidecar metadata |
| msgpack | `rmp-serde` | embedding cache, centroid state |
| Binary plist | `plist` | macOS Finder tag xattr values |

**Rationale**: serde is the Rust serialisation standard. All formats have mature serde implementations. YAML for human-editable config, JSON for structured logs and plans (human-inspectable), msgpack for compact binary caches.

## R11: Progress Output

**Decision**: `indicatif` crate for progress bars and spinners.

**Rationale**: indicatif provides multi-bar support (scan, classify, plan phases), styled output, and automatic terminal width detection. Integrates well with tracing by pausing bars during log output.

**Alternatives considered**:
- Manual `\r` output — fragile across terminal types.
- `pbr` — less maintained, fewer features.

## R12: Rules YAML Schema

**Decision**: Custom schema with glob-first matching.

```yaml
rules:
  - name: "Work invoices"
    match:
      extension: "pdf"
      filename: "*invoice*"           # glob (default)
      path: "*/Downloads/*"           # glob
    destination: "{year}/Work/Invoices"
    tags: ["invoice", "work"]
    colour: null                      # optional

  - name: "Screenshots"
    match:
      filename: "regex:^Screenshot \\d{4}-\\d{2}-\\d{2}"
      extension: "png"
    destination: "{year}/Personal/Screenshots"
    tags: ["screenshot"]
    clean_name: true                  # always clean junk patterns
```

**Template variables**: `{year}` (current year), `{month}`, `{date}`, `{ext}`, `{source}` (inbox name).

**Match precedence**: Rules are evaluated in order. First match wins. This is simple, predictable, and documented.
