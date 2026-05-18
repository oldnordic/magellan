//! Per-project configuration loaded from `.magellan.toml`.
//!
//! Sits at the project root alongside `Cargo.toml`. Controls include/exclude
//! paths, watcher settings, and project metadata. When absent, behaviour is
//! identical to the pre-v4 CLI (backward compatible).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::graph::filter::FileFilter;

const CONFIG_FILENAME: &str = ".magellan.toml";

fn default_debounce() -> u64 {
    500
}

fn default_true() -> bool {
    true
}

// -- Section structs ----------------------------------------------------------

/// `[project]` — project identity.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSection {
    #[serde(default)]
    pub name: Option<String>,
}

/// `[index]` — path filtering during scan/watch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexSection {
    /// Glob patterns to include (additive). Empty means "include all".
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns to exclude. Applied after include.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// `[watch]` — watcher behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchSection {
    #[serde(default = "default_debounce")]
    pub debounce_ms: u64,
    #[serde(default = "default_true")]
    pub gitignore_aware: bool,
    #[serde(default = "default_true")]
    pub scan_initial: bool,
}

impl Default for WatchSection {
    fn default() -> Self {
        Self {
            debounce_ms: default_debounce(),
            gitignore_aware: true,
            scan_initial: true,
        }
    }
}

// -- Root config --------------------------------------------------------------

/// Per-project configuration read from `.magellan.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    #[serde(default)]
    pub project: ProjectSection,
    #[serde(default)]
    pub index: IndexSection,
    #[serde(default)]
    pub watch: WatchSection,
}

impl ProjectConfig {
    /// Load `.magellan.toml` from `project_root`.
    ///
    /// Returns `ProjectConfig::default()` when the file is absent (not an error).
    pub fn load(project_root: &Path) -> Result<Self> {
        let path = project_root.join(CONFIG_FILENAME);
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))
    }

    /// Write a default `.magellan.toml` into `project_root`.
    ///
    /// Returns an error if the file already exists (no silent overwrite).
    pub fn init(project_root: &Path, name: &str) -> Result<()> {
        let path = project_root.join(CONFIG_FILENAME);
        if path.exists() {
            anyhow::bail!(
                "{} already exists — remove it first if you want to regenerate",
                path.display()
            );
        }

        let config = Self {
            project: ProjectSection {
                name: Some(name.to_string()),
            },
            index: IndexSection {
                include: vec!["src/".into()],
                exclude: Vec::new(),
            },
            ..Self::default()
        };

        let content =
            toml::to_string_pretty(&config).context("Failed to serialize project config")?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write {}", path.display()))?;

        Ok(())
    }

    /// Convert include/exclude patterns into a `FileFilter`.
    ///
    /// Directory patterns ending with `/` are expanded to `dir/**` so that
    /// `src/` matches `src/main.rs` (not just the literal string `src/`).
    pub fn to_file_filter(&self, root: &Path) -> Result<FileFilter> {
        let include = normalize_dir_patterns(&self.index.include);
        let exclude = normalize_dir_patterns(&self.index.exclude);
        FileFilter::new(root, &include, &exclude)
    }
}

/// Parsed Cargo.toml manifest data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CargoManifest {
    pub package_name: Option<String>,
    /// Feature flags from [features] section.
    /// `default` feature is excluded — it's a meta-feature listing defaults.
    #[serde(default)]
    pub features: std::collections::HashMap<String, Vec<String>>,
    /// Crate names from [dependencies] section (dev-dependencies excluded).
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Target paths extracted from [[bin]], [[test]], [[bench]] arrays.
    #[serde(default)]
    pub targets: Vec<String>,
}

impl CargoManifest {
    /// Parse `Cargo.toml` from `project_root`.
    ///
    /// Returns `CargoManifest::default()` when `Cargo.toml` is absent.
    pub fn parse(project_root: &Path) -> Result<Self> {
        let path = project_root.join("Cargo.toml");
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let doc: toml::Table = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;

        let mut manifest = Self::default();

        // [package].name
        if let Some(toml::Value::Table(package)) = doc.get("package") {
            if let Some(toml::Value::String(name)) = package.get("name") {
                manifest.package_name = Some(name.clone());
            }
        }

        // [features] — skip "default" meta-feature
        if let Some(toml::Value::Table(features)) = doc.get("features") {
            for (k, v) in features.iter().filter(|(k, _)| *k != "default") {
                if let toml::Value::Array(arr) = v {
                    manifest.features.insert(
                        k.clone(),
                        arr.iter()
                            .filter_map(|item| item.as_str().map(String::from))
                            .collect(),
                    );
                }
            }
        }

        // [dependencies]
        if let Some(toml::Value::Table(deps)) = doc.get("dependencies") {
            manifest.dependencies = deps.keys().cloned().collect();
        }

        // [[bin]], [[test]], [[bench]] — extract path
        for key in ["bin", "test", "bench"] {
            if let Some(toml::Value::Array(arr)) = doc.get(key) {
                for item in arr {
                    if let toml::Value::Table(t) = item {
                        if let Some(toml::Value::String(path)) = t.get("path") {
                            manifest.targets.push(path.clone());
                        }
                    }
                }
            }
        }

        Ok(manifest)
    }

