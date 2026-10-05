//! `forge publish --registry <index-clone>` and `forge yank`: write to a
//! local clone of a sparse-index repository (rfcs/0007).
//!
//! Publishing produces two artifacts:
//!
//! 1. a deterministic `<name>-<version>.tar.gz` archive (to upload as a
//!    GitHub release asset at the entry's `url`), and
//! 2. a commit on a `publish/<name>-<version>` branch of the index clone
//!    that appends the version's entry and refreshes `index.toml`, ready to
//!    push and open as a pull request. The index repository's CI
//!    re-validates the entry (format, checksum, signature, ownership).
//!
//! No credentials are involved: write access is the pull request.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::manifest::{self, DependencySpec};
use crate::registry::client::sha256_hex;
use crate::registry::index::{self, IndexDep, IndexEntry, Owners};
use crate::registry::{signing, PackageIndex, PackageSummary};

pub struct IndexPublishOptions<'a> {
    pub project_dir: &'a Path,
    pub index_dir: &'a Path,
    pub dry_run: bool,
    /// Sign with the publisher key (created on first use).
    pub sign: bool,
    /// Override for the signing key path (default: `signing::default_key_path`).
    pub key_path: Option<PathBuf>,
    /// Archive URL template (`{name}`, `{vers}`); default derives a GitHub
    /// release URL from `project.repository`.
    pub download_url: Option<String>,
    /// Where the archive is written (default `<project>/dist`).
    pub out_dir: Option<PathBuf>,
    /// Create a branch + commit when the index is a git work tree.
    pub commit: bool,
}

#[derive(Debug)]
pub struct Published {
    pub entry: IndexEntry,
    /// Where the archive was (or, for a dry run, would be) written.
    pub archive: PathBuf,
    pub archive_size: usize,
    /// The index branch holding the commit, when one was made.
    pub branch: Option<String>,
    pub dry_run: bool,
}

/// A directory is treated as a sparse index when it has a `config.json`.
pub fn is_index_repo(path: &Path) -> bool {
    path.join(index::CONFIG_FILE).is_file()
}

/// Build the archive deterministically: sorted paths under a
/// `<name>-<version>/` prefix, zero timestamps/owners, fixed modes, and a
/// gzip header without mtime, so the same sources always hash the same.
pub fn build_archive(
    project_dir: &Path,
    name: &str,
    version: &str,
    files: &[PathBuf],
) -> Result<Vec<u8>, String> {
    let gz = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    let mut builder = tar::Builder::new(gz);
    builder.mode(tar::HeaderMode::Deterministic);
    let mut sorted: Vec<&PathBuf> = files.iter().collect();
    sorted.sort();
    for rel in sorted {
        let data = std::fs::read(project_dir.join(rel))
            .map_err(|e| format!("failed to read {}: {}", rel.display(), e))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(
                &mut header,
                format!("{}-{}/{}", name, version, rel_str),
                data.as_slice(),
            )
            .map_err(|e| format!("failed to archive {}: {}", rel_str, e))?;
    }
    let gz = builder
        .into_inner()
        .map_err(|e| format!("failed to finish archive: {}", e))?;
    gz.finish()
        .map_err(|e| format!("failed to compress archive: {}", e))
}

