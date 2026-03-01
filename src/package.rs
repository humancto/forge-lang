use std::path::{Path, PathBuf};
use std::process::Command;

use crate::manifest::{self, DependencySpec, LockedPackage, Lockfile};

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

    if source.starts_with("http://") || source.starts_with("https://") || source.starts_with("git@")
    {
        install_from_git(source, None, packages_dir);
    } else {
        install_from_local(source, packages_dir);
    }
}

pub fn install_from_manifest() {
    let manifest = match manifest::load_manifest() {
        Some(m) => m,
        None => {
            eprintln!("No forge.toml found in current directory");
            std::process::exit(1);
        }
    };

    if manifest.dependencies.is_empty() {
        println!("  No dependencies to install.");
        return;
    }

    let packages_dir = Path::new(PACKAGES_DIR);
    if let Err(e) = std::fs::create_dir_all(packages_dir) {
        eprintln!("Error: failed to create forge_modules/: {}", e);
        std::process::exit(1);
    }

    let mut lockfile = Lockfile::load().unwrap_or_default();
    let mut installed = 0;

    for (name, spec) in &manifest.dependencies {
        match spec {
            DependencySpec::Version(ver) => {
                println!("  {} @ {} (registry not yet available)", name, ver);
                lockfile.packages.retain(|p| p.name != *name);
                lockfile.packages.push(LockedPackage {
                    name: name.clone(),
                    version: ver.clone(),
                    source: "registry".to_string(),
                    checksum: String::new(),
                });
            }
            DependencySpec::Detailed(dep) => {
                if !dep.git.is_empty() {
                    let branch = if dep.branch.is_empty() {
                        None
                    } else {
                        Some(dep.branch.as_str())
                    };
                    install_from_git(&dep.git, branch, packages_dir);
                    lockfile.packages.retain(|p| p.name != *name);
                    lockfile.packages.push(LockedPackage {
                        name: name.clone(),
                        version: dep.version.clone(),
                        source: format!("git+{}", dep.git),
                        checksum: get_git_rev(packages_dir, name),
                    });
                    installed += 1;
                } else if !dep.path.is_empty() {
                    install_from_local(&dep.path, packages_dir);
                    lockfile.packages.retain(|p| p.name != *name);
                    lockfile.packages.push(LockedPackage {
                        name: name.clone(),
                        version: dep.version.clone(),
                        source: format!("path+{}", dep.path),
                        checksum: String::new(),
                    });
                    installed += 1;
                }
            }
        }
    }

    if let Err(e) = lockfile.save() {
        eprintln!("Warning: failed to write forge.lock: {}", e);
    } else if installed > 0 {
        println!(
            "  Updated forge.lock ({} packages)",
            lockfile.packages.len()
        );
    }

    println!(
        "  {} dependencies processed for '{}'",
        manifest.dependencies.len(),
        manifest.project.name
    );
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

fn install_from_git(url: &str, branch: Option<&str>, packages_dir: &Path) {
    let name = url
        .rsplit('/')
        .next()
        .unwrap_or("package")
        .trim_end_matches(".git");
    let target = packages_dir.join(name);

    if target.exists() {
        println!("  Updating {}...", name);
        let status = Command::new("git")
            .args(["pull"])
            .current_dir(&target)
            .status();
        match status {
            Ok(s) if s.success() => println!("  \x1B[32m✓\x1B[0m Updated {}", name),
            _ => eprintln!("  \x1B[31m✗\x1B[0m Failed to update {}", name),
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
                eprintln!("  \x1B[31m✗\x1B[0m Failed to clone {}", url);
                std::process::exit(1);
            }
        }
    }
}

fn install_from_local(source: &str, packages_dir: &Path) {
    let src = Path::new(source);
    if !src.exists() {
        eprintln!("  Error: '{}' not found", source);
        std::process::exit(1);
    }

    let name = src.file_name().unwrap_or_default().to_string_lossy();
    let target = packages_dir.join(name.as_ref());

    if target.exists() {
        std::fs::remove_dir_all(&target).ok();
    }

    if let Err(e) = copy_dir_all(src, &target) {
        eprintln!("Error: failed to copy package: {}", e);
        std::process::exit(1);
    }
    println!("  \x1B[32m✓\x1B[0m Installed {} from {}", name, source);
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

/// Resolve an import path, checking forge_modules/ and .forge/packages/
pub fn resolve_import(path: &str) -> Option<PathBuf> {
    // Direct file path
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
