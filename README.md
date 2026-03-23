<div align="center">

# indxr

**Fast codebase indexer for AI agents. Built in Rust.**

[Getting Started](#getting-started) · [Documentation](docs/) · [Agent Integration](docs/agent-integration.md) · [MCP Server](docs/mcp-server.md)

</div>

---

AI agents waste tokens reading files just to understand what exists and where. **indxr** pre-computes a compact structural index of your entire codebase — every function signature, every struct field, every import — so agents can orient themselves instantly.

```
indxr → 243 files, 124K lines indexed in 73ms
```

### Why indxr?

- **Save tokens** — agents get structure without reading source files
- **16 languages** — tree-sitter AST parsing + regex extraction
- **Instant** — parallel parsing with incremental caching (sub-10ms on warm runs)
- **MCP server** — agents query the index on-demand via Model Context Protocol
- **Git-aware** — structural diffs show added/removed/modified declarations
- **Token budgets** — intelligently truncate output to fit context windows
- **Zero config** — respects `.gitignore`, sensible defaults, just run it

## Getting Started

### Install

```bash
cargo install --path .
```

### Quick Start

```bash
# Index current directory → stdout
indxr

# Index a project → file
indxr ./my-project -o INDEX.md

# JSON output, only Rust and Python
indxr -f json -l rust,python -o index.json

# Start MCP server for AI agents
indxr serve ./my-project
```

### Example Output

```markdown
# Codebase Index: my-project

> Generated: 2025-03-23 | Files: 42 | Lines: 8,234
> Languages: Rust (28), Python (10), TypeScript (4)

## Directory Structure
src/
  main.rs
  parser/
    mod.rs
    rust.rs

## Public API Surface

**src/main.rs**
- `pub fn main() -> Result<()>`
- `pub struct App`

---

## src/main.rs

**Language:** Rust | **Size:** 1.2 KB | **Lines:** 45

**Declarations:**

`pub fn main() -> Result<()>` [async]
> Entry point for the application
> Line 10 (35 lines)
> implements `Runner`
```

## Features

### Supported Languages

<table>
<tr>
<td>

**Tree-sitter (full AST)**

| Language | Key Extractions |
|----------|----------------|
| Rust | Functions, structs, enums, traits, impls |
| Python | Functions, classes, decorators, docstrings |
| TypeScript | Functions, classes, interfaces, type aliases |
| JavaScript | Functions, classes, exports |
| Go | Functions, methods, structs, interfaces |
| Java | Classes, methods, annotations, fields |
| C | Functions, structs, typedefs, macros |
| C++ | Classes, namespaces, templates |

</td>
<td>

**Regex (structural extraction)**

| Language | Key Extractions |
|----------|----------------|
| Shell | Functions, exports, aliases |
| TOML | Sections, keys, Cargo deps |
| YAML | Top-level keys, docker-compose |
| JSON | Top-level keys, package.json deps |
| SQL | Tables, views, indexes, functions |
| Markdown | Heading hierarchy |
| Protobuf | Messages, services, RPCs |
| GraphQL | Types, queries, mutations |

</td>
</tr>
</table>

Full language reference: [docs/languages.md](docs/languages.md)

### MCP Server

Run `indxr serve` to expose a [Model Context Protocol](https://modelcontextprotocol.io/) server that AI agents can query on-demand:

```bash
indxr serve ./my-project
```

| Tool | What it does |
|------|-------------|
| `lookup_symbol` | Find declarations by name |
| `list_declarations` | List declarations in a file |
| `search_signatures` | Search signatures by substring |
| `get_tree` | Get directory structure |
| `get_imports` | Get imports for a file |
| `get_stats` | Index statistics |

Add to your AI tool's MCP config:

```json
{
  "mcpServers": {
    "indxr": {
      "command": "indxr",
      "args": ["serve", "/path/to/project"]
    }
  }
}
```

Setup guides for Claude Code, Claude Desktop, Cursor, Windsurf, and more: [docs/mcp-server.md](docs/mcp-server.md)

### Filtering & Scoped Output

Query exactly what you need instead of the full index:

```bash
# Files under a path
indxr --filter-path src/parser

# Only functions, only public
indxr --kind function --public-only

# Find a symbol by name
indxr --symbol "parse"

# Combine filters
indxr --filter-path src/model --kind struct --public-only
```

### Git-Aware Structural Diffing

See what changed structurally since any git ref — not line diffs, but declaration-level changes:

```bash
indxr --since main
indxr --since v1.0.0
indxr --since HEAD~5
```

```
## Modified Files

### src/parser/mod.rs
+ `pub fn new_parser() -> Parser`
- `fn old_helper()`
~ `fn process(x: i32)` → `fn process(x: i32, y: i32)`
```

### Token Budget

Fit output into finite context windows with intelligent progressive truncation:

```bash
indxr --max-tokens 4000
```

Truncation priority: doc comments → private declarations → children → least-important files. The directory tree and public API surface are always preserved first.

### Output Formats & Detail Levels

| Format | Best for |
|--------|----------|
| `markdown` (default) | AI context windows — compact, scannable |
| `json` | Tool integrations, programmatic access |
| `yaml` | Human-readable structured data |

| Detail Level | Content |
|-------------|---------|
| `summary` | Directory tree + file list |
| `signatures` (default) | + declarations, imports, doc comments, line numbers |
| `full` | + metadata badges, relationships, body line counts |

## Performance

| Codebase | Files | Lines | Cold | Cached |
|----------|-------|-------|------|--------|
| Small (indxr) | 23 | 4.6K | 17ms | 5ms |
| Medium (atuin) | 132 | 22K | 20ms | 6ms |
| Large (cloud-hypervisor) | 243 | 124K | 73ms | ~10ms |

## How It Works

1. **Walk** the directory tree (`.gitignore`-aware)
2. **Detect** language by file extension
3. **Check cache** — skip unchanged files (mtime + xxhash)
4. **Parse** with tree-sitter or regex (parallel via rayon)
5. **Extract** declarations, metadata, and relationships
6. **Filter** by path, kind, visibility, symbol, or token budget
7. **Format** as Markdown, JSON, or YAML
8. **Cache** results for next run

## Documentation

| Document | Description |
|----------|-------------|
| [Agent Integration Guide](docs/agent-integration.md) | Using indxr with Claude, Codex, Cursor, Copilot, and more |
| [MCP Server](docs/mcp-server.md) | MCP server setup, tools, and configuration |
| [CLI Reference](docs/cli-reference.md) | Complete command-line reference |
| [Supported Languages](docs/languages.md) | Full language support details and extraction reference |
| [Output Formats](docs/output-formats.md) | Formats, detail levels, and example outputs |
| [Filtering & Scoping](docs/filtering.md) | Path, kind, symbol, and visibility filters |
| [Git Diffing](docs/git-diffing.md) | Structural diff since any git ref |
| [Token Budget](docs/token-budget.md) | Context window optimization and truncation strategy |
| [Caching](docs/caching.md) | Incremental cache system and performance |

## License

MIT
