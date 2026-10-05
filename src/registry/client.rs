//! Sparse-index client: fetches individual index files over HTTP(S) or from
//! a `file://` registry, with an on-disk cache revalidated by ETag, an
//! offline mode, and a checksum-verified archive cache (rfcs/0007).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use super::index::{self, IndexConfig, IndexEntry};
use super::PackageIndex;

/// The registry used when `FORGE_REGISTRY_URL` is unset. The repository is
/// created by the maintainer from `tools/registry-template/`.
pub const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/humancto/forge-registry/main";
const DEFAULT_CACHE_TTL_SECS: u64 = 300;
/// Upper bounds on what a registry may send us.
const MAX_INDEX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;

/// Hosts that may receive `GITHUB_TOKEN` (rate-limit relief). The token is
/// never sent to any other registry or mirror.
const GITHUB_HOSTS: &[&str] = &[
    "raw.githubusercontent.com",
    "github.com",
    "objects.githubusercontent.com",
    "api.github.com",
];

/// What fetching one registry file produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    Body(Vec<u8>),
    NotFound,
}

pub struct RegistryClient {
    base: String,
    cache_root: PathBuf,
    archive_cache: PathBuf,
    ttl: Duration,
    offline: bool,
    http: std::sync::OnceLock<Result<reqwest::blocking::Client, String>>,
}

