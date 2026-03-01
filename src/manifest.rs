use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub dependencies: BTreeMap<String, DependencySpec>,
    #[serde(default)]
    pub test: TestConfig,
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectConfig {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub entry: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub repository: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            version: default_version(),
            description: String::new(),
            entry: String::new(),
            authors: Vec::new(),
            license: String::new(),
            repository: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum DependencySpec {
    Version(String),
    Detailed(DetailedDep),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DetailedDep {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub git: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub path: String,
}

impl DependencySpec {
    pub fn is_git(&self) -> bool {
        match self {
            DependencySpec::Detailed(d) => !d.git.is_empty(),
            _ => false,
        }
    }

    pub fn is_path(&self) -> bool {
        match self {
            DependencySpec::Detailed(d) => !d.path.is_empty(),
            _ => false,
        }
    }

    pub fn git_url(&self) -> Option<&str> {
        match self {
            DependencySpec::Detailed(d) if !d.git.is_empty() => Some(&d.git),
            _ => None,
        }
    }

    pub fn local_path(&self) -> Option<&str> {
        match self {
            DependencySpec::Detailed(d) if !d.path.is_empty() => Some(&d.path),
            _ => None,
        }
    }

    pub fn branch(&self) -> Option<&str> {
        match self {
            DependencySpec::Detailed(d) if !d.branch.is_empty() => Some(&d.branch),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TestConfig {
    #[serde(default = "default_test_dir")]
    pub directory: String,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            directory: default_test_dir(),
        }
    }
}

fn default_name() -> String {
    "forge-project".to_string()
}
fn default_version() -> String {
    "0.1.0".to_string()
}
fn default_test_dir() -> String {
    "tests".to_string()
}

pub fn load_manifest() -> Option<Manifest> {
    load_manifest_from(Path::new("forge.toml"))
}

pub fn load_manifest_from(path: &Path) -> Option<Manifest> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Lockfile {
    pub packages: Vec<LockedPackage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub checksum: String,
}

impl Lockfile {
    pub fn load() -> Option<Self> {
        let path = Path::new("forge.lock");
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write("forge.lock", content)
    }

    pub fn find(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.iter().find(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manifest() {
        let toml_str = r#"
[project]
name = "my-app"
version = "0.1.0"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.project.name, "my-app");
        assert_eq!(manifest.project.version, "0.1.0");
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn parse_with_string_dependencies() {
        let toml_str = r#"
[project]
name = "my-app"
version = "0.1.0"

[dependencies]
utils = "1.0.0"
helpers = "0.5.0"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.dependencies.len(), 2);
        match &manifest.dependencies["utils"] {
            DependencySpec::Version(v) => assert_eq!(v, "1.0.0"),
            _ => panic!("expected version string"),
        }
    }

    #[test]
    fn parse_with_git_dependency() {
        let toml_str = r#"
[project]
name = "my-app"
version = "0.1.0"

[dependencies]
mylib = { git = "https://github.com/user/mylib.git", branch = "main" }
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        let dep = &manifest.dependencies["mylib"];
        assert!(dep.is_git());
        assert_eq!(dep.git_url(), Some("https://github.com/user/mylib.git"));
        assert_eq!(dep.branch(), Some("main"));
    }

    #[test]
    fn parse_with_path_dependency() {
        let toml_str = r#"
[project]
name = "my-app"
version = "0.1.0"

[dependencies]
local-lib = { path = "../local-lib" }
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        let dep = &manifest.dependencies["local-lib"];
        assert!(dep.is_path());
        assert_eq!(dep.local_path(), Some("../local-lib"));
    }

    #[test]
    fn parse_with_scripts() {
        let toml_str = r#"
[project]
name = "my-app"
version = "0.1.0"

[scripts]
dev = "forge run main.fg"
build = "forge build main.fg"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.scripts.len(), 2);
        assert_eq!(manifest.scripts["dev"], "forge run main.fg");
    }

    #[test]
    fn parse_full_manifest() {
        let toml_str = r#"
[project]
name = "web-api"
version = "1.2.3"
description = "A web API built with Forge"
entry = "src/main.fg"
authors = ["Alice <alice@example.com>"]
license = "MIT"
repository = "https://github.com/alice/web-api"

[dependencies]
router = "2.0.0"
auth = { git = "https://github.com/alice/forge-auth.git" }
utils = { path = "../shared-utils" }

[test]
directory = "spec"

[scripts]
dev = "forge run src/main.fg"
test = "forge test"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.project.name, "web-api");
        assert_eq!(manifest.project.version, "1.2.3");
        assert_eq!(manifest.project.authors.len(), 1);
        assert_eq!(manifest.project.license, "MIT");
        assert_eq!(manifest.dependencies.len(), 3);
        assert_eq!(manifest.test.directory, "spec");
        assert_eq!(manifest.scripts.len(), 2);
    }

    #[test]
    fn parse_empty_manifest() {
        let toml_str = "";
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.project.name, "forge-project");
        assert_eq!(manifest.project.version, "0.1.0");
    }

    #[test]
    fn lockfile_round_trip() {
        let lockfile = Lockfile {
            packages: vec![
                LockedPackage {
                    name: "utils".to_string(),
                    version: "1.0.0".to_string(),
                    source: "registry".to_string(),
                    checksum: "abc123".to_string(),
                },
                LockedPackage {
                    name: "auth".to_string(),
                    version: "0.5.0".to_string(),
                    source: "git+https://github.com/x/auth.git".to_string(),
                    checksum: "def456".to_string(),
                },
            ],
        };
        let serialized = toml::to_string_pretty(&lockfile).unwrap();
        let restored: Lockfile = toml::from_str(&serialized).unwrap();
        assert_eq!(restored.packages.len(), 2);
        assert_eq!(restored.packages[0].name, "utils");
        assert_eq!(restored.find("auth").unwrap().version, "0.5.0");
    }

    #[test]
    fn lockfile_find() {
        let lockfile = Lockfile {
            packages: vec![LockedPackage {
                name: "foo".to_string(),
                version: "1.0.0".to_string(),
                source: String::new(),
                checksum: String::new(),
            }],
        };
        assert!(lockfile.find("foo").is_some());
        assert!(lockfile.find("bar").is_none());
    }
}
