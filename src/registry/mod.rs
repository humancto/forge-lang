//! Package registry client (rfcs/0007-package-registry.md).
//!
//! * [`index`] — the sparse index format (pure; shared by install and publish)
//! * [`client`] — fetching index files and archives (cache, ETag, offline)
//! * [`signing`] — ed25519 publisher signatures and trust-on-first-use pins
//! * this module — search over local registries + the remote summary, and
//!   safe archive extraction.

pub mod client;
pub mod index;
pub mod signing;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub use client::registry_url;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct PackageIndex {
    #[serde(default)]
    pub packages: Vec<PackageSummary>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PackageSummary {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub latest: String,
}

/// `~/.forge` (or `./.forge` when no home directory is known).
pub fn forge_home() -> PathBuf {
    match env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
        Ok(home) => PathBuf::from(home).join(".forge"),
        Err(_) => PathBuf::from(".forge"),
    }
}

/// True when `path` exists and was modified less than `ttl` ago.
fn is_fresh(path: &Path, ttl: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < ttl)
}

/// Write to a file atomically (write to temp, then rename).
fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let temp = path.with_file_name(format!(".{}.tmp.{}", file_name, std::process::id()));
    std::fs::write(&temp, content)?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Fetch the remote search summary (`index.toml`) of the configured registry.
pub fn fetch_index() -> Result<PackageIndex, String> {
    client::RegistryClient::from_env().search_index()
}

/// Search packages by case-insensitive substring match on name or description.
/// Empty query returns all packages.
pub fn search_packages<'a>(query: &str, index: &'a PackageIndex) -> Vec<&'a PackageSummary> {
    index
        .packages
        .iter()
        .filter(|p| matches_query(query, p))
        .collect()
}

/// Where a search hit was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageOrigin {
    /// A local registry directory (e.g. `~/.forge/registry`).
    Local(PathBuf),
    /// The remote registry index.
    Remote,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub summary: PackageSummary,
    pub origin: PackageOrigin,
}

/// Result of searching local registries plus (optionally) the remote index.
#[derive(Debug, Clone)]
pub struct SearchReport {
    pub hits: Vec<SearchHit>,
    /// Set when the remote index could not be fetched; local hits are still
    /// reported.
    pub remote_error: Option<String>,
}

/// Build an index of the packages published to local registry roots.
///
/// The layout is the one `forge publish` writes: `<root>/<name>/<semver>/`.
/// The description comes from the newest version's `forge.toml`. Roots that
/// do not exist are ignored. When a package appears in several roots, the
/// first root wins for the description and the newest version overall is
/// reported as `latest`.
pub fn local_index(roots: &[PathBuf]) -> Vec<(PackageSummary, PathBuf)> {
    let mut found: Vec<(PackageSummary, PathBuf, semver::Version)> = Vec::new();
    for root in roots {
        let Ok(packages) = std::fs::read_dir(root) else {
            continue;
        };
        let mut names: Vec<_> = packages
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .collect();
        names.sort_by_key(|e| e.file_name());
        for pkg in names {
            let name = pkg.file_name().to_string_lossy().to_string();
            let Ok(versions) = std::fs::read_dir(pkg.path()) else {
                continue;
            };
            let newest = versions
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter_map(|e| {
                    semver::Version::parse(&e.file_name().to_string_lossy())
                        .ok()
                        .map(|v| (v, e.path()))
                })
                .max_by(|a, b| a.0.cmp(&b.0));
            let Some((version, version_dir)) = newest else {
                continue;
            };
            let description = crate::manifest::load_manifest_from(&version_dir.join("forge.toml"))
                .map(|m| m.project.description)
                .unwrap_or_default();
            if let Some(existing) = found.iter_mut().find(|(s, _, _)| s.name == name) {
                if version > existing.2 {
                    existing.0.latest = version.to_string();
                    existing.2 = version;
                }
                continue;
            }
            found.push((
                PackageSummary {
                    name,
                    description,
                    latest: version.to_string(),
                },
                root.clone(),
                version,
            ));
        }
    }
    found.into_iter().map(|(s, root, _)| (s, root)).collect()
}

