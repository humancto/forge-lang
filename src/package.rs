use std::path::{Path, PathBuf};
use std::process::Command;

const PACKAGES_DIR: &str = ".forge/packages";

pub fn install(source: &str) {
    let packages_dir = Path::new(PACKAGES_DIR);
    if let Err(e) = std::fs::create_dir_all(packages_dir) {
        eprintln!("Error: failed to create packages directory: {}", e);
        std::process::exit(1);
    }

    if source.starts_with("http://") || source.starts_with("https://") || source.starts_with("git@")
    {
        install_from_git(source, packages_dir);
    } else {
        install_from_local(source, packages_dir);
    }
}

fn install_from_git(url: &str, packages_dir: &Path) {
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
            Ok(s) if s.success() => println!("  Updated {}", name),
            _ => eprintln!("  Failed to update {}", name),
        }
    } else {
        println!("  Installing {} from {}...", name, url);
        let status = Command::new("git")
            .args(["clone", url, &target.display().to_string()])
            .status();
        match status {
            Ok(s) if s.success() => {
                println!("  Installed {}", name);
                if target.join("forge.toml").exists() {
                    println!("  Found forge.toml");
                }
            }
            _ => {
                eprintln!("  Failed to clone {}", url);
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
    println!("  Installed {} from {}", name, source);
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

/// Resolve an import path, checking .forge/packages/ as fallback
#[allow(dead_code)]
pub fn resolve_import(path: &str) -> Option<PathBuf> {
    let direct = Path::new(path);
    if direct.exists() {
        return Some(direct.to_path_buf());
    }

    let pkg_path = Path::new(PACKAGES_DIR).join(path);
    if pkg_path.exists() {
        return Some(pkg_path);
    }

    // Check package_name/main.fg
    let parts: Vec<&str> = path.splitn(2, '/').collect();
    if parts.len() == 1 {
        let main_fg = Path::new(PACKAGES_DIR).join(path).join("main.fg");
        if main_fg.exists() {
            return Some(main_fg);
        }
    }

    None
}
