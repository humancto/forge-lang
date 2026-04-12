use std::path::{Path, PathBuf};

use crate::manifest::{self, Manifest};

/// Default files/directories to exclude from published packages.
const DEFAULT_EXCLUDES: &[&str] = &[
    "forge_modules",
    ".git",
    "target",
    ".forge",
    "node_modules",
    "tests",
    ".env",
];

/// Default file patterns to exclude.
const DEFAULT_EXCLUDE_PATTERNS: &[&str] = &["*.lock", "*.tar.gz", "*.secret*"];

/// Entry point from CLI — uses CWD as project directory.
pub fn publish(dry_run: bool, registry_override: Option<&str>) {
    publish_from(Path::new("."), dry_run, registry_override);
}

/// Publish a project from a given directory to the registry.
fn publish_from(project_dir: &Path, dry_run: bool, registry_override: Option<&str>) {
    let manifest_path = project_dir.join("forge.toml");
    let manifest = match manifest::load_manifest_from(&manifest_path) {
        Some(m) => m,
        None => {
            eprintln!("Error: no forge.toml found in {}", project_dir.display());
            std::process::exit(1);
        }
    };

    if let Err(e) = validate_manifest(&manifest) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    let name = &manifest.project.name;
    let version = &manifest.project.version;

    let registry_root = match registry_override {
        Some(path) => PathBuf::from(path),
        None => default_global_registry(),
    };

    let target_dir = registry_root.join(name).join(version);

    // Collect files to package
    let files = collect_files(project_dir);

    if files.is_empty() {
        eprintln!("Error: no .fg files found to publish");
        std::process::exit(1);
    }

    if dry_run {
        println!("  Would publish {} v{}", name, version);
        println!("  Registry: {}", registry_root.display());
        println!("  Files ({}):", files.len());
        for f in &files {
            println!("    {}", f.display());
        }
        return;
    }

    // Warn if overwriting
    if target_dir.exists() {
        eprintln!(
            "  Warning: replacing existing {}@{} in local registry",
            name, version
        );
        if let Err(e) = std::fs::remove_dir_all(&target_dir) {
            eprintln!("Error: failed to remove existing version: {}", e);
            std::process::exit(1);
        }
    }

    // Create registry directory
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        eprintln!("Error: failed to create registry directory: {}", e);
        std::process::exit(1);
    }

    // Copy files and compute checksum
    let mut hasher = Sha256::new();
    let mut total_size: u64 = 0;

    for file in &files {
        let src = project_dir.join(file);
        let dest = target_dir.join(file);
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "Error: failed to create directory {}: {}",
                    parent.display(),
                    e
                );
                std::process::exit(1);
            }
        }

        let content = match std::fs::read(&src) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error: failed to read {}: {}", src.display(), e);
                std::process::exit(1);
            }
        };

        total_size += content.len() as u64;
        hasher.update(&content);

        if let Err(e) = std::fs::write(&dest, &content) {
            eprintln!("Error: failed to write {}: {}", dest.display(), e);
            std::process::exit(1);
        }
    }

    let checksum = hasher.finalize_hex();

    // Write checksum file
    let checksum_content = format!("sha256:{}\n", checksum);
    if let Err(e) = std::fs::write(target_dir.join(".forge-checksum"), &checksum_content) {
        eprintln!("Warning: failed to write checksum file: {}", e);
    }

    // Verify the package is findable
    let found = crate::package::find_in_registry(name, version, &[registry_root.clone()]);
    if found.is_none() {
        eprintln!(
            "Warning: published package not found in registry at {}",
            target_dir.display()
        );
    }

    println!("  \x1B[32m✓\x1B[0m Published {} v{}", name, version);
    println!("    Registry: {}", registry_root.display());
    println!("    Files: {}", files.len());
    println!(
        "    Size: {}",
        if total_size > 1024 {
            format!("{:.1} KB", total_size as f64 / 1024.0)
        } else {
            format!("{} bytes", total_size)
        }
    );
    println!("    Checksum: {}", &checksum[..16]);
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    let name = &manifest.project.name;
    let version = &manifest.project.version;

    if name == "forge-project" || name.is_empty() {
        return Err(
            "project.name must be set in forge.toml (not the default 'forge-project')".into(),
        );
    }

    if version.is_empty() {
        return Err("project.version must be set in forge.toml".into());
    }

    // Validate name: alphanumeric, hyphens, underscores only
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "project.name '{}' contains invalid characters (use alphanumeric, hyphens, underscores)",
            name
        ));
    }

    // Validate version: no path separators
    if version.contains('/') || version.contains('\\') || version.contains("..") {
        return Err(format!(
            "project.version '{}' contains invalid characters",
            version
        ));
    }

    // Warn about missing recommended fields
    if manifest.project.description.is_empty() {
        eprintln!("  Warning: project.description is empty in forge.toml");
    }
    if manifest.project.license.is_empty() {
        eprintln!("  Warning: project.license is empty in forge.toml");
    }

    Ok(())
}