/// Search local registries and the remote index together.
///
/// Local packages are listed first and shadow remote packages with the same
/// name (installs also prefer the local registry). A remote failure is
/// reported in `remote_error` instead of aborting the search.
pub fn search_all(
    query: &str,
    local_roots: &[PathBuf],
    remote: Result<PackageIndex, String>,
) -> SearchReport {
    let mut hits: Vec<SearchHit> = Vec::new();
    for (summary, root) in local_index(local_roots) {
        if matches_query(query, &summary) {
            hits.push(SearchHit {
                summary,
                origin: PackageOrigin::Local(root),
            });
        }
    }
    let remote_error = match remote {
        Ok(index) => {
            for summary in search_packages(query, &index) {
                if hits.iter().any(|h| h.summary.name == summary.name) {
                    continue;
                }
                hits.push(SearchHit {
                    summary: summary.clone(),
                    origin: PackageOrigin::Remote,
                });
            }
            None
        }
        Err(e) => Some(e),
    };
    SearchReport { hits, remote_error }
}

/// Print a search report for `forge search`. Returns the process exit code:
/// non-zero only when the remote registry was unreachable and nothing was
/// found locally either.
pub fn print_search_report(query: &str, report: &SearchReport, local_roots: &[PathBuf]) -> i32 {
    if let Some(ref err) = report.remote_error {
        eprintln!(
            "  Warning: remote registry {} is unreachable: {}",
            registry_url(),
            err
        );
        eprintln!("  Showing packages from the local registry only. Set FORGE_REGISTRY_URL to use a different registry.");
    }

    if report.hits.is_empty() {
        let scope = if report.remote_error.is_some() {
            "the local registry"
        } else {
            "the registry"
        };
        if query.is_empty() {
            println!("No packages found in {}.", scope);
        } else {
            println!("No packages found matching '{}' in {}.", query, scope);
        }
        if report.remote_error.is_some() {
            let searched: Vec<String> = local_roots
                .iter()
                .map(|r| r.display().to_string())
                .collect();
            println!("  Searched local registries: {}", searched.join(", "));
            println!("  Publish a package locally with `forge publish`.");
            return 1;
        }
        return 0;
    }

    println!(
        "{:<20} {:<10} {:<8} DESCRIPTION",
        "NAME", "VERSION", "SOURCE"
    );
    println!("{}", "-".repeat(68));
    for hit in &report.hits {
        let pkg = &hit.summary;
        let source = match hit.origin {
            PackageOrigin::Local(_) => "local",
            PackageOrigin::Remote => "remote",
        };
        println!(
            "{:<20} {:<10} {:<8} {}",
            pkg.name,
            if pkg.latest.is_empty() {
                "-"
            } else {
                &pkg.latest
            },
            source,
            pkg.description
        );
    }
    println!("\n{} package(s) found.", report.hits.len());
    0
}

fn matches_query(query: &str, p: &PackageSummary) -> bool {
    let q = query.to_lowercase();
    q.is_empty() || p.name.to_lowercase().contains(&q) || p.description.to_lowercase().contains(&q)
}

/// Verify a file's SHA-256 checksum against an expected value.
/// Checksum format: "sha256:<hex>" or plain hex.
pub fn verify_checksum(path: &Path, expected: &str) -> Result<(), String> {
    let expected_hex = expected.strip_prefix("sha256:").unwrap_or(expected);

    let data =
        std::fs::read(path).map_err(|e| format!("failed to read file for checksum: {}", e))?;

    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual_hex = format!("{:x}", hasher.finalize());

    if actual_hex != expected_hex {
        return Err(format!(
            "checksum mismatch: expected {}, got {}",
            expected_hex, actual_hex
        ));
    }
    Ok(())
}

/// Extract a (checksum-verified) `.tar.gz` package archive into `dest`,
/// replacing whatever is there.
///
/// Only regular files and directories are accepted: absolute paths, `..`
/// components, symlinks, hard links and device nodes are rejected before
/// anything is moved into place. An archive with a single top-level
/// directory (the `<name>-<version>/` layout `forge publish` and GitHub
/// produce) is flattened into `dest`.
pub fn extract_archive(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let temp_dir = dest.with_extension("extracting");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)
            .map_err(|e| format!("failed to clean temp dir: {}", e))?;
    }
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("failed to create temp dir: {}", e))?;

    if let Err(e) = unpack_checked(archive_path, &temp_dir) {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(e);
    }

    let entries: Vec<_> = std::fs::read_dir(&temp_dir)
        .map_err(|e| format!("failed to read extracted dir: {}", e))?
        .filter_map(|e| e.ok())
        .collect();

    if dest.exists() {
        std::fs::remove_dir_all(dest)
            .map_err(|e| format!("failed to remove existing package: {}", e))?;
    }

    if entries.len() == 1 && entries[0].file_type().is_ok_and(|t| t.is_dir()) {
        std::fs::rename(entries[0].path(), dest)
            .map_err(|e| format!("failed to move extracted package: {}", e))?;
        let _ = std::fs::remove_dir_all(&temp_dir);
    } else {
        std::fs::rename(&temp_dir, dest)
            .map_err(|e| format!("failed to move extracted package: {}", e))?;
    }
    Ok(())
}

