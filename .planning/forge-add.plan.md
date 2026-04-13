# Plan: 10C.1 — `forge add <pkg>`

## Goal

Add `forge add <pkg>` command that adds a dependency to `forge.toml` and installs it.

## Design

`forge add router` should:

1. Load (or create) `forge.toml`
2. Add `router = "*"` to `[dependencies]`
3. Run `forge install` to resolve and install

`forge add router@^1.0` should add `router = "^1.0"`.

### Implementation

1. Add `Add { package: String }` variant to CLI `Command` enum
2. Parse `<name>` or `<name>@<version>` format
3. Load manifest from `forge.toml` (create minimal one if missing)
4. Add/update the dependency in `[dependencies]`
5. Write back the updated `forge.toml` (preserve existing content)
6. Run `install_from_manifest()` to install

### Manifest writing approach

Use `toml_edit` crate for preserving formatting and comments. If not available, use a simple approach: read the file, parse as `toml_edit::Document`, add the dep, serialize back.

Actually, `toml_edit` is not in Cargo.toml. Simpler approach: read the raw TOML, use string manipulation to add/update the `[dependencies]` section, avoiding a full rewrite that loses comments.

Even simpler: since `forge.toml` is a project config (not user-maintained config with lots of comments), we can:

1. Parse with `toml::from_str`
2. Add the dep
3. Serialize with `toml::to_string_pretty`

This loses comments but is acceptable for a v1.

### Files to touch

1. **`src/main.rs`** — add Add subcommand and handler
2. **`src/manifest.rs`** — add `save_manifest()` function
3. **`src/package.rs`** — already has `install_from_manifest()`

### Edge cases

- No `forge.toml` → create minimal one
- Package already in deps → update version
- Invalid package name → error
- `@` in version spec → parse correctly

## Test strategy

- Parse `name@version` format
- Parse `name` (no version = "\*")
- save_manifest round-trip preserves deps
- Adding to existing manifest with deps

## Rollback

Revert changes to `src/main.rs`, `src/manifest.rs`.
