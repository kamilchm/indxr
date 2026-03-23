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

# YAML output
indxr -f yaml -o index.yaml
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

## Supported languages

| Language | Extractor |
|----------|-----------|
| Rust | Functions, structs, enums, traits, impl blocks, modules, constants, type aliases |
| Python | Functions, classes, decorators, docstrings, imports, module-level constants |
| TypeScript | Functions, classes, interfaces, enums, type aliases, exports |
| JavaScript | Functions, classes, exports, const declarations |
| Go | Functions, methods (with receivers), structs, interfaces, constants |
| Java | Classes, interfaces, enums, methods, constructors, fields, annotations |
| C | Functions, structs, enums, typedefs, `#include`, `#define` |
| C++ | Everything in C + classes, namespaces, templates, access specifiers |

## Output formats

### Markdown (default)

Designed for AI agent context windows — compact, scannable, minimal tokens:

```markdown
# Codebase Index: my-project

> Generated: 2026-03-23 | Files: 42 | Lines: 8,234
> Languages: Rust (28), Python (10), TypeScript (4)

## Directory Structure

my-project/
  src/
    main.rs
    config.rs

---

## src/main.rs

**Language:** Rust | **Size:** 1.2 KB | **Lines:** 45

**Imports:**
- `crate::config::Config`
- `clap::Parser`

**Declarations:**

`pub fn main() -> Result<()>`
> Entry point. Parses CLI args and runs the indexer.

`pub struct App`
> Fields: `config: Config`, `registry: ParserRegistry`
```

### JSON

Full structured output via `serde`. Machine-readable for tool integrations.

### YAML

Same structure as JSON, human-readable format.

## Detail levels

| Level | What's included |
|-------|-----------------|
| `summary` | Directory tree + file list with languages |
| `signatures` (default) | Above + all declarations with signatures, imports, doc comments |
| `full` | Above + line numbers for each declaration |

## Performance

| Codebase | Files | Lines | Cold | Cached |
|----------|-------|-------|------|--------|
| Small (indxr) | 23 | 4.6K | 17ms | 5ms |
| Medium (atuin) | 132 | 22K | 20ms | 6ms |
| Large (cloud-hypervisor) | 243 | 124K | 73ms | ~10ms |

Key design choices:
- **tree-sitter** for accurate AST parsing (not regex)
- **rayon** for parallel file parsing
- **xxhash** + mtime for incremental caching (`.indxr-cache/`)
- **ignore** crate for .gitignore-aware directory walking (from ripgrep)

## CLI reference

```
indxr [OPTIONS] [PATH]

Arguments:
  [PATH]  Root directory to index [default: .]

Options:
  -o, --output <FILE>         Output file path (default: stdout)
  -f, --format <FORMAT>       markdown|json|yaml [default: markdown]
  -d, --detail <LEVEL>        summary|signatures|full [default: signatures]
      --max-depth <N>         Maximum directory depth to traverse
      --max-file-size <KB>    Skip files larger than N KB [default: 512]
  -l, --languages <LANGS>     Filter by language (comma-separated)
  -e, --exclude <PATTERNS>    Glob patterns to exclude
      --no-gitignore          Don't respect .gitignore
      --no-cache              Disable incremental caching
      --cache-dir <DIR>       Cache directory [default: .indxr-cache]
  -q, --quiet                 Suppress progress output
      --stats                 Print indexing statistics to stderr
```

## How it works

1. **Walk** the directory tree (`.gitignore`-aware via the `ignore` crate)
2. **Detect** language by file extension
3. **Check cache** — skip unchanged files (mtime + size, fallback to xxhash)
4. **Parse** source files with tree-sitter (parallel via rayon)
5. **Extract** declarations using language-specific traversal of the AST
6. **Format** output as Markdown, JSON, or YAML
7. **Save cache** for next run

## License

MIT
