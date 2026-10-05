//! The checker must not cry wolf: every program in the repository's test
//! and example corpus runs correctly, so checking it must produce no errors
//! in default mode, and in strict mode no errors beyond a curated allowlist
//! (`tests/typecheck_allowlist.txt`) where each entry says why the code is
//! intentionally "wrong".

use super::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn corpus_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in [
        "tests",
        "examples",
        "tests/parity/supported",
        "tests/parity/modules",
    ] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "fg") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn relative(path: &Path) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

/// `path:line code` keys of the allowlist (comments after `#` are the
/// mandatory justification).
fn allowlist() -> BTreeSet<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("tests/typecheck_allowlist.txt"))
        .expect("tests/typecheck_allowlist.txt exists");
    let mut keys = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, why) = line.split_once('#').unwrap_or((line, ""));
        assert!(
            !why.trim().is_empty(),
            "allowlist entry '{}' needs a justification after '#'",
            line
        );
        keys.insert(key.trim().to_string());
    }
    keys
}

fn check(path: &Path, strict: bool) -> Vec<Diagnostic> {
    let source = std::fs::read_to_string(path).expect("read corpus file");
    let options = CheckOptions {
        strict,
        file: Some(path.to_path_buf()),
    };
    match analyze(&source, &options) {
        Ok(a) => a.diagnostics,
        Err(e) => panic!("{} does not parse: {:?}", path.display(), e),
    }
}

#[test]
fn corpus_has_no_errors_in_default_mode() {
    let files = corpus_files();
    assert!(files.len() > 40, "corpus not found");
    for path in &files {
        let errors: Vec<String> = check(path, false)
            .iter()
            .filter(|d| d.is_error())
            .map(|d| format!("[{}] {}", d.code, d.full_message()))
            .collect();
        assert!(errors.is_empty(), "{}: {:?}", path.display(), errors);
    }
}

#[test]
fn corpus_strict_errors_are_exactly_the_allowlist() {
    let allowed = allowlist();
    let mut found = BTreeSet::new();
    let mut unexpected = Vec::new();
    for path in corpus_files() {
        for d in check(&path, true) {
            let key = format!("{}:{} {}", relative(&path), d.line(), d.code);
            if !allowed.contains(&key) {
                unexpected.push(format!("{}  {}", key, d.full_message()));
            }
            found.insert(key);
        }
    }
    let stale: Vec<&String> = allowed.difference(&found).collect();
    assert!(
        unexpected.is_empty(),
        "strict-mode errors not in tests/typecheck_allowlist.txt:\n{}",
        unexpected.join("\n")
    );
    assert!(
        stale.is_empty(),
        "stale allowlist entries (no longer reported): {:?}",
        stale
    );
}
