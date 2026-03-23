# indxr

A fast Rust codebase indexer for AI agents. Extracts structural maps (declarations, imports, tree) using tree-sitter and regex parsing across 27 languages.

## Codebase Navigation — MUST USE indxr MCP tools

An MCP server called `indxr` is available. **Always use indxr tools before the Read tool.** Do NOT read full source files as a first step — use the MCP tools to explore, then read only what you need.

### Workflow (follow this order):
1. `get_tree` — see directory/file layout
2. `get_file_summary` — get a complete overview of any file (declarations, imports, counts) WITHOUT reading the file
3. `get_file_context` — understand a file's reverse dependencies and related files
4. `lookup_symbol` / `search_signatures` — find specific functions/types across the codebase
5. `read_source` — read source code by **symbol name** or line range (NOT full files)
6. `list_declarations` / `get_imports` — drill into a file's exports or dependencies

### When to use the Read tool instead:
- You need to **edit** a file (Read is required before Edit)
- You need exact formatting/whitespace that `read_source` doesn't preserve
- The file is not a source file (e.g., CLAUDE.md, Cargo.toml, docs)

### DO NOT:
- Read full source files just to understand what's in them — use `get_file_summary`
- Read full source files to review code — use `get_file_summary` to triage, then `read_source` on specific symbols
- Dump all files into context — use MCP tools to be surgical

After making code changes, run `regenerate_index` to keep INDEX.md current.
