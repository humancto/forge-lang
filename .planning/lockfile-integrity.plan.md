# Plan: 10C.3 — Lockfile Integrity Check

## Goal

`forge install` verifies `forge.lock` checksums match installed packages, warns on tampering.

## Design (revised after expert review)

### Checksum domain problem

Existing `forge.lock` checksums are tarball SHA-256s (from remote registry). A directory-content hash will never match a tarball hash. Solution: use `checksum_kind` to distinguish.

### LockedPackage changes

Add optional `checksum_kind` field:

```rust
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    pub source: String,
    pub checksum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum_kind: Option<String>, // "directory-sha256"
}
```

Old lockfiles without `checksum_kind` are treated as tarball checksums and skipped during directory verification.

### Directory checksum algorithm

1. Walk the package directory recursively
2. Collect all file paths relative to the package root
3. Exclude platform junk: `.DS_Store`, `__MACOSX/`, `.gitkeep`
4. Sort by byte-order of UTF-8 relative paths
5. For each file: feed `relative_path_bytes` then `file_content_bytes` into a streaming SHA-256
6. Output hex-encoded digest prefixed with `directory-sha256:`

### Where to hook in

1. **On install**: after `install_single_dependency()` succeeds, compute directory checksum, store in `LockedPackage` with `checksum_kind = Some("directory-sha256")`
2. **On verify**: after `install_manifest_dependencies()` completes, call `verify_lockfile_integrity()` which checks only entries where `checksum_kind == Some("directory-sha256")`

### Implementation

1. **`src/manifest.rs`** — add `checksum_kind: Option<String>` to `LockedPackage`
2. **`src/package.rs`**:
   - Add `compute_directory_checksum(dir: &Path) -> Result<String, String>`
   - Modify `install_single_dependency` to compute + store directory checksum after install
   - Add `verify_lockfile_integrity(lockfile_path, packages_dir) -> Vec<String>`
   - Call verify from `install_from_manifest()` after install completes
   - Missing package dir during verify → warning (not skip)

### Edge cases

- No `forge.lock` → skip verification
- `checksum_kind` is None (old lockfile) → skip that entry
- `checksum_kind` is `"directory-sha256"` but checksum is empty → skip
- Package dir missing → warning "package 'X' not found in forge_modules/"
- `forge_modules/` doesn't exist → skip all

## Test strategy

- `compute_directory_checksum` determinism: same dir → same hash
- Tampered file → different hash
- File ordering doesn't affect hash
- `verify_lockfile_integrity` detects mismatch
- Old lockfile entries (no `checksum_kind`) are skipped
- Missing package dir → warning

## Rollback

Revert changes to `src/manifest.rs`, `src/package.rs`.
