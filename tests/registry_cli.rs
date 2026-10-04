//! CLI-level tests for `forge search` and `forge add`.
//!
//! These run the real `forge` binary with an isolated HOME and a remote
//! registry URL that refuses connections, so they never touch the network.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "forge-registry-cli-{}-{}-{}",
        tag,
        std::process::id(),
        unique
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// A URL nothing listens on: bind an ephemeral port, then release it.
fn unreachable_registry_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    format!("http://127.0.0.1:{}", port)
}

fn forge(home: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("FORGE_REGISTRY_URL", unreachable_registry_url())
        .env_remove("FORGE_REGISTRY_PATH")
        .env_remove("GITHUB_TOKEN")
        .output()
        .expect("run forge")
}

fn publish_local(home: &Path, name: &str, version: &str, description: &str) {
    let dir = home
        .join(".forge")
        .join("registry")
        .join(name)
        .join(version);
    std::fs::create_dir_all(&dir).expect("registry dir");
    std::fs::write(
        dir.join("forge.toml"),
        format!(
            "[project]\nname = \"{}\"\nversion = \"{}\"\ndescription = \"{}\"\n",
            name, version, description
        ),
    )
    .expect("write manifest");
    std::fs::write(dir.join("main.fg"), "say \"hi\"\n").expect("write main");
}

#[test]
fn search_lists_local_packages_when_remote_is_unreachable() {
    let root = temp_dir("search");
    let home = root.join("home");
    publish_local(&home, "router", "1.2.0", "Local HTTP router");

    let out = forge(&home, &root, &["search", "rout"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !stderr.contains("panicked"),
        "forge search panicked: {stderr}"
    );
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert!(stderr.contains("unreachable"), "stderr: {stderr}");
    assert!(stdout.contains("router"), "stdout: {stdout}");
    assert!(stdout.contains("1.2.0"), "stdout: {stdout}");
    assert!(stdout.contains("local"), "stdout: {stdout}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn search_with_no_results_and_no_remote_fails_with_clear_message() {
    let root = temp_dir("search-empty");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");

    let out = forge(&home, &root, &["search", "nothing-here"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !stderr.contains("panicked"),
        "forge search panicked: {stderr}"
    );
    assert!(!out.status.success());
    assert!(stderr.contains("unreachable"), "stderr: {stderr}");
    assert!(stdout.contains("No packages found"), "stdout: {stdout}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn add_does_not_modify_manifest_when_install_fails() {
    let root = temp_dir("add-fail");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let original = "[project]\nname = \"app\"\n";
    std::fs::write(root.join("forge.toml"), original).expect("manifest");

    let out = forge(&home, &root, &["add", "router"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!stderr.contains("panicked"), "forge add panicked: {stderr}");
    assert!(!out.status.success(), "add of a missing package must fail");
    assert!(stderr.contains("forge.toml was not modified"), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(root.join("forge.toml")).expect("read"),
        original
    );
    assert!(!root.join("forge.lock").exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn add_installs_from_local_registry_and_records_dependency() {
    let root = temp_dir("add-ok");
    let home = root.join("home");
    publish_local(&home, "router", "1.2.0", "Local HTTP router");
    std::fs::write(root.join("forge.toml"), "[project]\nname = \"app\"\n").expect("manifest");

    let out = forge(&home, &root, &["add", "router@^1.0"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");

    let manifest = std::fs::read_to_string(root.join("forge.toml")).expect("read");
    assert!(manifest.contains("router = \"^1.0\""), "{manifest}");
    assert!(root
        .join("forge_modules")
        .join("router")
        .join("main.fg")
        .exists());
    assert!(root.join("forge.lock").exists());

    let _ = std::fs::remove_dir_all(&root);
}
