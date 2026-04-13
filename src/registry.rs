use serde::{Deserialize, Serialize};
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

/// Get the configured registry base URL.
pub fn registry_url() -> String {
    env::var("FORGE_REGISTRY_URL").unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string())
}

/// Get the cache directory for registry data.
fn cache_dir() -> PathBuf {
    PathBuf::from(".forge").join("cache").join("registry")
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
    let temp = path.with_extension("tmp");
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

/// Download a tarball and extract it to a destination directory.
/// Handles GitHub-style archives that contain a single root directory.
pub fn download_and_extract(url: &str, dest: &Path) -> Result<(), String> {
    let temp_dir = dest.with_extension("extracting");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)
            .map_err(|e| format!("failed to clean temp dir: {}", e))?;
    }

    let temp_archive = dest.with_extension("tar.gz");
    download_to(url, &temp_archive)?;

    // Extract the archive
    let file =
        std::fs::File::open(&temp_archive).map_err(|e| format!("failed to open archive: {}", e))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("failed to create temp dir: {}", e))?;

    archive
        .unpack(&temp_dir)
        .map_err(|e| format!("failed to extract archive: {}", e))?;

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
}
