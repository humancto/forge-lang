//! Publisher signatures and trust-on-first-use pinning (rfcs/0007).
//!
//! Signatures are optional, checksums are not. A signature is an ed25519
//! signature over [`index::signed_message`] (name, version, archive SHA-256),
//! carried in the index entry together with the signer's public key. The
//! client pins the first key it sees for each (registry, package) pair in
//! `~/.forge/trusted-keys.toml`; a later version signed by a different key,
//! or an unsigned version of a package that was signed before, is refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::index::{self, IndexEntry};

pub const KEY_PREFIX: &str = "ed25519:";
const SECRET_HEADER: &str = "forge-ed25519-secret-key-v1";

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// `ed25519:<base64>` form used in index entries, lockfiles and owners.toml.
pub fn format_public_key(key: &VerifyingKey) -> String {
    format!("{}{}", KEY_PREFIX, b64().encode(key.as_bytes()))
}

pub fn parse_public_key(s: &str) -> Result<VerifyingKey, String> {
    let body = s
        .strip_prefix(KEY_PREFIX)
        .ok_or_else(|| format!("public key '{}' must start with '{}'", s, KEY_PREFIX))?;
    let bytes = b64()
        .decode(body)
        .map_err(|e| format!("public key '{}' is not valid base64: {}", s, e))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("public key '{}' must be 32 bytes", s))?;
    VerifyingKey::from_bytes(&arr).map_err(|e| format!("public key '{}' is invalid: {}", s, e))
}

/// Generate a new signing key from the OS random number generator.
pub fn generate_key() -> Result<SigningKey, String> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| format!("no OS randomness available: {}", e))?;
    let key = SigningKey::from_bytes(&seed);
    seed.fill(0);
    Ok(key)
}

/// Where `forge publish --sign` keeps the publisher key:
/// `$FORGE_SIGNING_KEY`, else `~/.forge/keys/publish.key`.
pub fn default_key_path() -> PathBuf {
    if let Some(p) = std::env::var_os("FORGE_SIGNING_KEY") {
        return PathBuf::from(p);
    }
    super::forge_home().join("keys").join("publish.key")
}

/// Load the signing key at `path`, creating it (mode 0600) when missing.
/// Returns the key and whether it was newly created.
pub fn load_or_create_key(path: &Path) -> Result<(SigningKey, bool), String> {
    if path.exists() {
        return load_signing_key(path).map(|k| (k, false));
    }
    let key = generate_key()?;
    save_signing_key(path, &key)?;
    Ok((key, true))
}

pub fn load_signing_key(path: &Path) -> Result<SigningKey, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read signing key {}: {}", path.display(), e))?;
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some(SECRET_HEADER) {
        return Err(format!(
            "{} is not a Forge signing key (missing '{}' header)",
            path.display(),
            SECRET_HEADER
        ));
    }
    let body = lines.next().unwrap_or("").trim();
    let bytes = b64()
        .decode(body)
        .map_err(|e| format!("signing key {} is corrupt: {}", path.display(), e))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("signing key {} must hold 32 bytes", path.display()))?;
    Ok(SigningKey::from_bytes(&arr))
}

fn save_signing_key(path: &Path, key: &SigningKey) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    let content = format!("{}\n{}\n", SECRET_HEADER, b64().encode(key.to_bytes()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("failed to create signing key {}: {}", path.display(), e))?;
    std::io::Write::write_all(&mut file, content.as_bytes())
        .map_err(|e| format!("failed to write signing key {}: {}", path.display(), e))
}

/// Sign `entry` in place (sets `pubkey` and `sig`).
pub fn sign_entry(key: &SigningKey, entry: &mut IndexEntry) {
    let msg = index::signed_message(&entry.name, &entry.vers, &entry.cksum);
    let sig = key.sign(&msg);
    entry.pubkey = Some(format_public_key(&key.verifying_key()));
    entry.sig = Some(b64().encode(sig.to_bytes()));
}