fn collect_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(root, root, &mut files);
    files.sort();
    files
}

fn collect_files_recursive(base: &Path, dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        // Skip symlinks (security: avoid following links outside project)
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();
        let relative = path.strip_prefix(base).unwrap_or(&path);
        let name = relative
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_default();

        // Check directory excludes
        if file_type.is_dir() {
            if DEFAULT_EXCLUDES.contains(&name.as_str()) {
                continue;
            }
            collect_files_recursive(base, &path, files);
            continue;
        }

        // Check file pattern excludes
        let filename = entry.file_name().to_string_lossy().to_string();
        if is_pattern_excluded(&filename) {
            continue;
        }

        // Include .fg files, forge.toml, and README
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "fg" || filename == "forge.toml" || filename.to_lowercase().starts_with("readme")
        {
            files.push(relative.to_path_buf());
        }
    }
}

fn is_pattern_excluded(filename: &str) -> bool {
    for pattern in DEFAULT_EXCLUDE_PATTERNS {
        if pattern.starts_with('*') && pattern.ends_with('*') {
            let inner = &pattern[1..pattern.len() - 1];
            if filename.contains(inner) {
                return true;
            }
        } else if pattern.starts_with('*') {
            let suffix = &pattern[1..];
            if filename.ends_with(suffix) {
                return true;
            }
        } else if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            if filename.starts_with(prefix) {
                return true;
            }
        } else if filename == *pattern {
            return true;
        }
    }
    false
}

/// Get the global registry directory (~/.forge/registry/).
pub fn default_global_registry() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".forge").join("registry")
}

/// SHA-256 hasher wrapping the `sha2` crate.
struct Sha256 {
    hasher: sha2::Sha256,
}

impl Sha256 {
    fn new() -> Self {
        use sha2::Digest;
        Self {
            hasher: sha2::Sha256::new(),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        use sha2::Digest;
        self.hasher.update(bytes);
    }

    fn finalize_hex(self) -> String {
        use sha2::Digest;
        let result = self.hasher.finalize();
        let mut hex = String::with_capacity(64);
        for byte in result.iter() {
            use std::fmt::Write;
            write!(hex, "{:02x}", byte).unwrap();
        }
        hex
    }
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
        std::env::temp_dir().join(format!("forge-publish-{}-{}", prefix, unique))
    }

