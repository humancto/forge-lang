# Plan: 10A.2 — Semver resolution algorithm

## Goal

Given a semver constraint (e.g., `^1.0`) and a list of available versions in the registry, find the latest compatible version.

## Current State

- `install_from_registry_as()` calls `find_registry_package(name, version, roots)` which does exact path matching: `root/name/version/`
- Registry layout: `root/<name>/<version>/` directories, each containing package files
- `DependencySpec` now has `version_req()` (from 10A.1) that parses constraints into `semver::VersionReq`
- Callers of `install_from_registry_as`: `install_manifest_dependencies()` at lines 110 and 137

## Approach

1. Add `resolve_best_version(name, req, registry_roots) -> Result<(String, PathBuf), String>`:
   - **Sanitize `name`**: reject if contains `/`, `..`, or is empty (directory traversal protection)
   - For each registry root, list subdirectories of `root/<name>/`
   - Parse each subdirectory name as `semver::Version` (skip non-parseable names silently)
   - Filter to those matching the constraint via `VersionReq::matches()`
   - Use `.max()` to pick the highest matching version
   - Return `(version_string, path_to_package)`
   - On no match: error message includes list of available versions

2. Update `install_from_registry_as()` to accept a `&semver::VersionReq` + version string instead of raw version:
   - Call `resolve_best_version()` for resolution

3. Update `install_manifest_dependencies()` callers (lines 110, 137) to parse version constraint first via `DependencySpec::version_req()`, pass to updated `install_from_registry_as()`

### Files to touch

1. **`src/package.rs`** — add `resolve_best_version()`, update `install_from_registry_as()` and callers

### Edge cases

- No versions available → error with "no versions found for '<name>'"
- No matching version → error listing available versions
- Constraint `*` → latest version
- Pre-release versions → excluded by default (semver crate behavior, correct)
- Non-semver directory names → skip silently
- Directory traversal in name → reject early

### Out of scope (deferred to 10A.3)

- Transitive dependency resolution
- Conflict detection between incompatible constraints

## Test strategy

- Registry with versions 1.0.0, 1.0.3, 1.1.0, 2.0.0:
  - `^1.0` → 1.1.0 (latest compatible)
  - `~1.0` → 1.0.3 (highest in 1.0.x range)
  - `*` → 2.0.0 (latest overall)
  - `>=1.0, <2.0` → 1.1.0
  - `^3.0` → error listing available versions
  - Empty registry → error
- Existing test `install_manifest_resolves_version_dependencies_from_registry` still passes (exact version is valid semver)

## Rollback

Revert changes to `src/package.rs`.
