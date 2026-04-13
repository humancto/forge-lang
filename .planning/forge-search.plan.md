# Plan: 10B.2 — `forge search <query>`

## Goal

Add `forge search <query>` command to search the remote package index by name/description.

## Design

### Index File

The registry repo has an `index.toml` file listing all packages:

```toml
[[packages]]
name = "router"
description = "HTTP router for Forge"
latest = "2.0.0"

[[packages]]
name = "auth"
description = "JWT authentication library"
latest = "1.0.0"
```

### Implementation

1. Add `PackageSummary { name, description, latest }` to `registry.rs`
2. Add `fetch_index() -> Result<Vec<PackageSummary>, String>` — fetches `index.toml` from registry, caches locally
3. Add `search_packages(query, index) -> Vec<PackageSummary>` — filter by case-insensitive substring match on name or description
4. Add `Search { query: String }` variant to CLI `Command` enum in `main.rs`
5. Add handler that fetches index, searches, and prints results in a table format

### Files to touch

1. **`src/registry.rs`** — add PackageSummary, fetch_index(), search_packages()
2. **`src/main.rs`** — add Search subcommand and handler

### Edge cases

- No results → "No packages found matching '<query>'"
- Empty query → list all packages
- Network error → clear error message
- No registry → error with setup hint

## Test strategy

- search_packages() with matching name
- search_packages() with matching description
- search_packages() case insensitive
- search_packages() no match
- search_packages() empty query returns all

## Rollback

Revert changes to `src/registry.rs` and `src/main.rs`.
