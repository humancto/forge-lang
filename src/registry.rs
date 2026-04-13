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
        return Err(format!("registry returned {} for index", response.status()));
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
    let query_lower = query.to_lowercase();
    index
        .packages
        .iter()
        .filter(|p| {
            if query_lower.is_empty() {
                return true;
            }
            p.name.to_lowercase().contains(&query_lower)
                || p.description.to_lowercase().contains(&query_lower)
        })
        .collect()
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
}
