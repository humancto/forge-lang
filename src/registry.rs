use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DEFAULT_REGISTRY_URL: &str = "https://raw.githubusercontent.com/forge-lang/registry/main";
const DEFAULT_CACHE_TTL_SECS: u64 = 3600; // 1 hour

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PackageEntry {
    pub package: PackageMeta,
    #[serde(default)]
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PackageMeta {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub repository: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VersionEntry {
    pub version: String,
    pub url: String,
    #[serde(default)]
    pub checksum: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
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

/// Get the configured registry base URL.
pub fn registry_url() -> String {
    env::var("FORGE_REGISTRY_URL").unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string())
}

/// Get the cache directory for registry data.
/// Uses $HOME/.forge/cache/registry/ so the cache is shared across projects.
fn cache_dir() -> PathBuf {
    if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
        PathBuf::from(home)
            .join(".forge")
            .join("cache")
            .join("registry")
    } else {
        PathBuf::from(".forge").join("cache").join("registry")
    }
}

/// Get the configured cache TTL.
fn cache_ttl() -> Duration {
    let secs = env::var("FORGE_CACHE_TTL")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_CACHE_TTL_SECS);
    Duration::from_secs(secs)
}

/// Check if a cached file is still fresh.
fn is_cache_fresh(path: &Path) -> bool {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let modified = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return false,
    };
    match SystemTime::now().duration_since(modified) {
        Ok(age) => age < cache_ttl(),
        Err(_) => false,
    }
}

/// Write to a file atomically (write to temp, then rename).
fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&temp, content)?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Fetch a package entry from the remote registry.
/// Returns the parsed entry, or None if the package is not found.
/// Uses local cache when fresh.
pub fn fetch_package_entry(name: &str) -> Result<Option<PackageEntry>, String> {
    let cache_path = cache_dir().join(format!("{}.toml", name));

    // Check cache first
    if is_cache_fresh(&cache_path) {
        let content = std::fs::read_to_string(&cache_path)
            .map_err(|e| format!("failed to read cache: {}", e))?;
        let entry: PackageEntry =
            toml::from_str(&content).map_err(|e| format!("corrupt cache for '{}': {}", name, e))?;
        return Ok(Some(entry));
    }

    // Fetch from remote
    let base_url = registry_url();
    let url = format!("{}/packages/{}.toml", base_url, name);

    let mut builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {}", e))?
        .get(&url);

    // Add auth token if available (rate limit mitigation)
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        builder = builder.header("Authorization", format!("token {}", token));
    }

    let response = builder
        .send()
        .map_err(|e| format!("failed to fetch '{}': {}", url, e))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(format!(
            "registry returned {} for '{}'",
            response.status(),
            name
        ));
    }

    let body = response
        .text()
        .map_err(|e| format!("failed to read response: {}", e))?;

    let entry: PackageEntry = toml::from_str(&body)
        .map_err(|e| format!("invalid package entry for '{}': {}", name, e))?;

    // Cache the result atomically
    if let Err(e) = atomic_write(&cache_path, body.as_bytes()) {
        eprintln!(
            "  Warning: failed to cache registry entry for '{}': {}",
            name, e
        );
    }

    Ok(Some(entry))
}