fn truthy(var: &str) -> bool {
    std::env::var(var)
        .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// `FORGE_REGISTRY_URL`, else the default hosted registry.
pub fn registry_url() -> String {
    std::env::var("FORGE_REGISTRY_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_REGISTRY_URL.to_string())
        .trim()
        .trim_end_matches('/')
        .to_string()
}

/// Whether unsigned packages are refused (`FORGE_REQUIRE_SIGNATURES=1`).
pub fn require_signatures() -> bool {
    truthy("FORGE_REQUIRE_SIGNATURES")
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{:02x}", b).expect("BUG: writing to String is infallible");
    }
    s
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Turn a `file://` URL into a path (`file:///abs/path` or `file://./rel`).
fn file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    Some(PathBuf::from(rest))
}

impl RegistryClient {
    /// A client configured from the environment: `FORGE_REGISTRY_URL`,
    /// `FORGE_CACHE_TTL` (seconds), `FORGE_OFFLINE`, caches under
    /// `~/.forge/cache/`.
    pub fn from_env() -> Self {
        let ttl = std::env::var("FORGE_CACHE_TTL")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_CACHE_TTL_SECS);
        let cache = super::forge_home().join("cache");
        Self::new(
            &registry_url(),
            &cache.join("registry"),
            &cache.join("archives"),
            Duration::from_secs(ttl),
            truthy("FORGE_OFFLINE"),
        )
    }

    /// `cache_base` gets one subdirectory per registry URL.
    pub fn new(
        base: &str,
        cache_base: &Path,
        archive_cache: &Path,
        ttl: Duration,
        offline: bool,
    ) -> Self {
        let base = base.trim().trim_end_matches('/').to_string();
        let host = url::Url::parse(&base)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "local".to_string());
        let host: String = host
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let key = format!("{}-{}", host, &sha256_hex(base.as_bytes())[..12]);
        RegistryClient {
            cache_root: cache_base.join(key),
            archive_cache: archive_cache.to_path_buf(),
            base,
            ttl,
            offline,
            http: std::sync::OnceLock::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn http(&self) -> Result<&reqwest::blocking::Client, String> {
        self.http
            .get_or_init(|| {
                reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(60))
                    .user_agent(concat!("forge/", env!("CARGO_PKG_VERSION")))
                    .build()
                    .map_err(|e| format!("failed to create HTTP client: {}", e))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    fn get(&self, url: &str) -> Result<reqwest::blocking::RequestBuilder, String> {
        let mut req = self.http()?.get(url);
        if let Ok(parsed) = url::Url::parse(url) {
            let github = parsed.scheme() == "https"
                && parsed.host_str().is_some_and(|h| GITHUB_HOSTS.contains(&h));
            if github {
                if let Ok(token) = std::env::var("GITHUB_TOKEN") {
                    req = req.header("Authorization", format!("token {}", token));
                }
            }
        }
        Ok(req)
    }

    /// Fetch `rel` (a path relative to the registry root).
    ///
    /// HTTP: a cached copy younger than the TTL is used as is; an older one
    /// is revalidated with `If-None-Match` (a `304` refreshes it); on network
    /// failure a cached copy is used with a warning. `FORGE_OFFLINE` uses the
    /// cache only. `file://` registries are read directly, uncached.
    pub fn fetch_file(&self, rel: &str) -> Result<Fetched, String> {
        if let Some(root) = file_url_path(&self.base) {
            let path = root.join(rel);
            return match std::fs::read(&path) {
                Ok(bytes) => Ok(Fetched::Body(bytes)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Fetched::NotFound),
                Err(e) => Err(format!("failed to read {}: {}", path.display(), e)),
            };
        }

        let cached = self.cache_root.join(rel);
        let etag_path = PathBuf::from(format!("{}.etag", cached.display()));
        let cached_body = std::fs::read(&cached).ok();

        if let Some(body) = &cached_body {
            if self.offline || super::is_fresh(&cached, self.ttl) {
                return Ok(Fetched::Body(body.clone()));
            }
        } else if self.offline {
            return Err(format!(
                "FORGE_OFFLINE is set and {} is not cached for {}",
                rel, self.base
            ));
        }

        let url = format!("{}/{}", self.base, rel);
        let mut req = self.get(&url)?;
        if cached_body.is_some() {
            if let Ok(etag) = std::fs::read_to_string(&etag_path) {
                req = req.header(reqwest::header::IF_NONE_MATCH, etag.trim());
            }
        }

        let stale = |why: String| -> Result<Fetched, String> {
            match &cached_body {
                Some(body) => {
                    crate::color::ceprintln!("  Warning: {}; using cached copy of {}", why, rel);
                    Ok(Fetched::Body(body.clone()))
                }
                None => Err(why),
            }
        };

        let response = match req.send() {
            Ok(r) => r,
            Err(e) => return stale(format!("registry {} is unreachable: {}", self.base, e)),
        };
        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            if let Some(body) = cached_body {
                touch(&cached);
                return Ok(Fetched::Body(body));
            }
        }
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
            let _ = std::fs::remove_file(&cached);
            let _ = std::fs::remove_file(&etag_path);
            return Ok(Fetched::NotFound);
        }
        if !status.is_success() {
            return stale(format!("registry returned {} for {}", status, url));
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = match read_limited(response, MAX_INDEX_FILE_BYTES) {
            Ok(b) => b,
            Err(e) => return stale(format!("failed to read {}: {}", url, e)),
        };
        if let Err(e) = super::atomic_write(&cached, &body) {
            crate::color::ceprintln!("  Warning: failed to cache {}: {}", rel, e);
        } else {
            match etag {
                Some(tag) => {
                    let _ = super::atomic_write(&etag_path, tag.as_bytes());
                }
                None => {
                    let _ = std::fs::remove_file(&etag_path);
                }
            }
        }
        Ok(Fetched::Body(body))
    }

    /// The registry's `config.json`. A missing config means the URL is not a
    /// Forge registry (or the hosted index has not been created yet).
    pub fn config(&self) -> Result<IndexConfig, String> {
        match self.fetch_file(index::CONFIG_FILE)? {
            Fetched::Body(bytes) => {
                let config: IndexConfig = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("{}/config.json is invalid: {}", self.base, e))?;
                if config.v > index::INDEX_FORMAT_VERSION {
                    return Err(format!(
                        "registry {} uses index format v{}; this forge understands v{}. Upgrade forge.",
                        self.base, config.v, index::INDEX_FORMAT_VERSION
                    ));
                }
                Ok(config)
            }
            Fetched::NotFound => Err(self.not_a_registry()),
        }
    }

    fn not_a_registry(&self) -> String {
        let mut msg = format!(
            "{} is not a Forge registry (no config.json found)",
            self.base
        );
        if self.base == DEFAULT_REGISTRY_URL {
            msg.push_str(
                ".\n  The default hosted registry (github.com/humancto/forge-registry) has not \
                 been published yet.\n  Use a local registry (`forge publish`, FORGE_REGISTRY_PATH) \
                 or point FORGE_REGISTRY_URL at a mirror.",
            );
        } else {
            msg.push_str(". Check FORGE_REGISTRY_URL.");
        }
        msg
    }

    /// All published versions of `name`, or `None` if the package does not
    /// exist in this registry.
    pub fn entries(&self, name: &str) -> Result<Option<Vec<IndexEntry>>, String> {
        index::validate_name(name)?;
        match self.fetch_file(&index::index_path(name))? {
            Fetched::Body(bytes) => {
                let text = String::from_utf8(bytes)
                    .map_err(|_| format!("index file for '{}' is not UTF-8", name))?;
                index::parse_entries(name, &text).map(Some)
            }
            Fetched::NotFound => Ok(None),
        }
    }

    /// The search summary (`index.toml`).
    pub fn search_index(&self) -> Result<PackageIndex, String> {
        match self.fetch_file(index::SEARCH_INDEX_FILE)? {
            Fetched::Body(bytes) => {
                let text =
                    String::from_utf8(bytes).map_err(|_| "index.toml is not UTF-8".to_string())?;
                toml::from_str(&text).map_err(|e| format!("invalid package index: {}", e))
            }
            Fetched::NotFound => Err(self.not_a_registry()),
        }
    }

    /// Path of the checksum-verified archive for `entry`, downloading it
    /// into the archive cache if needed. Archives are content-addressed by
    /// checksum, so a cached one is valid offline and across registries.
    pub fn fetch_archive(
        &self,
        config: &IndexConfig,
        entry: &IndexEntry,
    ) -> Result<PathBuf, String> {
        if !index::is_sha256_hex(&entry.cksum) {
            return Err(format!(
                "{}@{} has no valid checksum; refusing to install",
                entry.name, entry.vers
            ));
        }
        let dest = self.archive_cache.join(format!(
            "{}-{}-{}.tar.gz",
            entry.name,
            entry.vers,
            &entry.cksum[..16]
        ));
        if dest.exists() {
            if super::verify_checksum(&dest, &entry.cksum).is_ok() {
                return Ok(dest);
            }
            let _ = std::fs::remove_file(&dest);
        }
        let url = index::archive_url(config, entry);
        let bytes = if let Some(path) = file_url_path(&url) {
            std::fs::read(&path).map_err(|e| format!("failed to read {}: {}", path.display(), e))?
        } else {
            if self.offline {
                return Err(format!(
                    "FORGE_OFFLINE is set and {}@{} is not in the archive cache",
                    entry.name, entry.vers
                ));
            }
            let response = self
                .get(&url)?
                .send()
                .map_err(|e| format!("failed to download {}: {}", url, e))?;
            if !response.status().is_success() {
                return Err(format!(
                    "download of {} failed with {}",
                    url,
                    response.status()
                ));
            }
            read_limited(response, MAX_ARCHIVE_BYTES)
                .map_err(|e| format!("failed to download {}: {}", url, e))?
        };
        let actual = sha256_hex(&bytes);
        if actual != entry.cksum {
            return Err(format!(
                "checksum mismatch for {}@{} from {}: index says {}, archive is {}",
                entry.name, entry.vers, url, entry.cksum, actual
            ));
        }
        super::atomic_write(&dest, &bytes)
            .map_err(|e| format!("failed to cache archive {}: {}", dest.display(), e))?;
        Ok(dest)
    }
}

fn read_limited(response: reqwest::blocking::Response, limit: u64) -> Result<Vec<u8>, String> {
    if response.content_length().is_some_and(|n| n > limit) {
        return Err(format!("response larger than {} bytes", limit));
    }
    let mut body = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > limit {
        return Err(format!("response larger than {} bytes", limit));
    }
    Ok(body)
}

/// Mark a cache file as fresh (after a `304 Not Modified`).
fn touch(path: &Path) {
    if let Ok(file) = std::fs::File::options().append(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "forge-client-{}-{}-{}",
            tag,
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn client(base: &str, cache: &Path, offline: bool) -> RegistryClient {
        RegistryClient::new(
            base,
            &cache.join("registry"),
            &cache.join("archives"),
            Duration::from_secs(0),
            offline,
        )
    }

    #[test]
    fn file_registry_reads_config_entries_and_archives() {
        let root = temp("file");
        let reg = root.join("index-repo");
        std::fs::create_dir_all(reg.join("index/ro/ut")).unwrap();
        std::fs::write(reg.join("config.json"), "{\"v\":1}").unwrap();
        let archive = root.join("router-1.0.0.tar.gz");
        std::fs::write(&archive, b"pretend tarball").unwrap();
        let entry = IndexEntry {
            v: 1,
            name: "router".into(),
            vers: "1.0.0".into(),
            deps: Vec::new(),
            cksum: sha256_hex(b"pretend tarball"),
            url: format!("file://{}", archive.display()),
            yanked: false,
            pubkey: None,
            sig: None,
            description: String::new(),
            license: String::new(),
            published: String::new(),
        };
        std::fs::write(
            reg.join("index/ro/ut/router"),
            index::serialize_entries(std::slice::from_ref(&entry)),
        )
        .unwrap();

        let c = client(
            &format!("file://{}/", reg.display()),
            &root.join("cache"),
            false,
        );
        let config = c.config().unwrap();
        assert_eq!(c.entries("router").unwrap().unwrap(), vec![entry.clone()]);
        assert_eq!(c.entries("missing").unwrap(), None);
        let cached = c.fetch_archive(&config, &entry).unwrap();
        assert_eq!(std::fs::read(&cached).unwrap(), b"pretend tarball");

        // Tampered source archive: the cached, verified copy is still used.
        std::fs::write(&archive, b"evil").unwrap();
        assert_eq!(c.fetch_archive(&config, &entry).unwrap(), cached);
        // Without the cache, the mismatch is detected.
        std::fs::remove_file(&cached).unwrap();
        let err = c.fetch_archive(&config, &entry).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_config_explains_itself() {
        let root = temp("noconfig");
        let c = client(
            &format!("file://{}", root.display()),
            &root.join("cache"),
            false,
        );
        let err = c.config().unwrap_err();
        assert!(err.contains("not a Forge registry"), "{err}");
        let d = client(DEFAULT_REGISTRY_URL, &root.join("cache"), true);
        assert!(d.not_a_registry().contains("has not been published yet"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn offline_mode_uses_only_the_cache() {
        let root = temp("offline");
        let c = client("https://registry.invalid", &root.join("cache"), true);
        let err = c.fetch_file("config.json").unwrap_err();
        assert!(err.contains("FORGE_OFFLINE"), "{err}");
        let cached = c.cache_root.join("config.json");
        std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
        std::fs::write(&cached, "{\"v\":1}").unwrap();
        assert_eq!(c.config().unwrap(), IndexConfig::default());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cache_dirs_are_per_registry() {
        let root = temp("percache");
        let a = client("https://a.example/reg", &root, false);
        let b = client("https://b.example/reg/", &root, false);
        assert_ne!(a.cache_root, b.cache_root);
        assert_eq!(a.base_url(), "https://a.example/reg");
        assert_eq!(b.base_url(), "https://b.example/reg");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn newer_index_format_is_rejected_with_upgrade_hint() {
        let root = temp("future");
        std::fs::write(root.join("config.json"), "{\"v\":99}").unwrap();
        let c = client(
            &format!("file://{}", root.display()),
            &root.join("cache"),
            false,
        );
        assert!(c.config().unwrap_err().contains("Upgrade forge"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
