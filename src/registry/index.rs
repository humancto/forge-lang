//! The sparse index format (rfcs/0007-package-registry.md).
//!
//! A registry is a tree of static files, served over HTTPS (or read from a
//! `file://` path for mirrors and tests):
//!
//! ```text
//! config.json            registry configuration (format version, mirror template)
//! index.toml             search summary (name, description, latest)
//! owners.toml            optional namespace / package key ownership
//! index/1/a              one file per package, one JSON line per version
//! index/2/ab
//! index/3/a/abc
//! index/ro/ut/router
//! ```
//!
//! Every function here is pure (no I/O) so the publish tool, the install
//! client and the tests share exactly one definition of the format.

use std::collections::BTreeMap;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

/// The index format version this client reads and writes. Lines carrying a
/// larger `v` are skipped so an old client keeps working on a newer index.
pub const INDEX_FORMAT_VERSION: u32 = 1;
pub const CONFIG_FILE: &str = "config.json";
pub const SEARCH_INDEX_FILE: &str = "index.toml";
pub const OWNERS_FILE: &str = "owners.toml";
pub const INDEX_DIR: &str = "index";
pub const MAX_NAME_LEN: usize = 64;

/// Names that can never be registered: the stdlib globals (an import of a
/// package called `json` would be confusing next to the `json` module),
/// toolchain words, and names Windows cannot create as directories.
const RESERVED_NAMES: &[&str] = &[
    "forge",
    "std",
    "core",
    "stdlib",
    "test",
    "tests",
    "forge_modules",
    "forge-modules",
    "math",
    "fs",
    "io",
    "crypto",
    "db",
    "pg",
    "mysql",
    "jwt",
    "env",
    "json",
    "regex",
    "log",
    "http",
    "csv",
    "term",
    "os",
    "path",
    "time",
    "url",
    "toml",
    "npc",
    "ws",
    "exec",
    "con",
    "prn",
    "aux",
    "nul",
    "com1",
    "com2",
    "com3",
    "com4",
    "com5",
    "com6",
    "com7",
    "com8",
    "com9",
    "lpt1",
    "lpt2",
    "lpt3",
    "lpt4",
    "lpt5",
    "lpt6",
    "lpt7",
    "lpt8",
    "lpt9",
];

/// `config.json` at the registry root.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexConfig {
    /// Index format version (see [`INDEX_FORMAT_VERSION`]).
    pub v: u32,
    /// Optional archive URL template for mirrors: `{name}`, `{vers}` and
    /// `{cksum}` are substituted. When unset, each entry's own `url` is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dl: Option<String>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        IndexConfig {
            v: INDEX_FORMAT_VERSION,
            dl: None,
        }
    }
}

/// One dependency of a published version. Only registry dependencies can be
/// published; `git`/`path` dependencies are rejected at publish time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexDep {
    pub name: String,
    pub req: String,
}

/// One line of a package file: a single published version.
///
/// Field order is the serialization order, which keeps index diffs stable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    pub v: u32,
    pub name: String,
    pub vers: String,
    #[serde(default)]
    pub deps: Vec<IndexDep>,
    /// Lowercase hex SHA-256 of the `.tar.gz` archive. Mandatory.
    pub cksum: String,
    /// Where the archive lives (normally a GitHub release asset).
    pub url: String,
    #[serde(default)]
    pub yanked: bool,
    /// `ed25519:<base64>` public key of the signer, when signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<String>,
    /// Base64 ed25519 signature over [`signed_message`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    /// RFC 3339 UTC publish time (informational).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub published: String,
}

/// Validate a registry package name.
///
/// Rules: 1..=64 chars, lowercase ASCII letters, digits, `-` and `_`; starts
/// with a letter; does not end with `-`/`_`; no `--`/`__`; not reserved.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return Err(format!(
            "package name '{}' must be 1 to {} characters long",
            name, MAX_NAME_LEN
        ));
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(format!(
            "package name '{}' must start with a lowercase ASCII letter",
            name
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(format!(
            "package name '{}' may only contain lowercase ASCII letters, digits, '-' and '_'",
            name
        ));
    }
    if name.ends_with(['-', '_']) || name.contains("--") || name.contains("__") {
        return Err(format!(
            "package name '{}' must not end with '-'/'_' or repeat them",
            name
        ));
    }
    if RESERVED_NAMES.contains(&name) {
        return Err(format!("package name '{}' is reserved", name));
    }
    Ok(())
}

