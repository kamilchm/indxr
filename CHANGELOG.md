# Changelog

## Unreleased

### Added
- `indxr wiki preflight` to inspect included members, groups, largest files, and generation bottlenecks before running wiki generation.
- `indxr wiki members` warnings for suspicious generated/vendor directories such as `node_modules` and `dist`.

### Changed
- Wiki planning now repairs uncovered files automatically by grouping them and attaching them to existing or generated pages so coverage gets much closer to complete on large repos.
- Wiki plan source-file resolution now accepts member-name-prefixed paths and expands glob-like entries more reliably.
- Workspace indexing now avoids re-indexing nested workspace members from the root member and normalizes member file paths to workspace-relative paths.
- Wiki status coverage now deduplicates overlapping workspace file paths.

### Fixed
- Wiki planning and page-generation contexts now respect hard size caps instead of growing past the intended budget and causing prompt-too-long failures.
