# Plan: 10A.1 — Semver constraint parsing

## Goal

Parse semver version constraints like `^1.0`, `~1.5`, `>=1.0.0, <2.0.0`, `*` in `forge.toml` dependency specs.

## Approach

Add the `semver` crate (well-maintained, standard Rust ecosystem crate) which provides:

- `Version` — parsed semver version (e.g., `1.2.3`)
- `VersionReq` — parsed version requirement (e.g., `^1.0`, `>=1.0.0, <2.0.0`)
- `VersionReq::matches(&self, version: &Version) -> bool`

### Implementation

1. Add `semver = "1"` to Cargo.toml
2. Add version constraint validation to `DependencySpec`
3. Add a `version_req()` method to `DependencySpec` that parses the version string as a `VersionReq`
4. Add a `matches_version()` method that checks if a given version satisfies the constraint
5. Add tests

### Files to touch

1. **`Cargo.toml`** — add `semver = "1"`
2. **`src/manifest.rs`** — add `version_req()` and `matches_version()` methods

### Edge cases

- Empty version string → treat as `*` (any version)
- Invalid semver → return error
- Plain version `1.2.3` → exact match (semver crate handles this)

## Test strategy

- `^1.0` matches `1.5.0` but not `2.0.0`
- `~1.5` matches `1.5.3` but not `1.6.0`
- `>=1.0.0, <2.0.0` range
- `*` matches everything
- `1.2.3` exact match

## Rollback

Remove semver from Cargo.toml, revert manifest.rs.