/// The form two names are compared in to reject look-alikes: `foo_bar` and
/// `foo-bar` are the same package name for registration purposes.
pub fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

/// The path of a package file relative to the registry root
/// (crates.io-style sharding so no directory grows unboundedly).
pub fn index_path(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    match n.len() {
        1 => format!("{}/1/{}", INDEX_DIR, n),
        2 => format!("{}/2/{}", INDEX_DIR, n),
        3 => format!("{}/3/{}/{}", INDEX_DIR, &n[..1], n),
        _ => format!("{}/{}/{}/{}", INDEX_DIR, &n[..2], &n[2..4], n),
    }
}

/// True when `s` is a lowercase 64-digit hex SHA-256.
pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The exact bytes a publisher signs. Binding name and version prevents a
/// signature from being replayed onto another package or version; binding
/// the archive checksum binds the content.
pub fn signed_message(name: &str, vers: &str, cksum: &str) -> Vec<u8> {
    format!("forge-registry-v1\n{}\n{}\n{}\n", name, vers, cksum).into_bytes()
}

/// Check one entry's shape (not its signature or archive).
pub fn validate_entry(entry: &IndexEntry) -> Result<(), String> {
    validate_name(&entry.name)?;
    let who = format!("{}@{}", entry.name, entry.vers);
    Version::parse(&entry.vers)
        .map_err(|e| format!("{}: version is not valid semver: {}", who, e))?;
    if !is_sha256_hex(&entry.cksum) {
        return Err(format!(
            "{}: cksum must be a lowercase hex SHA-256 (checksums are mandatory)",
            who
        ));
    }
    if !(entry.url.starts_with("https://")
        || entry.url.starts_with("http://")
        || entry.url.starts_with("file://"))
    {
        return Err(format!(
            "{}: url must be an https://, http:// or file:// URL",
            who
        ));
    }
    for dep in &entry.deps {
        validate_name(&dep.name).map_err(|e| format!("{}: dependency {}", who, e))?;
        VersionReq::parse(&dep.req).map_err(|e| {
            format!(
                "{}: dependency '{}' has an invalid requirement '{}': {}",
                who, dep.name, dep.req, e
            )
        })?;
    }
    match (&entry.pubkey, &entry.sig) {
        (Some(_), Some(_)) | (None, None) => Ok(()),
        _ => Err(format!("{}: pubkey and sig must be present together", who)),
    }
}

