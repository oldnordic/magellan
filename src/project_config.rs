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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSection {
    /// Glob patterns to include (additive). Empty means "include all".
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns to exclude. Applied after include.
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Default for IndexSection {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
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
}