fn unpack_checked(archive_path: &Path, into: &Path) -> Result<(), String> {
    let file =
        std::fs::File::open(archive_path).map_err(|e| format!("failed to open archive: {}", e))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in archive
        .entries()
        .map_err(|e| format!("failed to read archive entries: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("corrupt archive entry: {}", e))?;
        let path = entry
            .path()
            .map_err(|e| format!("invalid entry path: {}", e))?
            .into_owned();
        let path_str = path.to_string_lossy().to_string();
        let unsafe_path = path.is_absolute()
            || path.components().any(|c| {
                !matches!(
                    c,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            });
        if unsafe_path {
            return Err(format!("archive contains unsafe path: {}", path_str));
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(format!(
                "archive entry '{}' is not a regular file or directory",
                path_str
            ));
        }
        entry
            .unpack_in(into)
            .map_err(|e| format!("failed to extract '{}': {}", path_str, e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "forge-extract-{}-{}-{}",
            tag,
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Build a .tar.gz with raw headers so hostile entries can be expressed.
    fn tarball(path: &Path, entries: &[(&str, tar::EntryType, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        for (name, kind, data) in entries {
            let mut header = tar::Header::new_gnu();
            {
                let raw = header.as_old_mut();
                raw.name[..name.len()].copy_from_slice(name.as_bytes());
            }
            header.set_entry_type(*kind);
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            if *kind == tar::EntryType::Symlink {
                header.set_link_name("/etc/passwd").unwrap();
            }
            header.set_cksum();
            builder.append(&header, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn extract_flattens_single_root_directory() {
        let root = temp_root("flatten");
        let archive = root.join("pkg.tar.gz");
        tarball(
            &archive,
            &[
                (
                    "kv-0.1.0/forge.toml",
                    tar::EntryType::Regular,
                    b"[project]\n",
                ),
                ("kv-0.1.0/main.fg", tar::EntryType::Regular, b"say 1\n"),
            ],
        );
        let dest = root.join("forge_modules").join("kv");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("stale.fg"), "old").unwrap();
        extract_archive(&archive, &dest).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("main.fg")).unwrap(),
            "say 1\n"
        );
        assert!(!dest.join("stale.fg").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_rejects_traversal_and_links() {
        let root = temp_root("hostile");
        for (i, entry) in [
            ("../escape.fg", tar::EntryType::Regular),
            ("pkg/../../escape.fg", tar::EntryType::Regular),
            ("pkg/link", tar::EntryType::Symlink),
            ("pkg/dev", tar::EntryType::Char),
        ]
        .into_iter()
        .enumerate()
        {
            let archive = root.join(format!("bad{}.tar.gz", i));
            tarball(&archive, &[(entry.0, entry.1, b"")]);
            let dest = root.join(format!("out{}", i));
            let err = extract_archive(&archive, &dest).unwrap_err();
            assert!(
                err.contains("unsafe path") || err.contains("not a regular file"),
                "{}: {}",
                entry.0,
                err
            );
            assert!(!dest.exists(), "nothing is installed on failure");
        }
        assert!(!root.parent().unwrap().join("escape.fg").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cache_ttl_check() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp = std::env::temp_dir().join(format!("forge-cache-test-{}", unique));
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("test.toml");

        // Write a file — should be fresh
        std::fs::write(&file, "test").unwrap();
        assert!(is_fresh(&file, Duration::from_secs(3600)));

        // Non-existent file — not fresh
        assert!(!is_fresh(
            &temp.join("nonexistent"),
            Duration::from_secs(3600)
        ));
        assert!(!is_fresh(&file, Duration::ZERO));

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn atomic_write_creates_dirs() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir()
            .join(format!("forge-atomic-{}", unique))
            .join("sub")
            .join("test.txt");

        atomic_write(&path, b"hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");

        // Clean up
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn checksum_verification_pass() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("forge-checksum-{}", unique));
        std::fs::write(&path, b"hello world").unwrap();

        // SHA-256 of "hello world"
        let expected = "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        verify_checksum(&path, expected).unwrap();

        // Also works without prefix
        verify_checksum(
            &path,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
        )
        .unwrap();

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn checksum_verification_fail() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("forge-checksum-fail-{}", unique));
        std::fs::write(&path, b"hello world").unwrap();

        let err = verify_checksum(&path, "sha256:0000000000000000").unwrap_err();
        assert!(err.contains("checksum mismatch"));

        std::fs::remove_file(&path).unwrap();
    }

    fn test_index() -> PackageIndex {
        PackageIndex {
            packages: vec![
                PackageSummary {
                    name: "router".into(),
                    description: "HTTP router for Forge".into(),
                    latest: "2.0.0".into(),
                },
                PackageSummary {
                    name: "auth".into(),
                    description: "JWT authentication library".into(),
                    latest: "1.0.0".into(),
                },
                PackageSummary {
                    name: "csv-utils".into(),
                    description: "CSV parsing utilities".into(),
                    latest: "0.5.0".into(),
                },
            ],
        }
    }

    #[test]
    fn search_by_name() {
        let index = test_index();
        let results = search_packages("router", &index);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "router");
    }

    #[test]
    fn search_by_description() {
        let index = test_index();
        let results = search_packages("JWT", &index);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "auth");
    }

    #[test]
    fn search_case_insensitive() {
        let index = test_index();
        let results = search_packages("ROUTER", &index);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "router");
    }

    #[test]
    fn search_no_match() {
        let index = test_index();
        let results = search_packages("nonexistent", &index);
        assert!(results.is_empty());
    }

    #[test]
    fn search_empty_query_returns_all() {
        let index = test_index();
        let results = search_packages("", &index);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn search_partial_match() {
        let index = test_index();
        // "csv" matches name "csv-utils" and description "CSV parsing utilities"
        let results = search_packages("csv", &index);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "csv-utils");
    }

    #[test]
    fn parse_package_index() {
        let toml_str = r#"
[[packages]]
name = "router"
description = "HTTP router"
latest = "1.0.0"

[[packages]]
name = "auth"
description = "Auth library"
latest = "2.0.0"
"#;
        let index: PackageIndex = toml::from_str(toml_str).unwrap();
        assert_eq!(index.packages.len(), 2);
        assert_eq!(index.packages[0].name, "router");
        assert_eq!(index.packages[1].latest, "2.0.0");
    }

    fn local_registry(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("forge-search-{}-{}", tag, unique));
        for (name, version, desc) in [
            ("router", "1.0.0", "Old router"),
            ("router", "1.2.0", "Local HTTP router"),
            ("kv", "0.1.0", "Tiny key-value store"),
        ] {
            let dir = root.join(name).join(version);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("forge.toml"),
                format!(
                    "[project]\nname = \"{}\"\nversion = \"{}\"\ndescription = \"{}\"\n",
                    name, version, desc
                ),
            )
            .unwrap();
        }
        // Non-semver directories are ignored.
        std::fs::create_dir_all(root.join("junk").join("not-a-version")).unwrap();
        root
    }

    #[test]
    fn local_index_reports_newest_version_and_description() {
        let root = local_registry("index");
        let index = local_index(&[root.clone(), root.join("missing")]);
        let names: Vec<&str> = index.iter().map(|(s, _)| s.name.as_str()).collect();
        assert_eq!(names, vec!["kv", "router"]);
        let router = &index[1].0;
        assert_eq!(router.latest, "1.2.0");
        assert_eq!(router.description, "Local HTTP router");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn search_falls_back_to_local_when_remote_unreachable() {
        let root = local_registry("offline");
        let report = search_all(
            "router",
            std::slice::from_ref(&root),
            Err("registry returned 404 Not Found".into()),
        );
        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.hits[0].summary.name, "router");
        assert_eq!(report.hits[0].origin, PackageOrigin::Local(root.clone()));
        assert!(report.remote_error.unwrap().contains("404"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn search_merges_local_and_remote_with_local_shadowing() {
        let root = local_registry("merge");
        let report = search_all("", std::slice::from_ref(&root), Ok(test_index()));
        let names: Vec<(&str, bool)> = report
            .hits
            .iter()
            .map(|h| {
                (
                    h.summary.name.as_str(),
                    matches!(h.origin, PackageOrigin::Local(_)),
                )
            })
            .collect();
        assert_eq!(
            names,
            vec![
                ("kv", true),
                ("router", true),
                ("auth", false),
                ("csv-utils", false)
            ]
        );
        assert!(report.remote_error.is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn search_report_exit_code_only_fails_when_nothing_is_reachable() {
        let empty = SearchReport {
            hits: Vec::new(),
            remote_error: Some("offline".into()),
        };
        assert_eq!(print_search_report("x", &empty, &[]), 1);
        let ok_empty = SearchReport {
            hits: Vec::new(),
            remote_error: None,
        };
        assert_eq!(print_search_report("x", &ok_empty, &[]), 0);
    }
}