    fn create_project(dir: &Path, name: &str, version: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("forge.toml"),
            format!(
                "[project]\nname = \"{}\"\nversion = \"{}\"\ndescription = \"test\"\nlicense = \"MIT\"\n",
                name, version
            ),
        )
        .unwrap();
        std::fs::write(dir.join("main.fg"), "println(\"hello\")").unwrap();
    }

    #[test]
    fn validate_rejects_default_name() {
        let manifest: Manifest = toml::from_str("[project]\n").unwrap();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn validate_rejects_empty_name() {
        let manifest: Manifest =
            toml::from_str("[project]\nname = \"\"\nversion = \"1.0.0\"").unwrap();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn validate_rejects_invalid_name_chars() {
        let manifest: Manifest =
            toml::from_str("[project]\nname = \"../../bad\"\nversion = \"1.0.0\"").unwrap();
        let err = validate_manifest(&manifest).unwrap_err();
        assert!(err.contains("invalid characters"));
    }

    #[test]
    fn validate_rejects_path_traversal_version() {
        let manifest: Manifest =
            toml::from_str("[project]\nname = \"mylib\"\nversion = \"../../../etc\"").unwrap();
        let err = validate_manifest(&manifest).unwrap_err();
        assert!(err.contains("invalid characters"));
    }

    #[test]
    fn validate_accepts_good_manifest() {
        let manifest: Manifest = toml::from_str(
            "[project]\nname = \"my-lib\"\nversion = \"1.0.0\"\ndescription = \"A lib\"\nlicense = \"MIT\"",
        )
        .unwrap();
        assert!(validate_manifest(&manifest).is_ok());
    }

    #[test]
    fn collect_files_finds_fg_and_manifest() {
        let dir = temp_path("collect");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("forge.toml"), "[project]").unwrap();
        std::fs::write(dir.join("main.fg"), "let x = 1").unwrap();
        std::fs::write(dir.join("src").join("helper.fg"), "let y = 2").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignore me").unwrap();

        let files = collect_files(&dir);
        let names: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
        assert!(names.contains(&"forge.toml".to_string()));
        assert!(names.contains(&"main.fg".to_string()));
        assert!(names.iter().any(|n| n.contains("helper.fg")));
        assert!(!names.iter().any(|n| n.contains("notes.txt")));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn collect_files_excludes_forge_modules() {
        let dir = temp_path("exclude");
        std::fs::create_dir_all(dir.join("forge_modules").join("dep")).unwrap();
        std::fs::write(
            dir.join("forge_modules").join("dep").join("main.fg"),
            "let x = 1",
        )
        .unwrap();
        std::fs::write(dir.join("main.fg"), "let y = 2").unwrap();

        let files = collect_files(&dir);
        assert!(!files
            .iter()
            .any(|f| f.display().to_string().contains("forge_modules")));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn collect_files_excludes_lock_files() {
        let dir = temp_path("lock");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("forge.lock"), "lockfile").unwrap();
        std::fs::write(dir.join("main.fg"), "let x = 1").unwrap();

        let files = collect_files(&dir);
        assert!(!files
            .iter()
            .any(|f| f.display().to_string().contains(".lock")));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn publish_creates_registry_entry() {
        let project = temp_path("pub-project");
        let registry = temp_path("pub-registry");
        create_project(&project, "testlib", "1.0.0");

        publish_from(&project, false, Some(registry.to_str().unwrap()));

        let entry = registry.join("testlib").join("1.0.0");
        assert!(entry.exists(), "registry entry should exist");
        assert!(
            entry.join("main.fg").exists(),
            "main.fg should be in registry"
        );
        assert!(
            entry.join("forge.toml").exists(),
            "forge.toml should be in registry"
        );
        assert!(
            entry.join(".forge-checksum").exists(),
            "checksum should exist"
        );

        let checksum = std::fs::read_to_string(entry.join(".forge-checksum")).unwrap();
        assert!(checksum.starts_with("sha256:"));

        std::fs::remove_dir_all(&project).unwrap();
        std::fs::remove_dir_all(&registry).unwrap();
    }

    #[test]
    fn publish_dry_run_no_side_effects() {
        let project = temp_path("dry-project");
        let registry = temp_path("dry-registry");
        create_project(&project, "drylib", "0.1.0");

        publish_from(&project, true, Some(registry.to_str().unwrap()));

        assert!(
            !registry.exists(),
            "registry should not be created in dry run"
        );

        std::fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn publish_overwrites_cleans_old_files() {
        let project = temp_path("overwrite-project");
        let registry = temp_path("overwrite-registry");
        create_project(&project, "overlib", "1.0.0");

        // First publish with extra file
        std::fs::write(project.join("old_helper.fg"), "let old = true").unwrap();
        publish_from(&project, false, Some(registry.to_str().unwrap()));

        let entry = registry.join("overlib").join("1.0.0");
        assert!(entry.join("old_helper.fg").exists());

        // Remove old file and re-publish
        std::fs::remove_file(project.join("old_helper.fg")).unwrap();
        publish_from(&project, false, Some(registry.to_str().unwrap()));

        assert!(
            !entry.join("old_helper.fg").exists(),
            "old files should be cleaned up on re-publish"
        );

        std::fs::remove_dir_all(&project).unwrap();
        std::fs::remove_dir_all(&registry).unwrap();
    }

    #[test]
    fn publish_install_round_trip() {
        let project = temp_path("roundtrip-project");
        let registry = temp_path("roundtrip-registry");
        create_project(&project, "roundlib", "2.0.0");

        publish_from(&project, false, Some(registry.to_str().unwrap()));

        // Verify the package is findable via the same mechanism forge install uses
        let found = crate::package::find_in_registry("roundlib", "2.0.0", &[registry.clone()]);
        assert!(
            found.is_some(),
            "published package should be findable by install"
        );

        std::fs::remove_dir_all(&project).unwrap();
        std::fs::remove_dir_all(&registry).unwrap();
    }

    #[test]
    fn checksum_is_content_based() {
        let h1 = {
            let mut h = Sha256::new();
            h.update(b"hello world");
            h.finalize_hex()
        };
        let h2 = {
            let mut h = Sha256::new();
            h.update(b"hello world");
            h.finalize_hex()
        };
        let h3 = {
            let mut h = Sha256::new();
            h.update(b"different content");
            h.finalize_hex()
        };
        assert_eq!(h1, h2, "same content should produce same checksum");
        assert_ne!(
            h1, h3,
            "different content should produce different checksum"
        );
        assert_eq!(h1.len(), 64, "SHA-256 hex should be 64 chars");
    }

    #[test]
    fn is_pattern_excluded_works() {
        assert!(is_pattern_excluded("forge.lock"));
        assert!(is_pattern_excluded("package.lock"));
        assert!(is_pattern_excluded("archive.tar.gz"));
        assert!(is_pattern_excluded("db.secret.key"));
        assert!(!is_pattern_excluded("main.fg"));
        assert!(!is_pattern_excluded("forge.toml"));
    }
}
