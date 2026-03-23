# indxr

Fast codebase indexer for AI agents. Built in Rust.

AI agents waste tokens reading files just to understand what exists and where. `indxr` pre-computes a compact structural index of your entire codebase — every function signature, every struct field, every import — so agents can orient themselves instantly.

## Install

```bash
cargo install --path .
```

## Usage

```bash
# Index current directory, output markdown to stdout
indxr

# Index a specific project, write to file
indxr ./my-project -o INDEX.md

# JSON output with only Rust and Python
indxr -f json -l rust,python -o index.json

# Summary only (directory tree, no declarations)
indxr -d summary

# Full detail with line numbers
indxr -d full -o CODEBASE.md

# Only public API surface
indxr --public-only -o API.md

# Find a specific symbol across the codebase
indxr --symbol "Cache"

# Only functions in a specific directory
indxr --kind function --filter-path src/parser

# Structural diff since a branch/tag/commit
indxr --since main
indxr --since HEAD~5

# Fit output within a token budget
indxr --max-tokens 4000

# Start MCP server for AI agent integration
indxr serve ./my-project
```

## MCP Server

`indxr serve` starts a [Model Context Protocol](https://modelcontextprotocol.io/) server over stdin/stdout. This lets AI agents query the index on-demand instead of loading the full output into context.

```bash
indxr serve ./my-project
```

### Available tools

| Tool | Description |
|------|-------------|
| `lookup_symbol` | Find declarations matching a name (case-insensitive substring) |
| `list_declarations` | List declarations in a file, optionally filtered by kind |
| `search_signatures` | Search signatures by substring match |
| `get_tree` | Get directory/file tree, optionally filtered by path |
| `get_imports` | Get imports for a specific file |
| `get_stats` | Get index statistics (files, lines, languages) |

### MCP configuration

Add to your MCP client config (e.g. Claude Desktop, Cursor):

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

## What it extracts

For each source file, `indxr` extracts:

- **Functions** — name, parameters, return type, visibility
- **Structs / Classes** — name, fields, methods
- **Enums** — name, variants
- **Traits / Interfaces** — name, method signatures
- **Impl blocks** — associated methods
- **Imports** — full import paths
- **Constants / Type aliases / Modules**
- **Doc comments** — Rust `///`, Python docstrings, JSDoc `/** */`, Javadoc, Go `//` comments
- **Metadata** — `is_test`, `is_async`, `is_deprecated`, body line count
- **Relationships** — `implements`, `extends` (trait impls, class inheritance)

## Supported languages

### Tree-sitter based (full AST parsing)

| Language | Extracts |
|----------|----------|
| Rust | Functions, structs, enums, traits, impl blocks, modules, constants, type aliases |
| Python | Functions, classes, decorators, docstrings, imports, module-level constants |
| TypeScript | Functions, classes, interfaces, enums, type aliases, exports |
| JavaScript | Functions, classes, exports, const declarations |
| Go | Functions, methods (with receivers), structs, interfaces, constants |
| Java | Classes, interfaces, enums, methods, constructors, fields, annotations |
| C | Functions, structs, enums, typedefs, `#include`, `#define` |
| C++ | Everything in C + classes, namespaces, templates, access specifiers |

### Regex based (structural extraction)

| Language | Extracts |
|----------|----------|
| Shell | Functions, exports, aliases, source imports |
| TOML | Sections, keys; Cargo.toml dependency extraction |
| YAML | Top-level keys; docker-compose service detection |
| JSON | Top-level keys; package.json dependency extraction |
| SQL | Tables (with columns), views, indexes, functions, types |
| Markdown | Heading hierarchy |
| Protobuf | Messages (with fields), services (with RPCs), enums |
| GraphQL | Types, interfaces, enums, queries, mutations, subscriptions |

## Filtering & scoped output

Instead of dumping the entire index, query just what you need:

```bash
# Only files under a path
indxr --filter-path src/model

# Only a specific declaration kind
indxr --kind function
indxr --kind struct
indxr --kind trait

# Only public declarations
indxr --public-only

# Find a symbol by name (case-insensitive substring)
indxr --symbol "parse"

# Combine filters
indxr --filter-path src/parser --kind function --public-only
```

## Git-aware structural diffing

Show what changed structurally since a git ref — not line diffs, but added/removed/modified declarations:

```bash
indxr --since main
indxr --since v1.0.0
indxr --since HEAD~3
indxr --since abc1234
```

Output shows structural changes with `+`/`-`/`~` markers:

```
# Structural Changes (since main)

## Added Files
- src/new_module.rs

## Modified Files

### src/parser/mod.rs
+ `pub fn new_parser() -> Parser`
- `fn old_helper()`
~ `fn process(x: i32)` -> `fn process(x: i32, y: i32)`
```

Works with JSON output too: `indxr --since main -f json`

## Token budget

Agents have finite context. Specify a budget and `indxr` intelligently truncates:

```bash
indxr --max-tokens 4000
```

Truncation is progressive:
1. Remove doc comments
2. Remove private declarations
3. Remove children (fields, methods)
4. Drop files from the end

The directory tree and public API surface are always preserved first.

## Output formats

### Markdown (default)

Designed for AI agent context windows — compact, scannable, minimal tokens:

```markdown
# Codebase Index: my-project

> Generated: 2026-03-23 | Files: 42 | Lines: 8,234
> Languages: Rust (28), Python (10), TypeScript (4)

## Directory Structure
...

## Public API Surface

**src/main.rs**
- `pub fn main() -> Result<()>`
- `pub struct App`

---

## src/main.rs

**Language:** Rust | **Size:** 1.2 KB | **Lines:** 45

**Declarations:**

`pub fn main() -> Result<()>` [async]
> Entry point
> Line 10 (35 lines)
> implements `Runner`
```

Features:
- **Public API Surface** section at the top for quick orientation
- **Line numbers** at all detail levels (not just `full`)
- **Metadata badges** — `[test]`, `[async]`, `[deprecated]`
- **Relationship display** — implements/extends annotations
- **Body line counts** — helps agents decide whether to read the full source

### JSON

Full structured output via `serde`. Machine-readable for tool integrations.

### YAML

Same structure as JSON, human-readable format.

## Detail levels

| Level | What's included |
|-------|-----------------|
| `summary` | Directory tree + file list with languages |
| `signatures` (default) | Above + all declarations with signatures, imports, doc comments, line numbers |
| `full` | Above + metadata badges, relationships, body line counts |

## Performance

| Codebase | Files | Lines | Cold | Cached |
|----------|-------|-------|------|--------|
| Small (indxr) | 23 | 4.6K | 17ms | 5ms |
| Medium (atuin) | 132 | 22K | 20ms | 6ms |
| Large (cloud-hypervisor) | 243 | 124K | 73ms | ~10ms |

Key design choices:
- **tree-sitter** for accurate AST parsing (not regex) on 8 core languages
- **regex** for lightweight structural extraction on config/schema languages
- **rayon** for parallel file parsing
- **xxhash** + mtime for incremental caching (`.indxr-cache/`)
- **ignore** crate for .gitignore-aware directory walking (from ripgrep)

## CLI reference

```
indxr [OPTIONS] [PATH] [COMMAND]

Commands:
  serve  Start MCP server for AI agent integration

Arguments:
  [PATH]  Root directory to index [default: .]

Options:
  -o, --output <FILE>            Output file path (default: stdout)
  -f, --format <FORMAT>          markdown|json|yaml [default: markdown]
  -d, --detail <LEVEL>           summary|signatures|full [default: signatures]
      --max-depth <N>            Maximum directory depth
      --max-file-size <KB>       Skip files larger than N KB [default: 512]
  -l, --languages <LANGS>        Filter by language (comma-separated)
  -e, --exclude <PATTERNS>       Glob patterns to exclude
      --no-gitignore             Don't respect .gitignore
      --no-cache                 Disable incremental caching
      --cache-dir <DIR>          Cache directory [default: .indxr-cache]
  -q, --quiet                    Suppress progress output
      --stats                    Print indexing statistics to stderr
      --filter-path <SUBPATH>    Filter to a subdirectory
      --symbol <SYMBOL>          Search for a symbol by name
      --kind <KIND>              Filter by declaration kind
      --public-only              Only show public declarations
      --since <REF>              Structural diff since a git ref
      --max-tokens <N>           Token budget for output
```

## How it works

1. **Walk** the directory tree (`.gitignore`-aware via the `ignore` crate)
2. **Detect** language by file extension
3. **Check cache** — skip unchanged files (mtime + size, fallback to xxhash)
4. **Parse** source files with tree-sitter or regex (parallel via rayon)
5. **Extract** declarations, metadata, and relationships
6. **Filter** by path, kind, visibility, symbol, or token budget
7. **Format** output as Markdown, JSON, or YAML
8. **Save cache** for next run

## License

MIT