    /// Store manifest data into `magellan_meta` table on `conn`.
    pub fn store_in_db(&self, conn: &rusqlite::Connection) -> Result<()> {
        let metadata_json = serde_json::to_string(self)
            .context("Failed to serialize CargoManifest")?;
        conn.execute(
            "UPDATE magellan_meta SET project_name = ?1, project_metadata = ?2 WHERE id = 1",
            rusqlite::params![self.package_name, metadata_json],
        )
        .context("Failed to update magellan_meta with project metadata")?;
        Ok(())
    }
}

impl ProjectConfig {
    /// Parse `Cargo.toml` manifest from `project_root`.
    pub fn parse_cargo_manifest(project_root: &Path) -> Result<CargoManifest> {
        CargoManifest::parse(project_root)
    }
}

/// Expand directory patterns: `src/` → `src/**` so globs match files inside.
fn normalize_dir_patterns(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .map(|p| {
            if p.ends_with('/') {
                format!("{}**", p)
            } else {
                p.clone()
            }
        })
        .collect()
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_minimal_toml() {
        let cfg: ProjectConfig = toml::from_str("[project]\nname = \"test\"").unwrap();
        assert_eq!(cfg.project.name.as_deref(), Some("test"));
        assert!(cfg.index.include.is_empty()); // empty = include all (backward compat)
        assert!(cfg.index.exclude.is_empty());
        assert_eq!(cfg.watch.debounce_ms, 500);
    }

    #[test]
    fn parse_full_toml() {
        let input = r#"
[project]
name = "magellan"

[index]
include = ["src/", "tests/", "benches/"]
exclude = ["src/generated/**"]

[watch]
debounce_ms = 1000
gitignore_aware = false
scan_initial = false
"#;
        let cfg: ProjectConfig = toml::from_str(input).unwrap();
        assert_eq!(cfg.project.name.as_deref(), Some("magellan"));
        assert_eq!(cfg.index.include, vec!["src/", "tests/", "benches/"]);
        assert_eq!(cfg.index.exclude, vec!["src/generated/**"]);
        assert_eq!(cfg.watch.debounce_ms, 1000);
        assert!(!cfg.watch.gitignore_aware);
        assert!(!cfg.watch.scan_initial);
    }

    #[test]
    fn missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ProjectConfig::load(dir.path()).unwrap();
        assert!(cfg.project.name.is_none());
        assert!(cfg.index.include.is_empty()); // empty = include all (backward compat)
    }

    #[test]
    fn invalid_toml_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILENAME);
        fs::write(&path, "not valid toml {{{").unwrap();
        let result = ProjectConfig::load(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to parse"), "got: {err}");
    }

    #[test]
    fn init_writes_config() {
        let dir = tempfile::tempdir().unwrap();
        ProjectConfig::init(dir.path(), "my-project").unwrap();

        let path = dir.path().join(CONFIG_FILENAME);
        assert!(path.exists());

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("name = \"my-project\""));
        // init writes a starting config with src/ as include
        assert!(content.contains("include = [\"src/\"]"));

        // Round-trip: load what we just wrote
        let cfg = ProjectConfig::load(dir.path()).unwrap();
        assert_eq!(cfg.project.name.as_deref(), Some("my-project"));
        assert!(cfg.index.include.contains(&"src/".to_string()));
    }

    #[test]
    fn init_refuses_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        ProjectConfig::init(dir.path(), "first").unwrap();
        let result = ProjectConfig::init(dir.path(), "second");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn to_file_filter_creates_matcher() {
        let cfg = ProjectConfig {
            index: IndexSection {
                include: vec!["src/".into(), "tests/".into()],
                exclude: vec!["src/generated/**".into()],
            },
            ..Default::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let filter = cfg.to_file_filter(dir.path()).unwrap();
        // FileFilter::new succeeded — matchers compiled
        assert!(filter
            .should_skip(&dir.path().join("target/foo.rs"))
            .is_some());
    }

    #[test]
    fn parse_cargo_toml_features_and_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_toml = r#"
[package]
name = "test-crate"
version = "0.1.0"
edition = "2021"

[features]
default = ["sqlite-backend", "geometric-backend"]
sqlite-backend = []
geometric-backend = ["dep:geographdb-core"]
llvm-cfg = ["dep:llvm-sys"]

[dependencies]
anyhow = "1.0"
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }

[dev-dependencies]
tempfile = "3.10"

[[test]]
name = "integration"
path = "tests/integration.rs"
"#;
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

        let manifest = ProjectConfig::parse_cargo_manifest(dir.path()).unwrap();

        assert_eq!(manifest.package_name, Some("test-crate".to_string()));
        assert!(manifest.features.contains_key("sqlite-backend"));
        assert!(manifest.features.contains_key("geometric-backend"));
        assert!(manifest.features.contains_key("llvm-cfg"));
        assert!(!manifest.features.contains_key("default")); // default is a meta-feature, skip

        assert!(manifest.dependencies.contains(&"anyhow".to_string()));
        assert!(manifest.dependencies.contains(&"thiserror".to_string()));
        assert!(manifest.dependencies.contains(&"serde".to_string()));
        assert!(!manifest.dependencies.contains(&"tempfile".to_string())); // dev-dependencies excluded

        assert!(manifest.targets.contains(&"tests/integration.rs".to_string()));
    }
}
