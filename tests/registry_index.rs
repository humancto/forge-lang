//! End-to-end tests for the sparse package registry (rfcs/0007).
//!
//! Every test runs the real `forge` binary with an isolated HOME against a
//! registry that lives on disk (`file://`) or behind a tiny in-process HTTP
//! server with ETag support. Nothing touches the network.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "forge-registry-index-{}-{}-{}",
        tag,
        std::process::id(),
        unique
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

struct Env {
    root: PathBuf,
    home: PathBuf,
    index: PathBuf,
    registry_url: String,
    extra: Vec<(String, String)>,
}

impl Env {
    /// A fresh HOME plus an index clone (git repo with config.json).
    fn new(tag: &str) -> Env {
        let root = temp_dir(tag);
        let home = root.join("home");
        let index = root.join("forge-registry");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(&index).expect("index");
        std::fs::write(index.join("config.json"), "{\"v\":1}\n").expect("config");
        std::fs::write(index.join("index.toml"), "packages = []\n").expect("index.toml");
        let env = Env {
            registry_url: format!("file://{}", index.display()),
            root,
            home,
            index,
            extra: Vec::new(),
        };
        env.git(&["init", "-q", "-b", "main"]);
        env.git(&["add", "."]);
        env.git(&["commit", "-q", "-m", "seed"]);
        env
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.index)
            .args(args)
            .envs(git_identity(&self.home))
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn forge(&self, cwd: &Path, args: &[&str]) -> Output {
        let out = Command::new(env!("CARGO_BIN_EXE_forge"))
            .args(args)
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("FORGE_REGISTRY_URL", &self.registry_url)
            .env("NO_COLOR", "1")
            .env_remove("FORGE_REGISTRY_PATH")
            .env_remove("FORGE_SIGNING_KEY")
            .env_remove("FORGE_OFFLINE")
            .env_remove("FORGE_REQUIRE_SIGNATURES")
            .env_remove("FORGE_CACHE_TTL")
            .env_remove("GITHUB_TOKEN")
            .envs(git_identity(&self.home))
            .envs(self.extra.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .output()
            .expect("run forge");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("panicked"), "forge panicked: {stderr}");
        out
    }

    /// Create a library project and publish it into the index clone.
    fn publish(&self, name: &str, version: &str, body: &str, extra_args: &[&str]) -> Output {
        let project = self.root.join(format!("src-{}-{}", name, version));
        std::fs::create_dir_all(&project).expect("project");
        std::fs::write(
            project.join("forge.toml"),
            format!(
                "[project]\nname = \"{}\"\nversion = \"{}\"\ndescription = \"{} library\"\nlicense = \"MIT\"\n",
                name, version, name
            ),
        )
        .expect("manifest");
        std::fs::write(project.join("main.fg"), body).expect("main");
        let dist = self.root.join("dist");
        let template = format!("{}/{{name}}-{{vers}}.tar.gz", self.download_base());
        let dist_s = dist.display().to_string();
        let mut args = vec![
            "publish",
            "--registry",
            self.index.to_str().expect("utf8"),
            "--download-url",
            &template,
            "--out-dir",
            &dist_s,
        ];
        args.extend_from_slice(extra_args);
        let out = self.forge(&project, &args);
        if out.status.success() && !extra_args.contains(&"--no-commit") {
            // Simulate the merged pull request.
            let branch = self.git(&["rev-parse", "--abbrev-ref", "HEAD"]);
            self.git(&["checkout", "-q", "main"]);
            self.git(&["merge", "-q", "--ff-only", &branch]);
        }
        out
    }

    fn download_base(&self) -> String {
        format!("file://{}", self.root.join("dist").display())
    }