/// `https://github.com/<owner>/<repo>/releases/download/v{vers}/{name}-{vers}.tar.gz`
fn github_release_template(repository: &str) -> Option<String> {
    let url = url::Url::parse(repository.trim()).ok()?;
    if url.host_str()? != "github.com" {
        return None;
    }
    let mut parts = url.path_segments()?.filter(|s| !s.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?.trim_end_matches(".git");
    Some(format!(
        "https://github.com/{}/{}/releases/download/v{{vers}}/{{name}}-{{vers}}.tar.gz",
        owner, repo
    ))
}

fn registry_deps(manifest: &manifest::Manifest) -> Result<Vec<IndexDep>, String> {
    let mut deps = Vec::new();
    for (name, spec) in &manifest.dependencies {
        let req = match spec {
            DependencySpec::Version(v) => v.clone(),
            DependencySpec::Detailed(d) if d.git.is_empty() && d.path.is_empty() => {
                d.version.clone()
            }
            DependencySpec::Detailed(_) => {
                return Err(format!(
                    "dependency '{}' uses a git or path source; registry packages may only \
                     depend on registry packages",
                    name
                ))
            }
        };
        let req = if req.trim().is_empty() {
            "*".to_string()
        } else {
            req
        };
        deps.push(IndexDep {
            name: name.clone(),
            req,
        });
    }
    Ok(deps)
}

fn read_entries(index_dir: &Path, name: &str) -> Result<Vec<IndexEntry>, String> {
    let path = index_dir.join(index::index_path(name));
    match std::fs::read_to_string(&path) {
        Ok(text) => index::parse_entries(name, &text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("failed to read {}: {}", path.display(), e)),
    }
}

fn write_entries(index_dir: &Path, name: &str, entries: &[IndexEntry]) -> Result<(), String> {
    let path = index_dir.join(index::index_path(name));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    std::fs::write(&path, index::serialize_entries(entries))
        .map_err(|e| format!("failed to write {}: {}", path.display(), e))
}

/// Every package name in the index (the file names under `index/`).
fn all_names(index_dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if let Some(n) = path.file_name() {
                let n = n.to_string_lossy();
                if !n.starts_with('.') {
                    out.push(n.to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&index_dir.join(index::INDEX_DIR), &mut out);
    out.sort();
    out
}

fn read_owners(index_dir: &Path) -> Result<Owners, String> {
    let path = index_dir.join(index::OWNERS_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            toml::from_str(&text).map_err(|e| format!("{} is invalid: {}", path.display(), e))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Owners::default()),
        Err(e) => Err(format!("failed to read {}: {}", path.display(), e)),
    }
}

/// Refresh one package's row in `index.toml` (latest non-yanked version).
fn update_search_index(index_dir: &Path, name: &str, entries: &[IndexEntry]) -> Result<(), String> {
    let path = index_dir.join(index::SEARCH_INDEX_FILE);
    let mut summary: PackageIndex = match std::fs::read_to_string(&path) {
        Ok(text) => {
            toml::from_str(&text).map_err(|e| format!("{} is invalid: {}", path.display(), e))?
        }
        Err(_) => PackageIndex::default(),
    };
    summary.packages.retain(|p| p.name != name);
    let latest = entries
        .iter()
        .filter(|e| !e.yanked)
        .filter_map(|e| semver::Version::parse(&e.vers).ok().map(|v| (v, e)))
        .max_by(|a, b| a.0.cmp(&b.0));
    if let Some((_, e)) = latest {
        summary.packages.push(PackageSummary {
            name: name.to_string(),
            description: e.description.clone(),
            latest: e.vers.clone(),
        });
    }
    summary.packages.sort_by(|a, b| a.name.cmp(&b.name));
    let body = toml::to_string_pretty(&summary)
        .map_err(|e| format!("failed to encode index.toml: {}", e))?;
    let content = format!(
        "# Search summary maintained by `forge publish` / `forge yank`. Do not edit by hand.\n{}",
        body
    );
    std::fs::write(&path, content).map_err(|e| format!("failed to write {}: {}", path.display(), e))
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn is_git_work_tree(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--is-inside-work-tree"]).is_ok_and(|o| o == "true")
}

/// Create `branch` in a clean index clone (refusing to mix unrelated
/// changes into the commit).
fn start_branch(index_dir: &Path, branch: &str) -> Result<(), String> {
    let status = git(index_dir, &["status", "--porcelain"])?;
    if !status.is_empty() {
        return Err(format!(
            "the index clone {} has uncommitted changes; commit or stash them first",
            index_dir.display()
        ));
    }
    git(index_dir, &["checkout", "-b", branch]).map(|_| ())
}

fn commit(index_dir: &Path, paths: &[String], message: &str) -> Result<(), String> {
    let mut add = vec!["add", "--"];
    add.extend(paths.iter().map(String::as_str));
    git(index_dir, &add)?;
    git(index_dir, &["commit", "-q", "-m", message]).map(|_| ())
}

pub fn publish_to_index(opts: &IndexPublishOptions) -> Result<Published, String> {
    let manifest_path = opts.project_dir.join("forge.toml");
    let manifest = manifest::load_manifest_from(&manifest_path)
        .ok_or_else(|| format!("no forge.toml found in {}", opts.project_dir.display()))?;
    crate::publish::validate_manifest(&manifest)?;
    let name = manifest.project.name.clone();
    let vers = manifest.project.version.clone();
    index::validate_name(&name)?;
    semver::Version::parse(&vers)
        .map_err(|e| format!("project.version '{}' is not valid semver: {}", vers, e))?;
    let deps = registry_deps(&manifest)?;

    let files = crate::publish::collect_files(opts.project_dir);
    if !files
        .iter()
        .any(|f| f.extension().is_some_and(|e| e == "fg"))
    {
        return Err("no .fg files found to publish".into());
    }
    let archive_bytes = build_archive(opts.project_dir, &name, &vers, &files)?;
    let cksum = sha256_hex(&archive_bytes);

    let template = match &opts.download_url {
        Some(t) => t.clone(),
        None => github_release_template(&manifest.project.repository).ok_or_else(|| {
            "cannot derive a download URL: set project.repository to a github.com URL \
             or pass --download-url '<url with {name} and {vers}>'"
                .to_string()
        })?,
    };
    let url = template.replace("{name}", &name).replace("{vers}", &vers);

    let mut entry = IndexEntry {
        v: index::INDEX_FORMAT_VERSION,
        name: name.clone(),
        vers: vers.clone(),
        deps,
        cksum,
        url,
        yanked: false,
        pubkey: None,
        sig: None,
        description: manifest.project.description.clone(),
        license: manifest.project.license.clone(),
        published: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    };

    let key_path = opts
        .key_path
        .clone()
        .unwrap_or_else(signing::default_key_path);
    // A dry run never creates a key; it signs only with an existing one.
    if opts.sign && !(opts.dry_run && !key_path.exists()) {
        let (key, created) = signing::load_or_create_key(&key_path)?;
        if created {
            crate::color::cprintln!(
                "  Created signing key {} (keep it private, back it up)",
                key_path.display()
            );
        }
        signing::sign_entry(&key, &mut entry);
    }

    let mut entries = read_entries(opts.index_dir, &name)?;
    let owners = read_owners(opts.index_dir)?;
    index::check_publish_allowed(&entry, &entries, &all_names(opts.index_dir), &owners)?;

    let out_dir = opts
        .out_dir
        .clone()
        .unwrap_or_else(|| opts.project_dir.join("dist"));
    let archive = out_dir.join(format!("{}-{}.tar.gz", name, vers));

    if opts.dry_run {
        return Ok(Published {
            entry,
            archive,
            archive_size: archive_bytes.len(),
            branch: None,
            dry_run: true,
        });
    }

    let use_git = opts.commit && is_git_work_tree(opts.index_dir);
    let branch = format!("publish/{}-{}", name, vers);
    if use_git {
        start_branch(opts.index_dir, &branch)?;
    }

    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create {}: {}", out_dir.display(), e))?;
    let mut f = std::fs::File::create(&archive)
        .map_err(|e| format!("failed to create {}: {}", archive.display(), e))?;
    f.write_all(&archive_bytes)
        .map_err(|e| format!("failed to write {}: {}", archive.display(), e))?;

    entries.push(entry.clone());
    write_entries(opts.index_dir, &name, &entries)?;
    update_search_index(opts.index_dir, &name, &entries)?;

    if use_git {
        commit(
            opts.index_dir,
            &[
                index::index_path(&name),
                index::SEARCH_INDEX_FILE.to_string(),
            ],
            &format!("publish {}@{}", name, vers),
        )?;
    }

    Ok(Published {
        entry,
        archive,
        archive_size: archive_bytes.len(),
        branch: use_git.then_some(branch),
        dry_run: false,
    })
}

/// Print the outcome of [`publish_to_index`] for the CLI.
pub fn print_report(index_dir: &Path, p: &Published) {
    let e = &p.entry;
    if p.dry_run {
        crate::color::cprintln!(
            "  Would publish {} v{} to index {}",
            e.name,
            e.vers,
            index_dir.display()
        );
        crate::color::cprintln!(
            "    Archive: {} ({} bytes)",
            p.archive.display(),
            p.archive_size
        );
        crate::color::cprintln!("    URL: {}", e.url);
        crate::color::cprintln!("    SHA-256: {}", e.cksum);
        return;
    }
    crate::color::cprintln!(
        "  \x1B[32m✓\x1B[0m Added {} v{} to index {}",
        e.name,
        e.vers,
        index_dir.display()
    );
    crate::color::cprintln!(
        "    Archive: {} ({} bytes)",
        p.archive.display(),
        p.archive_size
    );
    crate::color::cprintln!("    SHA-256: {}", e.cksum);
    if let Some(pk) = &e.pubkey {
        crate::color::cprintln!("    Signed by: {}", pk);
    }
    crate::color::cprintln!("  Next steps:");
    crate::color::cprintln!("    1. Upload the archive so it is served at {}", e.url);
    match &p.branch {
        Some(branch) => crate::color::cprintln!(
            "    2. Push branch '{}' and open a pull request against the index",
            branch
        ),
        None => crate::color::cprintln!(
            "    2. Commit {} and index.toml, then open a pull request",
            index::index_path(&e.name)
        ),
    }
}

/// Flip the `yanked` flag of `name@vers` in a local index clone.
pub fn yank(
    index_dir: &Path,
    name: &str,
    vers: &str,
    undo: bool,
    commit_changes: bool,
) -> Result<Option<String>, String> {
    if !is_index_repo(index_dir) {
        return Err(format!(
            "{} is not a sparse index clone (no config.json)",
            index_dir.display()
        ));
    }
    index::validate_name(name)?;
    let mut entries = read_entries(index_dir, name)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.vers == vers)
        .ok_or_else(|| format!("{}@{} is not in the index", name, vers))?;
    if entry.yanked != undo {
        return Err(format!(
            "{}@{} is already {}",
            name,
            vers,
            if undo { "not yanked" } else { "yanked" }
        ));
    }
    entry.yanked = !undo;

    let use_git = commit_changes && is_git_work_tree(index_dir);
    let action = if undo { "unyank" } else { "yank" };
    let branch = format!("{}/{}-{}", action, name, vers);
    if use_git {
        start_branch(index_dir, &branch)?;
    }
    write_entries(index_dir, name, &entries)?;
    update_search_index(index_dir, name, &entries)?;
    if use_git {
        commit(
            index_dir,
            &[
                index::index_path(name),
                index::SEARCH_INDEX_FILE.to_string(),
            ],
            &format!("{} {}@{}", action, name, vers),
        )?;
    }
    Ok(use_git.then_some(branch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "forge-pubidx-{}-{}-{}",
            tag,
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn project(dir: &Path, name: &str, version: &str, deps: &str) {
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("forge.toml"),
            format!(
                "[project]\nname = \"{}\"\nversion = \"{}\"\ndescription = \"demo\"\nlicense = \"MIT\"\n\
                 repository = \"https://github.com/acme/{}\"\n\n[dependencies]\n{}",
                name, version, name, deps
            ),
        )
        .unwrap();
        std::fs::write(dir.join("main.fg"), "say \"hi\"\n").unwrap();
        std::fs::write(dir.join("src").join("lib.fg"), "fn f() { 1 }\n").unwrap();
    }

    fn index_repo(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("config.json"), "{\"v\":1}\n").unwrap();
    }

    fn opts<'a>(project: &'a Path, index_dir: &'a Path) -> IndexPublishOptions<'a> {
        IndexPublishOptions {
            project_dir: project,
            index_dir,
            dry_run: false,
            sign: false,
            key_path: None,
            download_url: None,
            out_dir: None,
            commit: false,
        }
    }

    #[test]
    fn archives_are_deterministic() {
        let root = temp("determinism");
        let p = root.join("p");
        project(&p, "kv", "0.1.0", "");
        let files = crate::publish::collect_files(&p);
        let a = build_archive(&p, "kv", "0.1.0", &files).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(p.join("main.fg"), "say \"hi\"\n").unwrap(); // new mtime
        let b = build_archive(&p, "kv", "0.1.0", &files).unwrap();
        assert_eq!(sha256_hex(&a), sha256_hex(&b));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn publish_appends_entry_and_search_row() {
        let root = temp("publish");
        let p = root.join("p");
        let idx = root.join("index");
        project(&p, "kv", "0.1.0", "json-utils = \"^1.0\"\n");
        index_repo(&idx);

        let mut o = opts(&p, &idx);
        o.sign = true;
        o.key_path = Some(root.join("key"));
        let published = publish_to_index(&o).unwrap();
        assert_eq!(
            published.entry.url,
            "https://github.com/acme/kv/releases/download/v0.1.0/kv-0.1.0.tar.gz"
        );
        assert_eq!(published.entry.deps[0].name, "json-utils");
        let bytes = std::fs::read(&published.archive).unwrap();
        assert_eq!(sha256_hex(&bytes), published.entry.cksum);
        assert!(signing::verify_entry(&published.entry).unwrap().is_some());

        let entries = read_entries(&idx, "kv").unwrap();
        assert_eq!(entries, vec![published.entry.clone()]);
        let summary: PackageIndex =
            toml::from_str(&std::fs::read_to_string(idx.join("index.toml")).unwrap()).unwrap();
        assert_eq!(summary.packages[0].latest, "0.1.0");

        // Same version again: immutable.
        let err = publish_to_index(&o).unwrap_err();
        assert!(err.contains("immutable"), "{err}");

        // Yank / unyank.
        yank(&idx, "kv", "0.1.0", false, false).unwrap();
        assert!(read_entries(&idx, "kv").unwrap()[0].yanked);
        let summary: PackageIndex =
            toml::from_str(&std::fs::read_to_string(idx.join("index.toml")).unwrap()).unwrap();
        assert!(summary.packages.is_empty(), "no installable version left");
        assert!(yank(&idx, "kv", "0.1.0", false, false)
            .unwrap_err()
            .contains("already yanked"));
        yank(&idx, "kv", "0.1.0", true, false).unwrap();
        assert!(!read_entries(&idx, "kv").unwrap()[0].yanked);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dry_run_has_no_side_effects() {
        let root = temp("dryrun");
        let p = root.join("p");
        let idx = root.join("index");
        project(&p, "kv", "0.1.0", "");
        index_repo(&idx);
        let mut o = opts(&p, &idx);
        o.dry_run = true;
        o.sign = true;
        o.key_path = Some(root.join("key"));
        let published = publish_to_index(&o).unwrap();
        assert!(published.dry_run);
        assert!(published.archive_size > 0);
        assert!(!root.join("key").exists(), "dry run must not create a key");
        assert!(!published.archive.exists());
        assert!(read_entries(&idx, "kv").unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn publish_rejects_unpublishable_projects() {
        let root = temp("reject");
        let idx = root.join("index");
        index_repo(&idx);

        let p = root.join("gitdep");
        project(
            &p,
            "kv",
            "0.1.0",
            "x = { git = \"https://example.com/x.git\" }\n",
        );
        assert!(publish_to_index(&opts(&p, &idx))
            .unwrap_err()
            .contains("git or path source"));

        let p = root.join("upper");
        project(&p, "KV", "0.1.0", "");
        assert!(publish_to_index(&opts(&p, &idx))
            .unwrap_err()
            .contains("lowercase"));

        let p = root.join("badver");
        project(&p, "kv", "one", "");
        assert!(publish_to_index(&opts(&p, &idx))
            .unwrap_err()
            .contains("semver"));

        let p = root.join("nourl");
        project(&p, "kv", "0.1.0", "");
        std::fs::write(
            p.join("forge.toml"),
            "[project]\nname = \"kv\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert!(publish_to_index(&opts(&p, &idx))
            .unwrap_err()
            .contains("--download-url"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn github_template_derivation() {
        assert_eq!(
            github_release_template("https://github.com/o/r.git").as_deref(),
            Some("https://github.com/o/r/releases/download/v{vers}/{name}-{vers}.tar.gz")
        );
        assert_eq!(github_release_template("https://gitlab.com/o/r"), None);
        assert_eq!(github_release_template(""), None);
    }
}
