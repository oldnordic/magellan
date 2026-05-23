//! Project registry: read/write ~/.config/magellan/registry.toml

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::types::ProjectEntry;

const DEFAULT_REGISTRY_PATH: &str = "/home/feanor/.config/magellan/registry.toml";

/// Top-level registry file content
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryFile {
    pub version: String,
    #[serde(default)]
    pub project: Vec<ProjectEntry>,
}

/// Manages the persistent project registry
#[derive(Debug, Clone)]
pub struct Registry {
    pub projects: Vec<ProjectEntry>,
    path: PathBuf,
}

impl Registry {
    /// Load registry from the default path
    pub fn load() -> Result<Self> {
        Self::load_from(PathBuf::from(DEFAULT_REGISTRY_PATH))
    }

    /// Load registry from a specific path
    pub fn load_from(path: PathBuf) -> Result<Self> {
        let projects = if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let file: RegistryFile =
                toml::from_str(&content).with_context(|| "Failed to parse registry TOML")?;
            file.project
        } else {
            Vec::new()
        };
        Ok(Self { projects, path })
    }

    /// Save registry to disk
    pub fn save(&self) -> Result<()> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        }
        let file = RegistryFile {
            version: "1".to_string(),
            project: self.projects.clone(),
        };
        let content = toml::to_string_pretty(&file).context("Failed to serialize registry")?;
        fs::write(&self.path, content)
            .with_context(|| format!("Failed to write {}", self.path.display()))?;
        Ok(())
    }

    /// List all registered projects
    pub fn list(&self) -> &[ProjectEntry] {
        &self.projects
    }

    /// Find a project by name (exact match)
    pub fn find(&self, name: &str) -> Option<&ProjectEntry> {
        self.projects.iter().find(|p| p.name == name)
    }

    /// Find a project by root path
    pub fn find_by_root(&self, root: &Path) -> Option<&ProjectEntry> {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        self.projects
            .iter()
            .find(|p| p.root.canonicalize().unwrap_or_else(|_| p.root.clone()) == root)
    }

    /// Check if a root path is already registered
    pub fn is_registered(&self, root: &Path) -> bool {
        self.find_by_root(root).is_some()
    }

    /// All unique project names
    pub fn names(&self) -> Vec<String> {
        self.projects.iter().map(|p| p.name.clone()).collect()
    }

    /// Enabled project names
    pub fn enabled_names(&self) -> Vec<String> {
        self.projects
            .iter()
            .filter_map(|p| {
                if p.enabled {
                    Some(p.name.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Add a new project, auto-disambiguating name collisions
    pub fn register(&mut self, entry: ProjectEntry) -> Result<()> {
        if self.is_registered(&entry.root) {
            anyhow::bail!(
                "Project at root '{}' is already registered",
                entry.root.display()
            );
        }
        let name = Self::disambiguate_name(&self.projects, &entry.name);
        let entry = ProjectEntry { name, ..entry };
        self.projects.push(entry);
        self.save()
    }

    /// Remove a project by name. Returns true if found.
    pub fn unregister(&mut self, name: &str) -> Result<bool> {
        let before = self.projects.len();
        self.projects.retain(|p| p.name != name);
        let removed = self.projects.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// Pause (disable) a project by name
    pub fn pause(&mut self, name: &str) -> Result<bool> {
        if let Some(p) = self.projects.iter_mut().find(|p| p.name == name) {
            p.enabled = false;
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Resume (enable) a project by name
    pub fn resume(&mut self, name: &str) -> Result<bool> {
        if let Some(p) = self.projects.iter_mut().find(|p| p.name == name) {
            p.enabled = true;
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Auto-disambiguate duplicate names: foo → foo-1, foo-1 → foo-2, etc.
    pub fn disambiguate_name(projects: &[ProjectEntry], base: &str) -> String {
        let names: HashSet<String> = projects.iter().map(|p| p.name.clone()).collect();
        if !names.contains(base) {
            return base.to_string();
        }
        let mut n = 1;
        loop {
            let candidate = format!("{}-{}", base, n);
            if !names.contains(&candidate) {
                return candidate;
            }
            n += 1;
            if n > 10_000 {
                // Safety valve
                return format!("{}-{}", base, uuid::Uuid::new_v4());
            }
        }
    }

    /// Derive the canonical database path for a project under ~/.magellan/
    pub fn canonical_db_path(name: &str) -> PathBuf {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        home.join(".magellan")
            .join(name)
            .join(format!("{}.db", name))
    }

    /// Ensure the database directory exists
    pub fn ensure_db_dir(name: &str) -> Result<PathBuf> {
        let path = Self::canonical_db_path(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_registry() -> (Registry, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.toml");
        let reg = Registry::load_from(path).unwrap();
        (reg, dir)
    }

    fn make_entry(name: &str) -> ProjectEntry {
        ProjectEntry::new(
            name.to_string(),
            PathBuf::from(format!("/tmp/roots/{}", name)),
            PathBuf::from(format!("/tmp/dbs/{}.db", name)),
            "cargo".to_string(),
        )
    }

    #[test]
    fn registry_register_and_find() {
        let (mut reg, _dir) = tmp_registry();
        let e = make_entry("test");
        reg.register(e.clone()).unwrap();
        assert_eq!(reg.list().len(), 1);
        assert!(reg.find("test").is_some());
        assert_eq!(reg.find("test").unwrap().name, "test");
    }

    #[test]
    fn registry_unregister() {
        let (mut reg, _dir) = tmp_registry();
        reg.register(make_entry("a")).unwrap();
        reg.register(make_entry("b")).unwrap();
        assert!(reg.unregister("a").unwrap());
        assert!(reg.find("a").is_none());
        assert!(reg.find("b").is_some());
    }

    #[test]
    fn registry_roundtrip() {
        let (mut reg, _dir) = tmp_registry();
        reg.register(make_entry("x")).unwrap();
        // Reload via the same path
        let reg2 = Registry::load_from(reg.path.clone()).unwrap();
        assert_eq!(reg2.list().len(), 1);
        assert_eq!(reg2.find("x").unwrap().name, "x");
    }

    fn make_entry_with_root(name: &str, root_name: &str) -> ProjectEntry {
        ProjectEntry::new(
            name.to_string(),
            PathBuf::from(format!("/tmp/roots/{}", root_name)),
            PathBuf::from(format!("/tmp/dbs/{}.db", name)),
            "cargo".to_string(),
        )
    }

    #[test]
    fn registry_duplicate_name_disambiguates() {
        let (mut reg, _dir) = tmp_registry();
        reg.register(make_entry_with_root("foo", "foo-0")).unwrap();
        reg.register(make_entry_with_root("foo", "foo-1")).unwrap();
        let names = reg.names();
        assert!(names.contains(&"foo".to_string()));
        assert!(
            names.contains(&"foo-1".to_string()),
            "Expected foo-1 but got {:?}",
            names
        );
    }
}