    fn app(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).expect("app");
        std::fs::write(
            dir.join("forge.toml"),
            format!("[project]\nname = \"{}\"\nversion = \"0.1.0\"\n", name),
        )
        .expect("manifest");
        dir
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git_identity(home: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("GIT_AUTHOR_NAME", "Forge Test".to_string()),
        ("GIT_AUTHOR_EMAIL", "test@example.com".to_string()),
        ("GIT_COMMITTER_NAME", "Forge Test".to_string()),
        ("GIT_COMMITTER_EMAIL", "test@example.com".to_string()),
        (
            "GIT_CONFIG_GLOBAL",
            home.join(".gitconfig-none").display().to_string(),
        ),
        ("GIT_CONFIG_NOSYSTEM", "1".to_string()),
    ]
}

fn text(out: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn lockfile(app: &Path) -> String {
    std::fs::read_to_string(app.join("forge.lock")).expect("forge.lock")
}

fn index_file(env: &Env, rel: &str) -> PathBuf {
    env.index.join(rel)
}

const KV_V1: &str = "fn kv_version() { return \"kv 0.1.0\" }\n";
const KV_V2: &str = "fn kv_version() { return \"kv 0.2.0\" }\n";

#[test]
fn publish_sign_install_and_run() {
    let env = Env::new("roundtrip");
    let out = env.publish("kv", "0.1.0", KV_V1, &["--sign"]);
    assert!(out.status.success(), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Created signing key"), "{stdout}");
    assert!(stdout.contains("publish/kv-0.1.0"), "{stdout}");
    assert!(env.home.join(".forge/keys/publish.key").exists());
    assert_eq!(env.git(&["log", "-1", "--format=%s"]), "publish kv@0.1.0");

    let entry_line = std::fs::read_to_string(index_file(&env, "index/2/kv")).expect("entry");
    assert!(entry_line.contains("\"sig\":"), "{entry_line}");

    let app = env.app("app");
    std::fs::write(app.join("main.fg"), "import \"kv\"\nsay kv_version()\n").expect("main");
    let out = env.forge(&app, &["add", "kv@^0.1"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Trusting new publisher key"));

    let lock = lockfile(&app);
    // TOML may emit a literal ('...') string when the path has backslashes
    // (Windows), so match the value, not the quoting.
    assert!(
        lock.lines()
            .any(|l| l.starts_with("source = ") && l.contains("sparse+file://")),
        "{lock}"
    );
    assert!(lock.contains("archive_checksum = \"sha256:"), "{lock}");
    assert!(lock.contains("signer = \"ed25519:"), "{lock}");
    let trust = std::fs::read_to_string(env.home.join(".forge/trusted-keys.toml")).expect("pins");
    assert!(trust.contains("kv = \"ed25519:"), "{trust}");

    let out = env.forge(&app, &["run", "main.fg"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("kv 0.1.0"));

    // The independent Python validator accepts what `forge publish` wrote.
    run_python_validator(&env.index, &["--allow-insecure-urls"]);
}

#[test]
fn checksum_mismatch_is_refused() {
    let env = Env::new("tamper");
    let out = env.publish("kv", "0.1.0", KV_V1, &[]);
    assert!(out.status.success(), "{}", text(&out));
    // Replace the uploaded archive after publishing.
    let archive = env.root.join("dist").join("kv-0.1.0.tar.gz");
    std::fs::write(&archive, b"not the published bytes").expect("tamper");

    let app = env.app("app");
    let out = env.forge(&app, &["add", "kv"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("checksum mismatch"), "{}", text(&out));
    assert!(!app.join("forge_modules/kv").exists());
    python_validator_rejects(&env.index, &["--allow-insecure-urls"], "has SHA-256");
}

#[test]
fn forged_signature_is_refused() {
    let env = Env::new("forged");
    let out = env.publish("kv", "0.1.0", KV_V1, &["--sign"]);
    assert!(out.status.success(), "{}", text(&out));
    let path = index_file(&env, "index/2/kv");
    let line = std::fs::read_to_string(&path).expect("entry");
    let mut entry: serde_json::Value = serde_json::from_str(line.trim()).expect("json");
    // Point the entry at different content while keeping the old signature.
    entry["cksum"] = serde_json::Value::String("0".repeat(64));
    std::fs::write(&path, format!("{}\n", entry)).expect("write");

    let app = env.app("app");
    let out = env.forge(&app, &["add", "kv"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("signature verification FAILED"),
        "{}",
        text(&out)
    );
    // CI catches it too (offline: the checksum is deliberately bogus).
    python_validator_rejects(
        &env.index,
        &["--allow-insecure-urls", "--offline"],
        "signature verification FAILED",
    );
}

#[test]
fn lockfile_pins_archive_checksum() {
    let env = Env::new("lockpin");
    assert!(env.publish("kv", "0.1.0", KV_V1, &[]).status.success());
    let app = env.app("app");
    let out = env.forge(&app, &["add", "kv"]);
    assert!(out.status.success(), "{}", text(&out));

    // Someone rewrites history: same version, different archive + checksum.
    let other = env.root.join("dist").join("kv-0.1.0.tar.gz");
    let src = env.root.join("evil");
    std::fs::create_dir_all(&src).expect("evil");
    std::fs::write(
        src.join("forge.toml"),
        "[project]\nname = \"kv\"\nversion = \"0.1.0\"\n",
    )
    .expect("m");
    std::fs::write(src.join("main.fg"), "say \"evil\"\n").expect("evil main");
    let path = index_file(&env, "index/2/kv");
    std::fs::remove_file(&path).expect("rm entry");
    std::fs::remove_file(&other).expect("rm archive");
    let out = env.forge(
        &src,
        &[
            "publish",
            "--registry",
            env.index.to_str().expect("utf8"),
            "--download-url",
            &format!("{}/{{name}}-{{vers}}.tar.gz", env.download_base()),
            "--out-dir",
            &env.root.join("dist").display().to_string(),
            "--no-commit",
        ],
    );
    assert!(out.status.success(), "{}", text(&out));

    let out = env.forge(&app, &["install", "."]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("forge.lock pins"), "{}", text(&out));
    // The index CI refuses the rewrite as well.
    python_validator_rejects(
        &env.index,
        &["--allow-insecure-urls", "--base", "HEAD"],
        "a published entry changed",
    );
}

#[test]
fn yanked_versions_are_skipped_unless_locked() {
    let env = Env::new("yank");
    assert!(env.publish("kv", "0.1.0", KV_V1, &[]).status.success());
    assert!(env.publish("kv", "0.2.0", KV_V2, &[]).status.success());

    let locked_app = env.app("locked");
    let out = env.forge(&locked_app, &["add", "kv"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(lockfile(&locked_app).contains("version = \"0.2.0\""));

    let out = env.forge(
        &env.root,
        &[
            "yank",
            "kv@0.2.0",
            "--registry",
            env.index.to_str().expect("utf8"),
        ],
    );
    assert!(out.status.success(), "{}", text(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("yank/kv-0.2.0"));
    assert_eq!(env.git(&["log", "-1", "--format=%s"]), "yank kv@0.2.0");
    env.git(&["checkout", "-q", "main"]);
    env.git(&["merge", "-q", "--ff-only", "yank/kv-0.2.0"]);

    // New resolutions skip the yanked version...
    let fresh = env.app("fresh");
    let out = env.forge(&fresh, &["add", "kv"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        lockfile(&fresh).contains("version = \"0.1.0\""),
        "{}",
        lockfile(&fresh)
    );

    // ...but an existing lockfile keeps it.
    let out = env.forge(&locked_app, &["install", "."]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("is yanked; keeping it"),
        "{}",
        text(&out)
    );
    assert!(lockfile(&locked_app).contains("version = \"0.2.0\""));

    // An exact requirement on a yanked version explains itself.
    let pinned = env.app("pinned");
    let out = env.forge(&pinned, &["add", "kv@=0.2.0"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("0.2.0 (yanked)"), "{}", text(&out));

    run_python_validator(&env.index, &["--allow-insecure-urls", "--base", "HEAD~1"]);
}

#[test]
fn publish_refuses_republish_and_key_change() {
    let env = Env::new("immutable");
    assert!(env
        .publish("kv", "0.1.0", KV_V1, &["--sign"])
        .status
        .success());
    let again = env.publish("kv", "0.1.0", KV_V1, &["--sign"]);
    assert!(!again.status.success());
    assert!(text(&again).contains("immutable"), "{}", text(&again));

    // A different key for the next version is refused.
    let other_key = env.root.join("other.key");
    let mut env2 = env;
    env2.extra
        .push(("FORGE_SIGNING_KEY".into(), other_key.display().to_string()));
    let out = env2.publish("kv", "0.2.0", KV_V2, &["--sign"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("same key"), "{}", text(&out));
}

#[test]
fn missing_registry_explains_itself() {
    let mut env = Env::new("noreg");
    env.registry_url = format!("file://{}", env.root.join("nothing-here").display());
    let app = env.app("app");
    let out = env.forge(&app, &["add", "kv"]);
    assert!(!out.status.success());
    let t = text(&out);
    assert!(t.contains("not a Forge registry"), "{t}");
    assert!(t.contains("forge.toml was not modified"), "{t}");
}

// ---------------------------------------------------------------------------
// HTTP: ETag revalidation, offline mode, credentials.

#[derive(Debug, Clone)]
struct Hit {
    path: String,
    status: u16,
    if_none_match: bool,
    authorization: bool,
}

/// Serve `root` over HTTP with strong ETags; record every request.
fn serve(root: PathBuf) -> (String, Arc<Mutex<Vec<Hit>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let log = Arc::new(Mutex::new(Vec::new()));
    let log2 = Arc::clone(&log);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let mut headers = BTreeMap::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((k, v)) = line.split_once(':') {
                    headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
                }
            }
            let path = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .trim_start_matches('/')
                .to_string();
            let file = root.join(&path);
            let (status, body, etag) = if path.contains("..") {
                (400, Vec::new(), None)
            } else {
                match std::fs::read(&file) {
                    Ok(bytes) => {
                        let etag = format!("\"{:x}-{}\"", bytes.len(), simple_hash(&bytes));
                        if headers.get("if-none-match") == Some(&etag) {
                            (304, Vec::new(), Some(etag))
                        } else {
                            (200, bytes, Some(etag))
                        }
                    }
                    Err(_) => (404, Vec::new(), None),
                }
            };
            log2.lock().expect("log").push(Hit {
                path: path.clone(),
                status,
                if_none_match: headers.contains_key("if-none-match"),
                authorization: headers.contains_key("authorization"),
            });
            let reason = match status {
                200 => "OK",
                304 => "Not Modified",
                404 => "Not Found",
                _ => "Bad Request",
            };
            let mut head = format!(
                "HTTP/1.1 {} {}\r\nConnection: close\r\nContent-Length: {}\r\n",
                status,
                reason,
                body.len()
            );
            if let Some(etag) = etag {
                head.push_str(&format!("ETag: {}\r\n", etag));
            }
            head.push_str("\r\n");
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
            let mut sink = [0u8; 0];
            let _ = reader.read(&mut sink);
        }
    });
    (format!("http://{}", addr), log)
}

fn simple_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

#[test]
fn http_registry_revalidates_with_etag_and_works_offline() {
    let mut env = Env::new("http");
    let (base, log) = serve(env.root.clone());
    env.registry_url = format!("{}/forge-registry", base);
    // Archives are served by the same server.
    let template = format!("{}/dist/{{name}}-{{vers}}.tar.gz", base);
    let project = env.root.join("src-kv");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(
        project.join("forge.toml"),
        "[project]\nname = \"kv\"\nversion = \"0.1.0\"\n",
    )
    .expect("m");
    std::fs::write(project.join("main.fg"), KV_V1).expect("main");
    let out = env.forge(
        &project,
        &[
            "publish",
            "--registry",
            env.index.to_str().expect("utf8"),
            "--download-url",
            &template,
            "--out-dir",
            &env.root.join("dist").display().to_string(),
            "--no-commit",
        ],
    );
    assert!(out.status.success(), "{}", text(&out));

    // Always revalidate, and send a token that must not leak to this host.
    env.extra.push(("FORGE_CACHE_TTL".into(), "0".into()));
    env.extra.push(("GITHUB_TOKEN".into(), "secret".into()));
    let app = env.app("app");
    let out = env.forge(&app, &["add", "kv"]);
    assert!(out.status.success(), "{}", text(&out));

    let first: Vec<Hit> = log.lock().expect("log").drain(..).collect();
    assert!(
        first
            .iter()
            .any(|h| h.path == "forge-registry/index/2/kv" && h.status == 200),
        "{first:?}"
    );
    assert!(first
        .iter()
        .any(|h| h.path == "dist/kv-0.1.0.tar.gz" && h.status == 200));
    assert!(
        first.iter().all(|h| !h.authorization),
        "token leaked: {first:?}"
    );

    // Second resolution: conditional requests answered with 304; the archive
    // comes from the content-addressed cache.
    let out = env.forge(&app, &["install", "."]);
    assert!(out.status.success(), "{}", text(&out));
    let second: Vec<Hit> = log.lock().expect("log").drain(..).collect();
    let entry_hit = second
        .iter()
        .find(|h| h.path == "forge-registry/index/2/kv")
        .expect("entry revalidated");
    assert!(entry_hit.if_none_match, "{second:?}");
    assert_eq!(entry_hit.status, 304, "{second:?}");
    assert!(
        !second.iter().any(|h| h.path.starts_with("dist/")),
        "archive re-downloaded: {second:?}"
    );

    // Offline: no requests at all, everything from cache.
    env.extra.push(("FORGE_OFFLINE".into(), "1".into()));
    let offline_app = env.app("offline");
    let out = env.forge(&offline_app, &["add", "kv"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(offline_app.join("forge_modules/kv/main.fg").exists());
    assert!(
        log.lock().expect("log").is_empty(),
        "offline mode hit the network"
    );

    // Offline and not cached: a clear error.
    let out = env.forge(&offline_app, &["add", "nothere"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("FORGE_OFFLINE"), "{}", text(&out));
}

#[test]
fn http_registry_without_config_is_reported_clearly() {
    let mut env = Env::new("http404");
    let (base, _log) = serve(env.root.join("empty"));
    env.registry_url = base;
    let app = env.app("app");
    let out = env.forge(&app, &["add", "kv"]);
    assert!(!out.status.success());
    assert!(
        text(&out).contains("not a Forge registry"),
        "{}",
        text(&out)
    );
}

/// Run tools/registry-template/scripts/validate_index.py when Python 3.11+
/// with `cryptography` is available (it is in CI); otherwise say why not.
fn python_validator(index: &Path, extra: &[&str]) -> Option<Output> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tools/registry-template/scripts/validate_index.py");
    let probe = Command::new("python3")
        .args(["-c", "import tomllib, cryptography"])
        .output();
    if !probe.is_ok_and(|o| o.status.success()) {
        eprintln!(
            "note: python3 with tomllib+cryptography not found; skipping validator cross-check"
        );
        return None;
    }
    Some(
        Command::new("python3")
            .arg(&script)
            .arg("--root")
            .arg(index)
            .args(extra)
            .output()
            .expect("run validator"),
    )
}

fn run_python_validator(index: &Path, extra: &[&str]) {
    if let Some(out) = python_validator(index, extra) {
        assert!(
            out.status.success(),
            "validate_index.py rejected the index:\n{}",
            text(&out)
        );
    }
}

/// The validator must reject the index and mention `needle`.
fn python_validator_rejects(index: &Path, extra: &[&str], needle: &str) {
    if let Some(out) = python_validator(index, extra) {
        assert!(!out.status.success(), "validator accepted:\n{}", text(&out));
        assert!(text(&out).contains(needle), "{}", text(&out));
    }
}
