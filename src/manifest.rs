use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct Manifest {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub test: TestConfig,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ProjectConfig {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub entry: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            version: default_version(),
            description: String::new(),
            entry: String::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
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
    let path = Path::new("forge.toml");
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}
