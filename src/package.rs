use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::manifest::{self, DependencySpec, LockedPackage, Lockfile, Manifest};
use semver::{Version, VersionReq};

const PACKAGES_DIR: &str = "forge_modules";

pub fn install(source: &str) {
    if source == "." || source.is_empty() {
        install_from_manifest();
        return;
    }

    let packages_dir = Path::new(PACKAGES_DIR);
    if let Err(e) = std::fs::create_dir_all(packages_dir) {
        eprintln!("Error: failed to create packages directory: {}", e);
        std::process::exit(1);
    }

    let result = if source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("git@")
    {
        let name = package_name_from_source(source);
        install_from_git_as(&name, source, None, packages_dir)
    } else {
        let name = package_name_from_source(source);
        install_from_local_as(&name, source, packages_dir)
    };

    if let Err(message) = result {
        eprintln!("{}", message);
        std::process::exit(1);
    }
}

pub fn install_from_manifest() {
    let manifest_path = Path::new("forge.toml");
    let manifest = match manifest::load_manifest_from(manifest_path) {
        Some(m) => m,
        None => {
            eprintln!("No forge.toml found in current directory");
            std::process::exit(1);
        }
    };

    let packages_dir = Path::new(PACKAGES_DIR);
    let lockfile_path = Path::new("forge.lock");
    let registry_roots = default_registry_roots();
    let manifest_root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    match install_manifest_dependencies(
        &manifest,
        manifest_root,
        packages_dir,
        lockfile_path,
        &registry_roots,
    ) {
        Ok(summary) => {
            println!(
                "  {} dependencies processed for '{}'",
                summary.processed, manifest.project.name
            );
            if summary.installed > 0 {
                println!(
                    "  Updated forge.lock ({} packages)",
                    summary.locked_packages
                );
            }
        }
        Err(message) => {
            eprintln!("{}", message);
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InstallSummary {
    processed: usize,
    installed: usize,
    locked_packages: usize,
}

fn install_manifest_dependencies(
    manifest: &Manifest,
    manifest_root: &Path,
    packages_dir: &Path,
    lockfile_path: &Path,
    registry_roots: &[PathBuf],
) -> Result<InstallSummary, String> {
    if manifest.dependencies.is_empty() {
        println!("  No dependencies to install.");
        return Ok(InstallSummary {
            processed: 0,
            installed: 0,
            locked_packages: 0,
        });
    }

    std::fs::create_dir_all(packages_dir)
        .map_err(|e| format!("Error: failed to create forge_modules/: {}", e))?;

    let mut lockfile = load_lockfile_from(lockfile_path).unwrap_or_default();
    let mut installed = 0;

    // Track the root project to detect cycles back to it
    let mut visiting = vec![manifest.project.name.clone()];

    for (name, spec) in &manifest.dependencies {
        let locked =
            install_single_dependency(name, spec, manifest_root, packages_dir, registry_roots)?;

        lockfile.packages.retain(|p| p.name != *name);
        lockfile.packages.push(locked);
        installed += 1;

        // Resolve transitive dependencies
        let transitive = resolve_transitive(name, packages_dir, registry_roots, &mut visiting)?;
        for tlocked in transitive {
            lockfile.packages.retain(|p| p.name != tlocked.name);
            lockfile.packages.push(tlocked);
            installed += 1;
        }
    }

    save_lockfile_at(&lockfile, lockfile_path)
        .map_err(|e| format!("Warning: failed to write forge.lock: {}", e))?;

    Ok(InstallSummary {
        processed: manifest.dependencies.len(),
        installed,
        locked_packages: lockfile.packages.len(),
    })
}

fn package_name_from_source(source: &str) -> String {
    Path::new(source)
        .file_stem()
        .or_else(|| Path::new(source).file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "package".to_string())
}

fn install_single_dependency(
    name: &str,
    spec: &DependencySpec,
    context_dir: &Path,
    packages_dir: &Path,
    registry_roots: &[PathBuf],
) -> Result<LockedPackage, String> {
    match spec {
        DependencySpec::Version(ver) => {
            install_from_registry_as(name, ver, packages_dir, registry_roots)
        }
        DependencySpec::Detailed(dep) if !dep.git.is_empty() => {
            let branch = if dep.branch.is_empty() {
                None
            } else {
                Some(dep.branch.as_str())
            };
            install_from_git_as(name, &dep.git, branch, packages_dir)?;
            Ok(LockedPackage {
                name: name.to_string(),
                version: dep.version.clone(),
                source: format!("git+{}", dep.git),
                checksum: get_git_rev(packages_dir, name),
            })
        }
        DependencySpec::Detailed(dep) if !dep.path.is_empty() => {
            let source_path = context_dir.join(&dep.path);
            install_from_path_as(name, &source_path, packages_dir)?;
            Ok(LockedPackage {
                name: name.to_string(),
                version: dep.version.clone(),
                source: format!("path+{}", source_path.display()),
                checksum: String::new(),
            })
        }
        DependencySpec::Detailed(dep) if !dep.version.is_empty() => {
            install_from_registry_as(name, &dep.version, packages_dir, registry_roots)
        }
        DependencySpec::Detailed(_) => Err(format!(
            "  Error: dependency '{}' has no supported source. Use version, path, or git.",
            name
        )),
    }
}

fn resolve_transitive(
    parent: &str,
    packages_dir: &Path,
    registry_roots: &[PathBuf],
    visiting: &mut Vec<String>,
) -> Result<Vec<LockedPackage>, String> {
    let pkg_dir = packages_dir.join(parent);
    let manifest_path = pkg_dir.join("forge.toml");

    let sub_manifest = match manifest::load_manifest_from(&manifest_path) {
        Some(m) => m,
        None => return Ok(Vec::new()), // Leaf package, no transitive deps
    };

    if sub_manifest.dependencies.is_empty() {
        return Ok(Vec::new());
    }

    visiting.push(parent.to_string());

    let mut results = Vec::new();

    for (dep_name, dep_spec) in &sub_manifest.dependencies {
        // Cycle detection: check before install skip
        if visiting.iter().any(|v| v == dep_name) {
            let chain = visiting.join(" -> ");
            return Err(format!(
                "  Error: circular dependency detected: {} -> {}",
                chain, dep_name
            ));
        }

        // Skip if already installed (diamond dependency)
        if packages_dir.join(dep_name).exists() {
            continue;
        }

        // Resolve relative to the installed package's directory
        let locked =
            install_single_dependency(dep_name, dep_spec, &pkg_dir, packages_dir, registry_roots)?;
        results.push(locked);

        // Recurse into this transitive dep's own dependencies
        let nested = resolve_transitive(dep_name, packages_dir, registry_roots, visiting)?;
        results.extend(nested);
    }

    visiting.pop();
    Ok(results)
}

fn default_registry_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(paths) = env::var_os("FORGE_REGISTRY_PATH") {
        roots.extend(env::split_paths(&paths));
    }
    roots.push(PathBuf::from(".forge/registry"));
    // Global user registry (~/.forge/registry/)
    if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
        roots.push(PathBuf::from(home).join(".forge").join("registry"));
    }
    roots
}

/// Check if a package exists in any of the given registry roots.
/// Used by `forge publish` to verify the published package is findable.
pub fn find_in_registry(name: &str, version: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    for root in roots {
        for candidate in registry_candidates(root, name, version) {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn load_lockfile_from(path: &Path) -> Option<Lockfile> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

fn save_lockfile_at(lockfile: &Lockfile, path: &Path) -> std::io::Result<()> {
    let content = toml::to_string_pretty(lockfile)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, content)
}

fn install_from_registry_as(
    name: &str,
    version_str: &str,
    packages_dir: &Path,
    registry_roots: &[PathBuf],
) -> Result<LockedPackage, String> {
    let req = if version_str.is_empty() || version_str == "*" {
        VersionReq::STAR
    } else {
        VersionReq::parse(version_str).map_err(|e| {
            format!(
                "  Error: invalid version constraint '{}': {}",
                version_str, e
            )
        })?
    };

    // Try local registry first
    if let Ok((resolved_version, source)) = resolve_best_version(name, &req, registry_roots) {
        install_from_path_as(name, &source, packages_dir)?;
        println!(
            "  \x1B[32m✓\x1B[0m Installed {} @ {}",
            name, resolved_version
        );
        return Ok(LockedPackage {
            name: name.to_string(),
            version: resolved_version,
            source: format!("registry+{}", source.display()),
            checksum: String::new(),
        });
    }

    // Fall back to remote registry
    install_from_remote_registry(name, &req, packages_dir)
}

fn install_from_remote_registry(
    name: &str,
    req: &VersionReq,
    packages_dir: &Path,
) -> Result<LockedPackage, String> {
    use crate::registry;

    validate_package_name(name)?;

    let entry = match registry::fetch_package_entry(name) {
        Ok(Some(e)) => e,
        Ok(None) => {
            return Err(format!(
                "  Error: package '{}' not found in local or remote registry",
                name
            ));
        }
        Err(e) => {
            return Err(format!(
                "  Error: failed to fetch '{}' from remote registry: {}",
                name, e
            ));
        }
    };

    let resolved = registry::resolve_remote_version(name, req, &entry.versions)?;

    if resolved.checksum.is_empty() {
        eprintln!(
            "  Warning: no checksum for {} @ {} — integrity not verified",
            name, resolved.version
        );
    }

    println!(
        "  Downloading {} @ {} from remote registry...",
        name, resolved.version
    );
    registry::download_and_extract(&resolved.url, &packages_dir.join(name), &resolved.checksum)?;
    println!(
        "  \x1B[32m✓\x1B[0m Installed {} @ {} (remote)",
        name, resolved.version
    );

    Ok(LockedPackage {
        name: name.to_string(),
        version: resolved.version,
        source: format!("remote+{}", resolved.url),
        checksum: resolved.checksum,
    })
}

fn validate_package_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return Err(format!(
            "  Error: invalid package name '{}': must not be empty or contain '/' or '..'",
            name
        ));
    }
    Ok(())
}

fn resolve_best_version(
    name: &str,
    req: &VersionReq,
    registry_roots: &[PathBuf],
) -> Result<(String, PathBuf), String> {
    validate_package_name(name)?;

    let mut all_versions: Vec<(Version, PathBuf)> = Vec::new();

    for root in registry_roots {
        let pkg_dir = root.join(name);
        let entries = match std::fs::read_dir(&pkg_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if !entry.file_type().map_or(false, |t| t.is_dir()) {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if let Ok(ver) = Version::parse(&dir_name) {
                all_versions.push((ver, entry.path()));
            }
        }
    }

    if all_versions.is_empty() {
        return Err(format!(
            "  Error: no versions found for '{}' in registry",
            name
        ));
    }

    let best = all_versions
        .iter()
        .filter(|(v, _)| req.matches(v))
        .max_by(|(a, _), (b, _)| a.cmp(b));

    match best {
        Some((ver, path)) => Ok((ver.to_string(), path.clone())),
        None => {
            let available: Vec<String> = {
                let mut vs: Vec<&Version> = all_versions.iter().map(|(v, _)| v).collect();
                vs.sort();
                vs.iter().map(|v| v.to_string()).collect()
            };
            Err(format!(
                "  Error: no version of '{}' matches '{}' (available: {})",
                name,
                req,
                available.join(", ")
            ))
        }
    }
}

fn find_registry_package(
    name: &str,
    version: &str,
    registry_roots: &[PathBuf],
) -> Result<PathBuf, String> {
    let mut searched = Vec::new();
    for root in registry_roots {
        for candidate in registry_candidates(root, name, version) {
            searched.push(candidate.display().to_string());
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(format!(
        "  Error: registry package '{}@{}' not found. Searched: {}",
        name,
        version,
        searched.join(", ")
    ))
}

fn registry_candidates(root: &Path, name: &str, version: &str) -> Vec<PathBuf> {
    vec![
        root.join(name).join(version),
        root.join(format!("{}-{}", name, version)),
        root.join(name).join(version).join("package"),
    ]
}

fn install_from_path_as(name: &str, src: &Path, packages_dir: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("  Error: '{}' not found", src.display()));
    }

    let target = packages_dir.join(name);
    if target.exists() {
        remove_path(&target)
            .map_err(|e| format!("Error: failed to remove existing package: {}", e))?;
    }

    if src.is_dir() {
        copy_dir_all(src, &target).map_err(|e| format!("Error: failed to copy package: {}", e))?;
    } else {
        std::fs::create_dir_all(&target)
            .map_err(|e| format!("Error: failed to create package directory: {}", e))?;
        std::fs::copy(src, target.join("main.fg"))
            .map_err(|e| format!("Error: failed to copy package file: {}", e))?;
    }

    Ok(())
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn get_git_rev(packages_dir: &Path, name: &str) -> String {
    let target = packages_dir.join(name);
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&target)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn install_from_git_as(
    name: &str,
    url: &str,
    branch: Option<&str>,
    packages_dir: &Path,
) -> Result<(), String> {
    let target = packages_dir.join(name);

    if target.exists() {
        println!("  Updating {}...", name);
        let status = Command::new("git")
            .args(["pull"])
            .current_dir(&target)
            .status();
        match status {
            Ok(s) if s.success() => println!("  \x1B[32m✓\x1B[0m Updated {}", name),
            _ => return Err(format!("  \x1B[31m✗\x1B[0m Failed to update {}", name)),
        }
    } else {
        println!("  Installing {} from {}...", name, url);
        let target_str = target.display().to_string();
        let mut args = vec!["clone"];
        if let Some(b) = branch {
            args.push("--branch");
            args.push(b);
        }
        args.push("--depth");
        args.push("1");
        args.push(url);
        args.push(&target_str);

        let status = Command::new("git").args(&args).status();
        match status {
            Ok(s) if s.success() => {
                println!("  \x1B[32m✓\x1B[0m Installed {}", name);
            }
            _ => {
                return Err(format!("  \x1B[31m✗\x1B[0m Failed to clone {}", url));
            }
        }
    }

    Ok(())
}

fn install_from_local_as(name: &str, source: &str, packages_dir: &Path) -> Result<(), String> {
    let src = Path::new(source);
    install_from_path_as(name, src, packages_dir)?;
    println!("  \x1B[32m✓\x1B[0m Installed {} from {}", name, source);
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

/// Resolve an import path, checking relative to the importing file first,
/// then forge_modules/ and .forge/packages/.
///
/// `base_dir` is the directory of the file containing the import statement.
/// When `None`, only CWD-relative and package paths are checked.
pub fn resolve_import(path: &str) -> Option<PathBuf> {
    resolve_import_from(path, None)
}

pub fn resolve_import_from(path: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
    // If a base directory is provided, check relative to it first
    if let Some(base) = base_dir {
        let relative = base.join(path);
        if relative.exists() {
            return Some(relative);
        }
        let relative_fg = base.join(format!("{}.fg", path));
        if relative_fg.exists() {
            return Some(relative_fg);
        }
    }

    // Direct file path (CWD-relative)
    let direct = Path::new(path);
    if direct.exists() {
        return Some(direct.to_path_buf());
    }

    let with_ext = PathBuf::from(format!("{}.fg", path));
    if with_ext.exists() {
        return Some(with_ext);
    }

    // forge_modules/<name>/main.fg
    let pkg_main = Path::new(PACKAGES_DIR).join(path).join("main.fg");
    if pkg_main.exists() {
        return Some(pkg_main);
    }

    // forge_modules/<name>.fg
    let pkg_file = Path::new(PACKAGES_DIR).join(format!("{}.fg", path));
    if pkg_file.exists() {
        return Some(pkg_file);
    }

    // forge_modules/<name>/src/main.fg
    let pkg_src_main = Path::new(PACKAGES_DIR)
        .join(path)
        .join("src")
        .join("main.fg");
    if pkg_src_main.exists() {
        return Some(pkg_src_main);
    }

    // Legacy: .forge/packages/<name>/main.fg
    let legacy_main = Path::new(".forge/packages").join(path).join("main.fg");
    if legacy_main.exists() {
        return Some(legacy_main);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("forge-package-{}-{}", prefix, unique))
    }

    #[test]
    fn install_manifest_uses_dependency_name_as_install_target() {
        let workspace = temp_path("target");
        let package_src = workspace.join("pkg-src");
        let packages_dir = workspace.join(PACKAGES_DIR);
        let lockfile_path = workspace.join("forge.lock");
        std::fs::create_dir_all(&package_src).unwrap();
        std::fs::write(package_src.join("main.fg"), "println(\"hi\")").unwrap();

        let manifest: Manifest = toml::from_str(
            r#"
[project]
name = "app"

[dependencies]
toolkit = { path = "pkg-src" }
"#,
        )
        .unwrap();

        let summary = install_manifest_dependencies(
            &manifest,
            &workspace,
            &packages_dir,
            &lockfile_path,
            &[],
        )
        .unwrap();
        assert_eq!(summary.installed, 1);
        assert!(packages_dir.join("toolkit").join("main.fg").exists());

        let lockfile = load_lockfile_from(&lockfile_path).unwrap();
        let package = lockfile.find("toolkit").unwrap();
        assert_eq!(
            package.source,
            format!("path+{}", workspace.join("pkg-src").display())
        );

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn install_manifest_resolves_version_dependencies_from_registry() {
        let workspace = temp_path("registry");
        let registry_root = workspace.join("registry");
        let packages_dir = workspace.join(PACKAGES_DIR);
        let lockfile_path = workspace.join("forge.lock");
        let registry_pkg = registry_root.join("toolkit").join("1.2.3");
        std::fs::create_dir_all(&registry_pkg).unwrap();
        std::fs::write(registry_pkg.join("main.fg"), "println(\"hi\")").unwrap();

        let manifest: Manifest = toml::from_str(
            r#"
[project]
name = "app"

[dependencies]
toolkit = "1.2.3"
"#,
        )
        .unwrap();

        let summary = install_manifest_dependencies(
            &manifest,
            &workspace,
            &packages_dir,
            &lockfile_path,
            &[registry_root.clone()],
        )
        .unwrap();
        assert_eq!(summary.installed, 1);
        assert!(packages_dir.join("toolkit").join("main.fg").exists());

        let lockfile = load_lockfile_from(&lockfile_path).unwrap();
        let package = lockfile.find("toolkit").unwrap();
        assert_eq!(package.version, "1.2.3");
        assert!(package.source.starts_with("registry+"));

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn resolve_import_from_checks_base_dir_first() {
        let workspace = temp_path("resolve");
        let subdir = workspace.join("lib");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("helper.fg"), "let x = 1").unwrap();

        // Without base_dir, won't find it (not in CWD or forge_modules)
        assert!(resolve_import_from("helper", None).is_none());

        // With base_dir pointing to the lib/ directory, finds it
        let result = resolve_import_from("helper", Some(&subdir));
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("helper.fg"));

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn resolve_import_from_falls_back_to_packages() {
        let workspace = temp_path("fallback");
        let base = workspace.join("src");
        std::fs::create_dir_all(&base).unwrap();

        // Even with a base_dir, should still fall back to CWD-relative checks
        let result = resolve_import_from("nonexistent_pkg", Some(&base));
        assert!(result.is_none());

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    fn create_registry_versions(root: &Path, name: &str, versions: &[&str]) {
        for ver in versions {
            let dir = root.join(name).join(ver);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("main.fg"), format!("// {}-{}", name, ver)).unwrap();
        }
    }

    #[test]
    fn resolve_best_version_caret() {
        let workspace = temp_path("semver-caret");
        let registry = workspace.join("registry");
        create_registry_versions(&registry, "foo", &["1.0.0", "1.0.3", "1.1.0", "2.0.0"]);

        let req = VersionReq::parse("^1.0").unwrap();
        let (ver, _) = resolve_best_version("foo", &req, &[registry]).unwrap();
        assert_eq!(ver, "1.1.0");

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn resolve_best_version_tilde() {
        let workspace = temp_path("semver-tilde");
        let registry = workspace.join("registry");
        create_registry_versions(&registry, "foo", &["1.0.0", "1.0.3", "1.1.0", "2.0.0"]);

        let req = VersionReq::parse("~1.0").unwrap();
        let (ver, _) = resolve_best_version("foo", &req, &[registry]).unwrap();
        assert_eq!(ver, "1.0.3");

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn resolve_best_version_star() {
        let workspace = temp_path("semver-star");
        let registry = workspace.join("registry");
        create_registry_versions(&registry, "foo", &["1.0.0", "1.1.0", "2.0.0"]);

        let (ver, _) = resolve_best_version("foo", &VersionReq::STAR, &[registry]).unwrap();
        assert_eq!(ver, "2.0.0");

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn resolve_best_version_range() {
        let workspace = temp_path("semver-range");
        let registry = workspace.join("registry");
        create_registry_versions(&registry, "foo", &["1.0.0", "1.0.3", "1.1.0", "2.0.0"]);

        let req = VersionReq::parse(">=1.0.0, <2.0.0").unwrap();
        let (ver, _) = resolve_best_version("foo", &req, &[registry]).unwrap();
        assert_eq!(ver, "1.1.0");

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn resolve_best_version_no_match() {
        let workspace = temp_path("semver-nomatch");
        let registry = workspace.join("registry");
        create_registry_versions(&registry, "foo", &["1.0.0", "1.1.0"]);

        let req = VersionReq::parse("^3.0").unwrap();
        let err = resolve_best_version("foo", &req, &[registry]).unwrap_err();
        assert!(err.contains("no version of 'foo' matches"));
        assert!(err.contains("1.0.0"));
        assert!(err.contains("1.1.0"));

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn resolve_best_version_empty_registry() {
        let workspace = temp_path("semver-empty");
        let registry = workspace.join("registry");
        std::fs::create_dir_all(&registry).unwrap();

        let req = VersionReq::parse("^1.0").unwrap();
        let err = resolve_best_version("foo", &req, &[registry]).unwrap_err();
        assert!(err.contains("no versions found"));

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn resolve_best_version_skips_non_semver_dirs() {
        let workspace = temp_path("semver-skip");
        let registry = workspace.join("registry");
        create_registry_versions(&registry, "foo", &["1.0.0"]);
        // Add a non-semver directory that should be ignored
        std::fs::create_dir_all(registry.join("foo").join("not-a-version")).unwrap();

        let req = VersionReq::parse("^1.0").unwrap();
        let (ver, _) = resolve_best_version("foo", &req, &[registry]).unwrap();
        assert_eq!(ver, "1.0.0");

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn resolve_best_version_rejects_traversal() {
        let err = resolve_best_version("../etc", &VersionReq::STAR, &[]).unwrap_err();
        assert!(err.contains("invalid package name"));

        let err = resolve_best_version("foo/bar", &VersionReq::STAR, &[]).unwrap_err();
        assert!(err.contains("invalid package name"));

        let err = resolve_best_version("", &VersionReq::STAR, &[]).unwrap_err();
        assert!(err.contains("invalid package name"));
    }

    // --- Transitive dependency tests ---

    fn write_forge_toml(dir: &Path, content: &str) {
        std::fs::write(dir.join("forge.toml"), content).unwrap();
    }

    #[test]
    fn transitive_deps_chain() {
        // A -> B -> C (all path deps)
        let workspace = temp_path("trans-chain");
        let pkg_b_src = workspace.join("pkg-b");
        let pkg_c_src = workspace.join("pkg-c");
        let packages_dir = workspace.join(PACKAGES_DIR);
        let lockfile_path = workspace.join("forge.lock");

        // Create source packages
        std::fs::create_dir_all(&pkg_b_src).unwrap();
        std::fs::write(pkg_b_src.join("main.fg"), "// pkg-b").unwrap();
        write_forge_toml(
            &pkg_b_src,
            &format!(
                "[project]\nname = \"pkg-b\"\n[dependencies]\npkg-c = {{ path = \"{}\" }}",
                pkg_c_src.display()
            ),
        );

        std::fs::create_dir_all(&pkg_c_src).unwrap();
        std::fs::write(pkg_c_src.join("main.fg"), "// pkg-c").unwrap();

        let manifest: Manifest = toml::from_str(&format!(
            "[project]\nname = \"app\"\n[dependencies]\npkg-b = {{ path = \"{}\" }}",
            pkg_b_src.display()
        ))
        .unwrap();

        let summary = install_manifest_dependencies(
            &manifest,
            &workspace,
            &packages_dir,
            &lockfile_path,
            &[],
        )
        .unwrap();

        // Both B and C should be installed
        assert!(packages_dir.join("pkg-b").join("main.fg").exists());
        assert!(packages_dir.join("pkg-c").join("main.fg").exists());
        assert!(summary.installed >= 2);

        // Lockfile should have both
        let lockfile = load_lockfile_from(&lockfile_path).unwrap();
        assert!(lockfile.find("pkg-b").is_some());
        assert!(lockfile.find("pkg-c").is_some());

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn transitive_deps_cycle_detected() {
        // A -> B -> A (cycle)
        let workspace = temp_path("trans-cycle");
        let pkg_a_src = workspace.join("pkg-a");
        let pkg_b_src = workspace.join("pkg-b");
        let packages_dir = workspace.join(PACKAGES_DIR);
        let lockfile_path = workspace.join("forge.lock");

        std::fs::create_dir_all(&pkg_a_src).unwrap();
        std::fs::write(pkg_a_src.join("main.fg"), "// pkg-a").unwrap();
        write_forge_toml(
            &pkg_a_src,
            &format!(
                "[project]\nname = \"pkg-a\"\n[dependencies]\npkg-b = {{ path = \"{}\" }}",
                pkg_b_src.display()
            ),
        );

        std::fs::create_dir_all(&pkg_b_src).unwrap();
        std::fs::write(pkg_b_src.join("main.fg"), "// pkg-b").unwrap();
        // B depends on A — this creates the cycle
        write_forge_toml(
            &pkg_b_src,
            &format!(
                "[project]\nname = \"pkg-b\"\n[dependencies]\npkg-a = {{ path = \"{}\" }}",
                pkg_a_src.display()
            ),
        );

        let manifest: Manifest = toml::from_str(&format!(
            "[project]\nname = \"app\"\n[dependencies]\npkg-a = {{ path = \"{}\" }}",
            pkg_a_src.display()
        ))
        .unwrap();

        let result = install_manifest_dependencies(
            &manifest,
            &workspace,
            &packages_dir,
            &lockfile_path,
            &[],
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("circular dependency"),
            "Expected circular dependency error, got: {}",
            err
        );

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn transitive_deps_diamond() {
        // A -> B, A -> C, B -> C (diamond, C installed once)
        let workspace = temp_path("trans-diamond");
        let pkg_b_src = workspace.join("pkg-b");
        let pkg_c_src = workspace.join("pkg-c");
        let packages_dir = workspace.join(PACKAGES_DIR);
        let lockfile_path = workspace.join("forge.lock");

        std::fs::create_dir_all(&pkg_c_src).unwrap();
        std::fs::write(pkg_c_src.join("main.fg"), "// pkg-c").unwrap();

        std::fs::create_dir_all(&pkg_b_src).unwrap();
        std::fs::write(pkg_b_src.join("main.fg"), "// pkg-b").unwrap();
        write_forge_toml(
            &pkg_b_src,
            &format!(
                "[project]\nname = \"pkg-b\"\n[dependencies]\npkg-c = {{ path = \"{}\" }}",
                pkg_c_src.display()
            ),
        );

        let manifest: Manifest = toml::from_str(&format!(
            "[project]\nname = \"app\"\n[dependencies]\npkg-b = {{ path = \"{}\" }}\npkg-c = {{ path = \"{}\" }}",
            pkg_b_src.display(),
            pkg_c_src.display()
        ))
        .unwrap();

        let summary = install_manifest_dependencies(
            &manifest,
            &workspace,
            &packages_dir,
            &lockfile_path,
            &[],
        )
        .unwrap();

        assert!(packages_dir.join("pkg-b").join("main.fg").exists());
        assert!(packages_dir.join("pkg-c").join("main.fg").exists());
        // C should appear in lockfile
        let lockfile = load_lockfile_from(&lockfile_path).unwrap();
        assert!(lockfile.find("pkg-c").is_some());
        // processed should be at least 2 (B + C direct), transitive C skipped as already installed
        assert!(summary.processed >= 2);

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn transitive_deps_leaf_no_manifest() {
        // A -> B where B has no forge.toml (leaf)
        let workspace = temp_path("trans-leaf");
        let pkg_b_src = workspace.join("pkg-b");
        let packages_dir = workspace.join(PACKAGES_DIR);
        let lockfile_path = workspace.join("forge.lock");

        std::fs::create_dir_all(&pkg_b_src).unwrap();
        std::fs::write(pkg_b_src.join("main.fg"), "// pkg-b").unwrap();
        // No forge.toml in pkg-b

        let manifest: Manifest = toml::from_str(&format!(
            "[project]\nname = \"app\"\n[dependencies]\npkg-b = {{ path = \"{}\" }}",
            pkg_b_src.display()
        ))
        .unwrap();

        let summary = install_manifest_dependencies(
            &manifest,
            &workspace,
            &packages_dir,
            &lockfile_path,
            &[],
        )
        .unwrap();

        assert_eq!(summary.installed, 1);
        assert!(packages_dir.join("pkg-b").join("main.fg").exists());

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn transitive_deps_cycle_back_to_root() {
        // app -> pkg-a, pkg-a depends on "app" (cycle back to root project)
        let workspace = temp_path("trans-root-cycle");
        let pkg_a_src = workspace.join("pkg-a");
        let app_src = workspace.join("app-src");
        let packages_dir = workspace.join(PACKAGES_DIR);
        let lockfile_path = workspace.join("forge.lock");

        // pkg-a depends on "app" (the root project name)
        std::fs::create_dir_all(&pkg_a_src).unwrap();
        std::fs::write(pkg_a_src.join("main.fg"), "// pkg-a").unwrap();
        write_forge_toml(
            &pkg_a_src,
            &format!(
                "[project]\nname = \"pkg-a\"\n[dependencies]\nmy-app = {{ path = \"{}\" }}",
                app_src.display()
            ),
        );

        // Create a fake "app" source so the path dep can be installed
        std::fs::create_dir_all(&app_src).unwrap();
        std::fs::write(app_src.join("main.fg"), "// app").unwrap();

        let manifest: Manifest = toml::from_str(&format!(
            "[project]\nname = \"my-app\"\n[dependencies]\npkg-a = {{ path = \"{}\" }}",
            pkg_a_src.display()
        ))
        .unwrap();

        let result = install_manifest_dependencies(
            &manifest,
            &workspace,
            &packages_dir,
            &lockfile_path,
            &[],
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("circular dependency"),
            "Expected circular dependency error, got: {}",
            err
        );
        assert!(
            err.contains("my-app"),
            "Error should mention root project name: {}",
            err
        );

        std::fs::remove_dir_all(&workspace).unwrap();
    }
}
