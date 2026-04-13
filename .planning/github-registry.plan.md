# Plan: 10B.1 — GitHub-based package index

## Goal

Allow `forge install` to fetch packages from a central GitHub-hosted registry index.

## Design

### Index Format

Registry is a GitHub repo (default: `forge-lang/registry`). Each package has a TOML file:

```
packages/<name>.toml
```

```toml
[package]
name = "router"
description = "HTTP router for Forge"
repository = "https://github.com/user/forge-router"

[[versions]]
version = "1.0.0"
url = "https://github.com/user/forge-router/archive/refs/tags/v1.0.0.tar.gz"
checksum = "sha256:abc123..."

[[versions]]
version = "2.0.0"
url = "https://github.com/user/forge-router/archive/refs/tags/v2.0.0.tar.gz"
checksum = "sha256:def456..."
```

### Implementation

1. **Data types** in `src/registry.rs` (new file):
   - `PackageEntry { name, description, repository, versions: Vec<VersionEntry> }`
   - `VersionEntry { version, url, checksum }`
   - `parse_package_entry(toml_str) -> Result<PackageEntry, String>`

2. **Remote fetch**:
   - `fetch_package_entry(name, registry_url) -> Result<PackageEntry, String>`
   - URL: `{registry_url}/packages/{name}.toml`
   - Default registry URL from `FORGE_REGISTRY_URL` env var or `https://raw.githubusercontent.com/forge-lang/registry/main`
   - Accept `GITHUB_TOKEN` env var for auth header (rate limit mitigation)
   - Uses `reqwest::blocking` (already in crate for runtime client)

3. **Local cache**:
   - Cache fetched entries at `.forge/cache/registry/<name>.toml`
   - TTL: 1 hour (configurable via `FORGE_CACHE_TTL` env var in seconds)
   - Atomic writes: write to temp file, then rename

4. **Download + extract**:
   - `download_and_extract(url, dest_dir) -> Result<(), String>`
   - Download tarball to temp file
   - Extract to temp directory, then rename to final location (atomic)
   - If tarball contains a single root directory (GitHub archive style), flatten it

5. **Integration in `package.rs`**:
   - Update `install_from_registry_as()`: after local registry lookup fails, try remote
   - Resolve semver against remote `VersionEntry` list
   - Download and extract the resolved version
   - Lockfile already handles recording installed version

### Files to touch

1. **`src/registry.rs`** (new) — remote registry types, fetch, cache, download
2. **`src/package.rs`** — integrate remote fallback in `install_from_registry_as()`
3. **`src/main.rs`** — add `mod registry`

### Edge cases

- Network unavailable → fall back to local registry only, print warning
- Package not in remote index → clear error listing where we looked
- Download/extraction fails → clean up temp files, error with URL
- Cache fresh → skip network fetch
- Checksum empty → warn but allow (full enforcement in 10B.3)

### Out of scope

- Publish flow (how packages get into the registry repo)
- Version yanking
- Full checksum enforcement (10B.3)

## Test strategy

- Parse a PackageEntry from TOML string
- Semver resolution against VersionEntry list (reuse resolve logic)
- Cache TTL logic (fresh vs stale)
- Integration tests require network, skip in CI

## Rollback

Delete `src/registry.rs`, revert `src/package.rs` and `src/main.rs`.