/// Verify an entry's signature. `Ok(None)` means the entry is unsigned;
/// `Ok(Some(pubkey))` means it carries a valid signature by `pubkey`.
pub fn verify_entry(entry: &IndexEntry) -> Result<Option<String>, String> {
    let who = format!("{}@{}", entry.name, entry.vers);
    match (&entry.pubkey, &entry.sig) {
        (None, None) => Ok(None),
        (Some(pk), Some(sig)) => {
            let key = parse_public_key(pk)?;
            let bytes = b64()
                .decode(sig)
                .map_err(|e| format!("{}: signature is not valid base64: {}", who, e))?;
            let arr: [u8; 64] = bytes
                .try_into()
                .map_err(|_| format!("{}: signature must be 64 bytes", who))?;
            let msg = index::signed_message(&entry.name, &entry.vers, &entry.cksum);
            key.verify(&msg, &Signature::from_bytes(&arr))
                .map_err(|_| format!("{}: signature verification FAILED for key {}", who, pk))?;
            Ok(Some(pk.clone()))
        }
        _ => Err(format!("{}: pubkey and sig must be present together", who)),
    }
}

/// Pinned publisher keys: registry URL -> package -> `ed25519:` key.
#[derive(Debug, Default)]
pub struct TrustStore {
    path: PathBuf,
    pins: BTreeMap<String, BTreeMap<String, String>>,
    dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustDecision {
    /// No signature (allowed unless signatures are required or a key is pinned).
    Unsigned,
    /// Signed by the pinned key.
    Trusted(String),
    /// Signed; first time this package is seen, so the key is now pinned.
    NewlyPinned(String),
}

impl TrustStore {
    /// `~/.forge/trusted-keys.toml`.
    pub fn default_path() -> PathBuf {
        super::forge_home().join("trusted-keys.toml")
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let pins = if path.exists() {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
            #[derive(serde::Deserialize, Default)]
            struct File {
                #[serde(default)]
                registries: BTreeMap<String, BTreeMap<String, String>>,
            }
            toml::from_str::<File>(&text)
                .map_err(|e| format!("{} is corrupt: {}", path.display(), e))?
                .registries
        } else {
            BTreeMap::new()
        };
        Ok(TrustStore {
            path: path.to_path_buf(),
            pins,
            dirty: false,
        })
    }

    pub fn pinned(&self, registry: &str, package: &str) -> Option<&str> {
        self.pins
            .get(registry)
            .and_then(|m| m.get(package))
            .map(String::as_str)
    }

    /// Apply the trust policy to `entry` (whose signature is verified here).
    /// Pins new keys in memory; call [`TrustStore::save`] once the install
    /// succeeded.
    pub fn check(
        &mut self,
        registry: &str,
        entry: &IndexEntry,
        require_signatures: bool,
    ) -> Result<TrustDecision, String> {
        let signer = verify_entry(entry)?;
        let pinned = self.pinned(registry, &entry.name).map(str::to_string);
        match (signer, pinned) {
            (Some(key), Some(pin)) if key == pin => Ok(TrustDecision::Trusted(key)),
            (Some(key), Some(pin)) => Err(format!(
                "{}@{} is signed by {}, but {} is pinned for '{}' from this registry.\n  \
                 If the publisher rotated keys, verify the new key out of band, then remove the \
                 '{}' line under [registries.\"{}\"] in {}.",
                entry.name,
                entry.vers,
                key,
                pin,
                entry.name,
                entry.name,
                registry,
                self.path.display()
            )),
            (Some(key), None) => {
                self.pins
                    .entry(registry.to_string())
                    .or_default()
                    .insert(entry.name.clone(), key.clone());
                self.dirty = true;
                Ok(TrustDecision::NewlyPinned(key))
            }
            (None, Some(pin)) => Err(format!(
                "{}@{} is unsigned, but earlier versions were signed by {} (pinned in {}). \
                 Refusing a possible signature-stripping attack.",
                entry.name,
                entry.vers,
                pin,
                self.path.display()
            )),
            (None, None) if require_signatures => Err(format!(
                "{}@{} is unsigned and FORGE_REQUIRE_SIGNATURES is set",
                entry.name, entry.vers
            )),
            (None, None) => Ok(TrustDecision::Unsigned),
        }
    }

