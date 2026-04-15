# Plan: 10C.2 — `forge update`

## Goal

Add `forge update` command that updates all dependencies to latest compatible versions.

## Design

`forge update` should:

1. Load `forge.toml` dependencies
2. For each dependency, resolve the latest compatible version (using semver constraints)
3. Re-install the latest versions
4. Update `forge.lock` with new versions

This is essentially `forge install` with "force re-resolve" — it bypasses already-installed checks and re-resolves from registry.

### Implementation

1. Add `Update` variant to CLI `Command` enum
2. Add `update_dependencies()` to `package.rs`:
   - Load manifest
   - For each dep, remove existing installed version from `forge_modules/`
   - Run `install_from_manifest()` which will re-resolve from registries
3. Print summary of what was updated

### Files to touch

1. **`src/main.rs`** — add Update subcommand
2. **`src/package.rs`** — add `update_dependencies()` or reuse install path with force flag

### Edge cases

- No forge.toml → error
- No deps → "no dependencies to update"
- Network failure on remote deps → error with which dep failed

## Test strategy

- Mostly integration-level (file system + manifest)
- Unit test: update flow removes old versions before re-installing

## Rollback

Revert changes to `src/main.rs`, `src/package.rs`.