/// Fetch the package index from the remote registry.
/// Lists all available packages with name, description, and latest version.
/// Uses local cache when fresh.
pub fn fetch_index() -> Result<PackageIndex, String> {
    let cache_path = cache_dir().join("index.toml");

    // Check cache first
    if is_cache_fresh(&cache_path) {
        let content = std::fs::read_to_string(&cache_path)
            .map_err(|e| format!("failed to read cached index: {}", e))?;
        let index: PackageIndex =
            toml::from_str(&content).map_err(|e| format!("corrupt cached index: {}", e))?;
        return Ok(index);
    }

    let base_url = registry_url();
    let url = format!("{}/index.toml", base_url);

    let mut builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {}", e))?
        .get(&url);

    if let Ok(token) = env::var("GITHUB_TOKEN") {
        builder = builder.header("Authorization", format!("token {}", token));
    }

    let response = match builder.send() {
        Ok(r) => r,
        Err(e) => {
            // If we have a stale cache, use it on network failure
            if cache_path.exists() {
                eprintln!(
                    "  Warning: failed to fetch index, using cached version: {}",
                    e
                );
                let content = std::fs::read_to_string(&cache_path)
                    .map_err(|e| format!("failed to read cached index: {}", e))?;
                return toml::from_str(&content)
                    .map_err(|e| format!("corrupt cached index: {}", e));
            }
            return Err(format!("failed to fetch package index: {}", e));
        }
    };

    if !response.status().is_success() {
        return Err(format!(
            "registry returned {} for {}",
            response.status(),
            url
        ));
    }

    let body = response
        .text()
        .map_err(|e| format!("failed to read index response: {}", e))?;

    let index: PackageIndex =
        toml::from_str(&body).map_err(|e| format!("invalid package index: {}", e))?;

    if let Err(e) = atomic_write(&cache_path, body.as_bytes()) {
        eprintln!("  Warning: failed to cache index: {}", e);
    }

    Ok(index)
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

/// Resolve the best version from a list of version entries using semver.
pub fn resolve_remote_version(
    name: &str,
    req: &semver::VersionReq,
    versions: &[VersionEntry],
) -> Result<VersionEntry, String> {
    let mut parsed: Vec<(semver::Version, &VersionEntry)> = versions
        .iter()
        .filter_map(|ve| semver::Version::parse(&ve.version).ok().map(|v| (v, ve)))
        .collect();

    if parsed.is_empty() {
        return Err(format!(
            "  Error: no valid versions found for '{}' in remote registry",
            name
        ));
    }

    let best = parsed
        .iter()
        .filter(|(v, _)| req.matches(v))
        .max_by(|(a, _), (b, _)| a.cmp(b));

    match best {
        Some((_, entry)) => Ok((*entry).clone()),
        None => {
            parsed.sort_by(|(a, _), (b, _)| a.cmp(b));
            let available: Vec<&str> = parsed.iter().map(|(_, ve)| ve.version.as_str()).collect();
            Err(format!(
                "  Error: no version of '{}' matches '{}' (available: {})",
                name,
                req,
                available.join(", ")
            ))
        }
    }
}

/// Download a file from a URL to a destination path.
/// Uses atomic write (download to temp, then rename).
pub fn download_to(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directory: {}", e))?;
    }

    let mut builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {}", e))?
        .get(url);

    if let Ok(token) = env::var("GITHUB_TOKEN") {
        builder = builder.header("Authorization", format!("token {}", token));
    }

    let response = builder
        .send()
        .map_err(|e| format!("failed to download '{}': {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "download failed with {} for '{}'",
            response.status(),
            url
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("failed to read download: {}", e))?;

    let temp = dest.with_extension("download");
    std::fs::write(&temp, &bytes).map_err(|e| format!("failed to write temp file: {}", e))?;
    std::fs::rename(&temp, dest).map_err(|e| format!("failed to finalize download: {}", e))?;

    Ok(())
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

/// Download a tarball and extract it to a destination directory.
/// Handles GitHub-style archives that contain a single root directory.
/// Validates tar entries to prevent path traversal attacks.
pub fn download_and_extract(url: &str, dest: &Path, checksum: &str) -> Result<(), String> {
    let temp_dir = dest.with_extension("extracting");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)
            .map_err(|e| format!("failed to clean temp dir: {}", e))?;
    }

    let temp_archive = dest.with_extension("tar.gz");
    download_to(url, &temp_archive)?;

    // Verify checksum if provided
    if !checksum.is_empty() {
        verify_checksum(&temp_archive, checksum)?;
    }

    // Extract the archive with path traversal protection
    let file =
        std::fs::File::open(&temp_archive).map_err(|e| format!("failed to open archive: {}", e))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("failed to create temp dir: {}", e))?;

    // Validate each entry path before extracting
    for entry in archive
        .entries()
        .map_err(|e| format!("failed to read archive entries: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("corrupt archive entry: {}", e))?;
        let path = entry
            .path()
            .map_err(|e| format!("invalid entry path: {}", e))?;

        // Reject absolute paths and path traversal
        let path_str = path.to_string_lossy().to_string();
        if path.is_absolute() || path_str.contains("..") {
            return Err(format!("archive contains unsafe path: {}", path_str));
        }
        drop(path);

        entry
            .unpack_in(&temp_dir)
            .map_err(|e| format!("failed to extract '{}': {}", path_str, e))?;
    }

    // Clean up the archive
    let _ = std::fs::remove_file(&temp_archive);

    // GitHub archives contain a single root directory (e.g., "repo-v1.0.0/")
    // Flatten if there's exactly one subdirectory
    let entries: Vec<_> = std::fs::read_dir(&temp_dir)
        .map_err(|e| format!("failed to read extracted dir: {}", e))?
        .filter_map(|e| e.ok())
        .collect();

    if dest.exists() {
        std::fs::remove_dir_all(dest)
            .map_err(|e| format!("failed to remove existing package: {}", e))?;
    }

    if entries.len() == 1 && entries[0].file_type().map_or(false, |t| t.is_dir()) {
        // Single root directory — move it to dest
        std::fs::rename(entries[0].path(), dest)
            .map_err(|e| format!("failed to move extracted package: {}", e))?;
        let _ = std::fs::remove_dir_all(&temp_dir);
    } else {
        // Multiple entries or flat — rename the temp dir itself
        std::fs::rename(&temp_dir, dest)
            .map_err(|e| format!("failed to move extracted package: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_package_entry() {
        let toml_str = r#"
[package]
name = "router"
description = "HTTP router for Forge"
repository = "https://github.com/user/forge-router"

[[versions]]
version = "1.0.0"
url = "https://example.com/router-1.0.0.tar.gz"
checksum = "sha256:abc123"

[[versions]]
version = "2.0.0"
url = "https://example.com/router-2.0.0.tar.gz"
checksum = "sha256:def456"
"#;
        let entry: PackageEntry = toml::from_str(toml_str).unwrap();
        assert_eq!(entry.package.name, "router");
        assert_eq!(entry.package.description, "HTTP router for Forge");
        assert_eq!(entry.versions.len(), 2);
        assert_eq!(entry.versions[0].version, "1.0.0");
        assert_eq!(entry.versions[1].version, "2.0.0");
        assert_eq!(entry.versions[0].checksum, "sha256:abc123");
    }

    #[test]
    fn parse_minimal_package_entry() {
        let toml_str = r#"
[package]
name = "utils"

[[versions]]
version = "0.1.0"
url = "https://example.com/utils.tar.gz"
"#;
        let entry: PackageEntry = toml::from_str(toml_str).unwrap();
        assert_eq!(entry.package.name, "utils");
        assert_eq!(entry.package.description, "");
        assert_eq!(entry.versions.len(), 1);
        assert_eq!(entry.versions[0].checksum, "");
    }

    #[test]
    fn resolve_remote_caret() {
        let versions = vec![
            VersionEntry {
                version: "1.0.0".into(),
                url: "url1".into(),
                checksum: String::new(),
            },
            VersionEntry {
                version: "1.5.0".into(),
                url: "url2".into(),
                checksum: String::new(),
            },
            VersionEntry {
                version: "2.0.0".into(),
                url: "url3".into(),
                checksum: String::new(),
            },
        ];

        let req = semver::VersionReq::parse("^1.0").unwrap();
        let resolved = resolve_remote_version("test", &req, &versions).unwrap();
        assert_eq!(resolved.version, "1.5.0");
        assert_eq!(resolved.url, "url2");
    }

    #[test]
    fn resolve_remote_no_match() {
        let versions = vec![VersionEntry {
            version: "1.0.0".into(),
            url: "url1".into(),
            checksum: String::new(),
        }];

        let req = semver::VersionReq::parse("^3.0").unwrap();
        let err = resolve_remote_version("test", &req, &versions).unwrap_err();
        assert!(err.contains("no version of 'test' matches"));
        assert!(err.contains("1.0.0"));
    }

    #[test]
    fn resolve_remote_star() {
        let versions = vec![
            VersionEntry {
                version: "1.0.0".into(),
                url: "url1".into(),
                checksum: String::new(),
            },
            VersionEntry {
                version: "3.0.0".into(),
                url: "url3".into(),
                checksum: String::new(),
            },
        ];

        let resolved =
            resolve_remote_version("test", &semver::VersionReq::STAR, &versions).unwrap();
        assert_eq!(resolved.version, "3.0.0");
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
        assert!(is_cache_fresh(&file));

        // Non-existent file — not fresh
        assert!(!is_cache_fresh(&temp.join("nonexistent")));

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