    pub fn save(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        #[derive(serde::Serialize)]
        struct File<'a> {
            registries: &'a BTreeMap<String, BTreeMap<String, String>>,
        }
        let body = toml::to_string_pretty(&File {
            registries: &self.pins,
        })
        .map_err(|e| format!("failed to encode trusted keys: {}", e))?;
        let content = format!(
            "# Publisher keys pinned on first use by `forge install` (rfcs/0007).\n\
             # Remove a line to accept a rotated key after verifying it out of band.\n{}",
            body
        );
        super::atomic_write(&self.path, content.as_bytes())
            .map_err(|e| format!("failed to write {}: {}", self.path.display(), e))?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> IndexEntry {
        IndexEntry {
            v: 1,
            name: "router".into(),
            vers: "1.0.0".into(),
            deps: Vec::new(),
            cksum: "b".repeat(64),
            url: "https://example.com/r.tar.gz".into(),
            yanked: false,
            pubkey: None,
            sig: None,
            description: String::new(),
            license: String::new(),
            published: String::new(),
        }
    }

    fn temp(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "forge-signing-{}-{}-{}",
            tag,
            std::process::id(),
            unique
        ))
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let key = generate_key().unwrap();
        let mut e = entry();
        assert_eq!(verify_entry(&e).unwrap(), None);
        sign_entry(&key, &mut e);
        let pk = verify_entry(&e).unwrap().unwrap();
        assert_eq!(pk, format_public_key(&key.verifying_key()));
        assert_eq!(parse_public_key(&pk).unwrap(), key.verifying_key());
    }

    #[test]
    fn tampering_breaks_the_signature() {
        let key = generate_key().unwrap();
        let mut e = entry();
        sign_entry(&key, &mut e);
        for tamper in [
            |e: &mut IndexEntry| e.cksum = "c".repeat(64),
            |e: &mut IndexEntry| e.vers = "1.0.1".into(),
            |e: &mut IndexEntry| e.name = "routes".into(),
        ] {
            let mut t = e.clone();
            tamper(&mut t);
            assert!(verify_entry(&t).unwrap_err().contains("FAILED"));
        }
        // A signature from another key under the original pubkey fails too.
        let other = generate_key().unwrap();
        let mut forged = e.clone();
        sign_entry(&other, &mut forged);
        forged.pubkey = e.pubkey.clone();
        assert!(verify_entry(&forged).is_err());
    }

    #[test]
    fn key_files_round_trip_with_private_permissions() {
        let dir = temp("keyfile");
        let path = dir.join("keys").join("publish.key");
        let (k1, created) = load_or_create_key(&path).unwrap();
        assert!(created);
        let (k2, created) = load_or_create_key(&path).unwrap();
        assert!(!created);
        assert_eq!(k1.to_bytes(), k2.to_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(&path, "garbage\n").unwrap();
        assert!(load_signing_key(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn trust_on_first_use() {
        let dir = temp("tofu");
        let path = dir.join("trusted-keys.toml");
        let reg = "https://registry.example";
        let key = generate_key().unwrap();
        let mut signed = entry();
        sign_entry(&key, &mut signed);

        let mut store = TrustStore::load(&path).unwrap();
        let first = store.check(reg, &signed, false).unwrap();
        assert!(matches!(first, TrustDecision::NewlyPinned(_)));
        store.save().unwrap();

        let mut store = TrustStore::load(&path).unwrap();
        assert!(matches!(
            store.check(reg, &signed, false).unwrap(),
            TrustDecision::Trusted(_)
        ));

        // A different key for the same package is refused.
        let mut rotated = entry();
        rotated.vers = "1.1.0".into();
        sign_entry(&generate_key().unwrap(), &mut rotated);
        let err = store.check(reg, &rotated, false).unwrap_err();
        assert!(err.contains("is pinned"), "{err}");

        // So is an unsigned version of a signed package.
        let err = store.check(reg, &entry(), false).unwrap_err();
        assert!(err.contains("signature-stripping"), "{err}");

        // Pins are per registry.
        assert!(matches!(
            store
                .check("https://other.example", &rotated, false)
                .unwrap(),
            TrustDecision::NewlyPinned(_)
        ));

        // Unsigned packages pass unless signatures are required.
        let mut plain = entry();
        plain.name = "plain".into();
        assert_eq!(
            store.check(reg, &plain, false).unwrap(),
            TrustDecision::Unsigned
        );
        assert!(store.check(reg, &plain, true).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
