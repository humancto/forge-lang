# Phase 2.4 — `forge publish`

Written: 2026-04-11
Prerequisite: Phase 2.1-2.3 (manifest, module resolution, install) — all DONE

---

## Goal

Enable `forge publish` to package and distribute Forge projects. Since building and maintaining a centralized registry service is a large infrastructure commitment, we take a pragmatic two-tier approach:

1. **Tier 1 (this PR):** Local filesystem registry — `forge publish` packages the project as a tarball and copies it to the local registry (`~/.forge/registry/`). Other projects on the same machine can `forge install name@version`.
2. **Tier 2 (future):** GitHub-based publishing — `forge publish --github` creates a GitHub release with the tarball attached. `forge install` gains the ability to fetch from GitHub releases.

This PR implements Tier 1 only.

## Design

### What `forge publish` does

1. **Validate** — read `forge.toml`, verify required fields exist: `name`, `version`
2. **Package** — create a tarball (`.tar.gz`) containing:
   - All `.fg` files in the project (respecting `.forgeignore` if present, else sensible defaults)
   - `forge.toml` manifest
   - Exclude: `forge_modules/`, `.git/`, `target/`, `.forge/`, `tests/`, `*.lock`
3. **Checksum** — compute SHA-256 of the tarball
4. **Copy to local registry** — place at `~/.forge/registry/<name>/<version>/`
   - Extract the tarball contents into the registry directory
   - Write a `.forge-checksum` file with the SHA-256
5. **Report** — print success message with name, version, size, checksum

### Package file collection

Default include: all `.fg` files, `forge.toml`, `README.md` (if present)
Default exclude: `forge_modules/`, `.git/`, `target/`, `.forge/`, `node_modules/`, `tests/`, `*.lock`, `*.tar.gz`

If `.forgeignore` exists, use it (gitignore-style patterns) for exclude rules.

### Registry directory layout

```
~/.forge/registry/
  mylib/
    1.0.0/
      forge.toml
      main.fg
      src/
        helper.fg
      .forge-checksum    # SHA-256 of original tarball
    1.1.0/
      ...
```

This layout is already compatible with `find_registry_package()` in `package.rs` (line 210-231) which checks `root.join(name).join(version)`.

### CLI integration

```
forge publish              # Package and publish to local registry
forge publish --dry-run    # Show what would be packaged without publishing
```

### Manifest validation

Required for publish:

- `project.name` must not be default ("forge-project")
- `project.version` must not be empty

Optional but recommended (warn if missing):

- `project.description`
- `project.license`
- `project.authors`

### Interaction with `forge install`

Already works — `install_from_registry_as()` searches `FORGE_REGISTRY_PATH` and `.forge/registry/` for `<name>/<version>/`. After `forge publish`, the package is findable at `~/.forge/registry/<name>/<version>/`.

For cross-machine sharing, users can:

- Set `FORGE_REGISTRY_PATH` to a shared network directory
- Use git dependencies (`forge install <git-url>`) — already supported
- Wait for Tier 2 (GitHub releases)

---

## Implementation

### New file: `src/publish.rs`

```rust
pub fn publish(dry_run: bool) {
    // 1. Load and validate manifest
    // 2. Collect files to package
    // 3. Create tarball
    // 4. Compute checksum
    // 5. Copy to registry (unless dry_run)
    // 6. Report
}
```

Dependencies: `flate2` (already in Cargo.toml for gzip), `tar` crate for tarball creation.

Check if `tar` is already a dependency, otherwise `flate2` + manual tar or just copy files directly (simpler — no tarball intermediate, just copy the file tree to the registry directory).

**Simpler approach (no tar crate):** Skip the tarball entirely. Just copy the project files directly to `~/.forge/registry/<name>/<version>/`. Compute checksum of a manifest hash (name + version + file list + sizes). This avoids adding a dependency and the registry layout is already a directory tree.

### Changes to `src/main.rs`

Add `Publish` command variant:

```rust
/// Publish the current project to the local registry
Publish {
    /// Show what would be packaged without publishing
    #[arg(long)]
    dry_run: bool,
},
```

Add match arm:

```rust
Some(Command::Publish { dry_run }) => {
    publish::publish(dry_run);
}
```

### Changes to `src/package.rs`

No changes needed — `find_registry_package()` already finds packages in `~/.forge/registry/<name>/<version>/`.

Add `~/.forge/registry` to `default_registry_roots()` (currently only has `.forge/registry` which is project-local, not user-global).

---

## File Change Summary

| File             | Change                                            |
| ---------------- | ------------------------------------------------- |
| `src/publish.rs` | New — publish logic                               |
| `src/main.rs`    | Add `Publish` command, `mod publish`              |
| `src/package.rs` | Add `~/.forge/registry` to default registry roots |
| `CHANGELOG.md`   | Entry under [Unreleased]                          |

---

## Test Plan

1. **publish_validates_manifest** — missing name/version fails
2. **publish_collects_fg_files** — finds all `.fg` files, excludes forge_modules/
3. **publish_creates_registry_entry** — files appear at `~/.forge/registry/<name>/<version>/`
4. **publish_dry_run_no_side_effects** — dry_run prints but doesn't create files
5. **publish_overwrites_existing_version** — re-publish same version replaces files
6. **publish_checksum_written** — `.forge-checksum` file exists after publish
7. **publish_install_round_trip** — publish a package, then install it in another project
8. **publish_respects_forgeignore** — files matching `.forgeignore` patterns are excluded

---

## Risks & Mitigations

1. **No version conflict detection:** Publishing the same name@version from different projects overwrites silently. Acceptable for local registry — users control their own machine.
2. **No authentication:** Local registry is just filesystem. No auth needed.
3. **Large projects:** Copying all `.fg` files could be slow for very large projects. Mitigated by default excludes.
4. **Cross-platform paths:** `~/.forge/registry` must expand `~` correctly on all platforms. Use `dirs::home_dir()` or `std::env::var("HOME")`.
