# Plan: 10A.3 — Transitive dependency resolution

## Goal

If A depends on B and B depends on C, `forge install` installs all three. Detect and reject dependency cycles.

## Current State

- `install_manifest_dependencies()` iterates `manifest.dependencies` and installs each one (registry, git, or path)
- No recursion: if package B has its own `forge.toml` with dependencies, those are ignored
- Packages are installed to `forge_modules/<name>/`
- Each installed package may contain a `forge.toml` with its own `[dependencies]`

## Approach

### Implementation

1. Add `resolve_transitive(name, packages_dir, registry_roots, visiting) -> Result<Vec<LockedPackage>, String>`:
   - After installing a package, load its `forge.toml` from `packages_dir/<name>/forge.toml`
   - If it has dependencies, iterate them and install + recurse (DFS)
   - `visiting: &mut Vec<String>` tracks the current resolution path (ordered for error messages)
   - If `name` is already in `visiting`, return error with full cycle chain
   - If already installed (exists in `packages_dir/<dep-name>/`), skip but warn if requested version differs from installed
   - Properly remove from `visiting` on backtrack (insert before recursion, remove after)
   - Path deps in transitive packages: resolve relative to the installed package location (`packages_dir/<name>/`), not cwd

2. Update `install_manifest_dependencies()` to call `resolve_transitive` after installing each direct dependency

3. Error strategy: abort on first transitive install failure (consistent with current behavior)

### Files to touch

1. **`src/package.rs`** — add `resolve_transitive()`, update `install_manifest_dependencies()`

### Edge cases

- A -> B -> A (cycle) → error with full chain
- A -> B -> C, A -> C (diamond) → install C once, skip on second encounter
- Package with no `forge.toml` → no transitive deps (leaf)
- Skipped dep with different version → warning printed
- Path deps in transitive packages → resolved relative to installed package dir

## Test strategy

- A depends on B (path dep), B depends on C (path dep): all three installed
- A -> B -> A cycle: error mentioning circular dependency
- Diamond: A -> B, A -> C, B -> C: C installed once, no error
- Leaf package (no forge.toml): no error, no recursion

## Rollback

Revert changes to `src/package.rs`.