/// Parse a package file. Blank lines are ignored; lines with a newer format
/// version are skipped; anything else malformed is an error (a corrupted
/// index must not silently hide versions).
pub fn parse_entries(name: &str, text: &str) -> Result<Vec<IndexEntry>, String> {
    let mut entries = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let raw: serde_json::Value = serde_json::from_str(line).map_err(|e| {
            format!(
                "index file for '{}' line {}: invalid JSON: {}",
                name,
                lineno + 1,
                e
            )
        })?;
        let v = raw.get("v").and_then(|v| v.as_u64()).unwrap_or(0);
        if v > u64::from(INDEX_FORMAT_VERSION) {
            continue;
        }
        let entry: IndexEntry = serde_json::from_value(raw).map_err(|e| {
            format!(
                "index file for '{}' line {}: invalid entry: {}",
                name,
                lineno + 1,
                e
            )
        })?;
        if entry.name != name {
            return Err(format!(
                "index file for '{}' line {}: entry is for '{}'",
                name,
                lineno + 1,
                entry.name
            ));
        }
        validate_entry(&entry).map_err(|e| format!("line {}: {}", lineno + 1, e))?;
        if entries.iter().any(|e: &IndexEntry| e.vers == entry.vers) {
            return Err(format!(
                "index file for '{}' lists version {} twice",
                name, entry.vers
            ));
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// Serialize entries back to a package file (one JSON object per line).
pub fn serialize_entries(entries: &[IndexEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        // Serializing a struct of strings/bools/vecs cannot fail.
        let line = serde_json::to_string(entry).expect("BUG: IndexEntry serializes to JSON");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Pick the version to install.
///
/// The newest non-yanked version matching `req` wins. `locked` (the version
/// recorded in forge.lock) is preferred whenever it still matches `req`,
/// even if it was yanked since: yanking stops new resolutions, it does not
/// break existing lockfiles.
pub fn resolve<'a>(
    name: &str,
    req: &VersionReq,
    entries: &'a [IndexEntry],
    locked: Option<&str>,
) -> Result<&'a IndexEntry, String> {
    let parsed: Vec<(Version, &IndexEntry)> = entries
        .iter()
        .filter_map(|e| Version::parse(&e.vers).ok().map(|v| (v, e)))
        .collect();
    if parsed.is_empty() {
        return Err(format!("no versions of '{}' are published", name));
    }
    if let Some(locked) = locked {
        if let Some((_, entry)) = parsed
            .iter()
            .find(|(v, e)| e.vers == locked && req.matches(v))
        {
            return Ok(entry);
        }
    }
    if let Some((_, entry)) = parsed
        .iter()
        .filter(|(v, e)| !e.yanked && req.matches(v))
        .max_by(|a, b| a.0.cmp(&b.0))
    {
        return Ok(entry);
    }
    let mut available: Vec<&(Version, &IndexEntry)> = parsed.iter().collect();
    available.sort_by(|a, b| a.0.cmp(&b.0));
    let listed: Vec<String> = available
        .iter()
        .map(|(_, e)| {
            if e.yanked {
                format!("{} (yanked)", e.vers)
            } else {
                e.vers.clone()
            }
        })
        .collect();
    let only_yanked = parsed.iter().any(|(v, e)| e.yanked && req.matches(v));
    Err(format!(
        "no {}version of '{}' matches '{}' (available: {})",
        if only_yanked { "non-yanked " } else { "" },
        name,
        req,
        listed.join(", ")
    ))
}

/// The URL to download an entry's archive from. A mirror's `dl` template
/// wins over the entry's own URL; the checksum keeps either one honest.
pub fn archive_url(config: &IndexConfig, entry: &IndexEntry) -> String {
    match &config.dl {
        Some(template) if !template.is_empty() => template
            .replace("{name}", &entry.name)
            .replace("{vers}", &entry.vers)
            .replace("{cksum}", &entry.cksum),
        _ => entry.url.clone(),
    }
}

/// `owners.toml`: which signing keys may publish which names.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Owners {
    /// `[namespaces.<prefix>]` owns every package named `<prefix>-*`.
    #[serde(default)]
    pub namespaces: BTreeMap<String, OwnerRule>,
    /// `[packages.<name>]` owns one package (also how keys are rotated).
    #[serde(default)]
    pub packages: BTreeMap<String, OwnerRule>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnerRule {
    /// `ed25519:<base64>` keys allowed to sign.
    #[serde(default)]
    pub keys: Vec<String>,
    /// GitHub handles that review changes (mirrored into CODEOWNERS).
    #[serde(default)]
    pub github: Vec<String>,
}

impl Owners {
    /// The keys that must sign `name`, if any rule covers it. A package rule
    /// wins over a namespace rule.
    pub fn required_keys(&self, name: &str) -> Option<&[String]> {
        if let Some(rule) = self.packages.get(name) {
            return Some(&rule.keys);
        }
        let prefix = name.split('-').next()?;
        if prefix.len() < name.len() {
            if let Some(rule) = self.namespaces.get(prefix) {
                return Some(&rule.keys);
            }
        }
        None
    }
}

/// Decide whether `new` may be added to a package whose existing versions
/// are `existing`. This is the index-side half of the trust model:
///
/// * versions are immutable: a version can be published once;
/// * a name covered by `owners.toml` must be signed by one of its keys;
/// * otherwise, once a package has a signed version, later versions must be
///   signed by the same key as the newest signed one (rotation goes through
///   an `owners.toml` package rule);
/// * names that differ only by `-`/`_` are the same name.
pub fn check_publish_allowed(
    new: &IndexEntry,
    existing: &[IndexEntry],
    all_names: &[String],
    owners: &Owners,
) -> Result<(), String> {
    validate_entry(new)?;
    if existing.iter().any(|e| e.vers == new.vers) {
        return Err(format!(
            "{}@{} is already published; versions are immutable (bump the version, or yank)",
            new.name, new.vers
        ));
    }
    if existing.is_empty() {
        let canonical = canonical_name(&new.name);
        if let Some(clash) = all_names
            .iter()
            .find(|n| *n != &new.name && canonical_name(n) == canonical)
        {
            return Err(format!(
                "package name '{}' is too similar to existing package '{}'",
                new.name, clash
            ));
        }
    }
    if let Some(keys) = owners.required_keys(&new.name) {
        return match &new.pubkey {
            Some(k) if keys.iter().any(|allowed| allowed == k) => Ok(()),
            Some(k) => Err(format!(
                "'{}' is owned in owners.toml and {} is not one of its keys",
                new.name, k
            )),
            None => Err(format!(
                "'{}' is owned in owners.toml, so it must be signed (use --sign)",
                new.name
            )),
        };
    }
    let newest_signed = existing
        .iter()
        .filter(|e| e.pubkey.is_some())
        .filter_map(|e| Version::parse(&e.vers).ok().map(|v| (v, e)))
        .max_by(|a, b| a.0.cmp(&b.0));
    if let Some((_, prev)) = newest_signed {
        if new.pubkey != prev.pubkey {
            return Err(format!(
                "{} {} is signed by {}; new versions must be signed by the same key \
                 (rotate keys with a [packages.{}] rule in owners.toml)",
                new.name,
                prev.vers,
                prev.pubkey.as_deref().unwrap_or("?"),
                new.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, vers: &str) -> IndexEntry {
        IndexEntry {
            v: 1,
            name: name.into(),
            vers: vers.into(),
            deps: Vec::new(),
            cksum: "a".repeat(64),
            url: format!("https://example.com/{}-{}.tar.gz", name, vers),
            yanked: false,
            pubkey: None,
            sig: None,
            description: String::new(),
            license: String::new(),
            published: String::new(),
        }
    }

    #[test]
    fn names_follow_the_rules() {
        for ok in ["router", "a", "json-utils", "kv_store2", "x1"] {
            assert!(validate_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", "Router", "1abc", "-x", "x-", "a--b", "a__b", "a/b", "../x", "json", "con", "a.b",
        ] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
        assert!(validate_name(&"a".repeat(65)).is_err());
        assert!(validate_name(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn index_paths_are_sharded_like_crates_io() {
        assert_eq!(index_path("a"), "index/1/a");
        assert_eq!(index_path("ab"), "index/2/ab");
        assert_eq!(index_path("abc"), "index/3/a/abc");
        assert_eq!(index_path("router"), "index/ro/ut/router");
    }

    #[test]
    fn entries_round_trip_and_skip_future_versions() {
        let mut a = entry("router", "1.0.0");
        a.deps.push(IndexDep {
            name: "kv".into(),
            req: "^0.1".into(),
        });
        let b = entry("router", "1.1.0");
        let mut text = serialize_entries(&[a.clone(), b.clone()]);
        text.push_str("{\"v\":2,\"name\":\"router\",\"whatever\":true}\n\n");
        let parsed = parse_entries("router", &text).unwrap();
        assert_eq!(parsed, vec![a, b]);
    }

    #[test]
    fn malformed_or_foreign_lines_are_errors() {
        assert!(parse_entries("router", "not json\n").is_err());
        let other = serialize_entries(&[entry("kv", "1.0.0")]);
        assert!(parse_entries("router", &other)
            .unwrap_err()
            .contains("entry is for 'kv'"));
        let mut no_ck = entry("router", "1.0.0");
        no_ck.cksum = String::new();
        assert!(parse_entries("router", &serialize_entries(&[no_ck]))
            .unwrap_err()
            .contains("checksums are mandatory"));
        let dup = serialize_entries(&[entry("router", "1.0.0"), entry("router", "1.0.0")]);
        assert!(parse_entries("router", &dup).unwrap_err().contains("twice"));
    }

    #[test]
    fn resolve_prefers_newest_non_yanked() {
        let mut yanked = entry("r", "1.5.0");
        yanked.yanked = true;
        let entries = vec![
            entry("r", "1.0.0"),
            entry("r", "1.2.0"),
            yanked,
            entry("r", "2.0.0"),
        ];
        let req = VersionReq::parse("^1.0").unwrap();
        assert_eq!(resolve("r", &req, &entries, None).unwrap().vers, "1.2.0");
        assert_eq!(
            resolve("r", &VersionReq::STAR, &entries, None)
                .unwrap()
                .vers,
            "2.0.0"
        );
        // A lockfile keeps a yanked version installable.
        assert_eq!(
            resolve("r", &req, &entries, Some("1.5.0")).unwrap().vers,
            "1.5.0"
        );
        // A locked version that no longer matches the requirement is ignored.
        assert_eq!(
            resolve("r", &req, &entries, Some("2.0.0")).unwrap().vers,
            "1.2.0"
        );
    }

    #[test]
    fn resolve_reports_yanked_only_matches() {
        let mut y = entry("r", "3.0.0");
        y.yanked = true;
        let entries = vec![entry("r", "1.0.0"), y];
        let err = resolve("r", &VersionReq::parse("^3").unwrap(), &entries, None).unwrap_err();
        assert!(err.contains("no non-yanked version"), "{err}");
        assert!(err.contains("3.0.0 (yanked)"), "{err}");
        let err = resolve("r", &VersionReq::parse("^9").unwrap(), &entries, None).unwrap_err();
        assert!(err.contains("no version of 'r' matches"), "{err}");
    }

    #[test]
    fn mirror_template_overrides_entry_url() {
        let e = entry("router", "1.0.0");
        assert_eq!(archive_url(&IndexConfig::default(), &e), e.url);
        let mirror = IndexConfig {
            v: 1,
            dl: Some("https://mirror.example/{name}/{vers}/{cksum}.tgz".into()),
        };
        assert_eq!(
            archive_url(&mirror, &e),
            format!("https://mirror.example/router/1.0.0/{}.tgz", e.cksum)
        );
    }

    #[test]
    fn publish_rules() {
        let owners: Owners = toml::from_str(
            "[namespaces.acme]\nkeys = [\"ed25519:A\"]\n[packages.special]\nkeys = [\"ed25519:B\"]\n",
        )
        .unwrap();
        let names = vec!["foo-bar".to_string()];

        // Immutable versions.
        let e = entry("foo-bar", "1.0.0");
        let err = check_publish_allowed(&e, std::slice::from_ref(&e), &names, &owners).unwrap_err();
        assert!(err.contains("immutable"), "{err}");

        // Look-alike names.
        let err =
            check_publish_allowed(&entry("foo_bar", "1.0.0"), &[], &names, &owners).unwrap_err();
        assert!(err.contains("too similar"), "{err}");

        // Namespace ownership.
        let mut ns = entry("acme-http", "1.0.0");
        assert!(check_publish_allowed(&ns, &[], &names, &owners).is_err());
        ns.pubkey = Some("ed25519:A".into());
        ns.sig = Some("x".into());
        assert!(check_publish_allowed(&ns, &[], &names, &owners).is_ok());
        ns.pubkey = Some("ed25519:C".into());
        assert!(check_publish_allowed(&ns, &[], &names, &owners).is_err());

        // Key continuity once signed.
        let mut v1 = entry("solo", "1.0.0");
        v1.pubkey = Some("ed25519:K".into());
        v1.sig = Some("s".into());
        let mut v2 = entry("solo", "1.1.0");
        let err =
            check_publish_allowed(&v2, std::slice::from_ref(&v1), &names, &owners).unwrap_err();
        assert!(err.contains("same key"), "{err}");
        v2.pubkey = v1.pubkey.clone();
        v2.sig = Some("s".into());
        assert!(check_publish_allowed(&v2, &[v1], &names, &owners).is_ok());
    }
}
