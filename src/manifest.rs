//! Project manifest auto-detection for include paths.
//!
//! Parses language-specific manifest files (`Cargo.toml`, `pyproject.toml`,
//! `go.mod`, `package.json`, `tsconfig.json`, `pom.xml`, `CMakeLists.txt`)
//! to extract source directory conventions. Used by `ProjectConfig::init()`
//! and the watch pipeline to auto-populate include paths.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

fn ensure_trailing_slash(s: &str) -> String {
    if s.ends_with('/') {
        s.to_string()
    } else {
        format!("{}/", s)
    }
}

fn sorted_dirs(dirs: HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = dirs.into_iter().collect();
    v.sort();
    v
}

// -- Cargo.toml ----------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CargoManifest {
    pub package_name: Option<String>,
    #[serde(default)]
    pub features: std::collections::HashMap<String, Vec<String>>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
}

impl CargoManifest {
    pub fn parse(project_root: &Path) -> Result<Self> {
        let mut path = project_root.join("Cargo.toml");
        let mut current = Some(project_root);
        while !path.exists() {
            if let Some(p) = current.and_then(|c| c.parent()) {
                path = p.join("Cargo.toml");
                current = Some(p);
            } else {
                break;
            }
        }
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let doc: toml::Table = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;

        let mut manifest = Self::default();

        if let Some(toml::Value::Table(package)) = doc.get("package") {
            if let Some(toml::Value::String(name)) = package.get("name") {
                manifest.package_name = Some(name.clone());
            }
        }

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

        if let Some(toml::Value::Table(deps)) = doc.get("dependencies") {
            manifest.dependencies = deps.keys().cloned().collect();
        }

        for key in ["bin", "test", "bench", "example"] {
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

        if let Some(toml::Value::Table(lib)) = doc.get("lib") {
            if let Some(toml::Value::String(path)) = lib.get("path") {
                manifest.targets.push(path.clone());
            }
        }

        Ok(manifest)
    }

    pub fn store_in_db(&self, conn: &rusqlite::Connection) -> Result<()> {
        let metadata_json =
            serde_json::to_string(self).context("Failed to serialize CargoManifest")?;
        conn.execute(
            "UPDATE magellan_meta SET project_name = ?1, project_metadata = ?2 WHERE id = 1",
            rusqlite::params![self.package_name, metadata_json],
        )
        .context("Failed to update magellan_meta with project metadata")?;
        Ok(())
    }

    pub fn detect_include_paths(&self) -> Vec<String> {
        let mut dirs: HashSet<String> = HashSet::new();
        dirs.insert("src/".to_string());

        for target in &self.targets {
            if let Some(parent) = Path::new(target).parent() {
                let s = parent.to_string_lossy().to_string();
                if !s.is_empty() {
                    dirs.insert(ensure_trailing_slash(&s));
                }
            }
        }

        sorted_dirs(dirs)
    }
}

// -- pyproject.toml ------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PyprojectManifest {
    pub package_name: Option<String>,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub test_dirs: Vec<String>,
}

impl PyprojectManifest {
    pub fn parse(project_root: &Path) -> Result<Self> {
        let path = project_root.join("pyproject.toml");
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let doc: toml::Table = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;

        let mut manifest = Self::default();

        if let Some(toml::Value::Table(project)) = doc.get("project") {
            if let Some(toml::Value::String(name)) = project.get("name") {
                manifest.package_name = Some(name.clone());
            }
        }

        if let Some(toml::Value::Table(tool)) = doc.get("tool") {
            if let Some(toml::Value::Table(setuptools)) = tool.get("setuptools") {
                if let Some(toml::Value::Table(packages)) = setuptools.get("packages") {
                    if let Some(toml::Value::Table(find)) = packages.get("find") {
                        if let Some(toml::Value::Array(where_)) = find.get("where") {
                            manifest.packages = where_
                                .iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();
                        }
                    }
                }
            }

            if let Some(toml::Value::Table(pytest)) = tool.get("pytest") {
                if let Some(toml::Value::Table(ini_opts)) = pytest.get("ini_options") {
                    if let Some(toml::Value::Array(testpaths)) = ini_opts.get("testpaths") {
                        manifest.test_dirs = testpaths
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                    }
                }
            }
        }

        Ok(manifest)
    }

    pub fn detect_include_paths(&self) -> Vec<String> {
        let mut dirs: HashSet<String> = HashSet::new();

        if !self.packages.is_empty() {
            for pkg in &self.packages {
                dirs.insert(ensure_trailing_slash(pkg));
            }
        } else {
            dirs.insert("src/".to_string());
        }

        for test_dir in &self.test_dirs {
            dirs.insert(ensure_trailing_slash(test_dir));
        }

        sorted_dirs(dirs)
    }
}

// -- go.mod --------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoModuleManifest {
    pub module_name: Option<String>,
}

impl GoModuleManifest {
    pub fn parse(project_root: &Path) -> Result<Self> {
        let path = project_root.join("go.mod");
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let module_name = content
            .lines()
            .find(|l| l.starts_with("module "))
            .map(|l| l.trim_start_matches("module ").trim().to_string());

        Ok(Self { module_name })
    }

    pub fn detect_include_paths(&self, root: &Path) -> Vec<String> {
        let mut dirs: HashSet<String> = HashSet::new();

        let convention_dirs = ["cmd/", "internal/", "pkg/", "api/", "web/"];
        for dir in &convention_dirs {
            if root.join(dir).is_dir() {
                dirs.insert(dir.to_string());
            }
        }

        if dirs.is_empty() {
            dirs.insert("src/".to_string());
        }

        sorted_dirs(dirs)
    }
}

// -- package.json (JavaScript / TypeScript) ------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageJsonManifest {
    pub name: Option<String>,
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub exports: std::collections::HashMap<String, serde_json::Value>,
}

impl PackageJsonManifest {
    pub fn parse(project_root: &Path) -> Result<Self> {
        let path = project_root.join("package.json");
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let manifest: Self =
            serde_json::from_str(&content).with_context(|| "Failed to parse package.json")?;

        Ok(manifest)
    }

    pub fn detect_include_paths(&self) -> Vec<String> {
        let mut dirs: HashSet<String> = HashSet::new();

        if let Some(main) = &self.main {
            if let Some(parent) = Path::new(main).parent() {
                let s = parent.to_string_lossy().to_string();
                if !s.is_empty() {
                    dirs.insert(ensure_trailing_slash(&s));
                }
            }
        }

        for file in &self.files {
            let p = Path::new(file);
            if p.extension().is_none() {
                dirs.insert(ensure_trailing_slash(file));
            }
        }

        for key in self.exports.keys() {
            let val = &self.exports[key];
            if let Some(s) = val.as_str() {
                if let Some(parent) = Path::new(s).parent() {
                    let d = parent.to_string_lossy().to_string();
                    if !d.is_empty() {
                        dirs.insert(ensure_trailing_slash(&d));
                    }
                }
            }
        }

        if dirs.is_empty() {
            dirs.insert("src/".to_string());
        }

        sorted_dirs(dirs)
    }
}

// -- tsconfig.json -------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TsconfigManifest {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl TsconfigManifest {
    pub fn parse(project_root: &Path) -> Result<Self> {
        let path = project_root.join("tsconfig.json");
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let manifest: Self =
            serde_json::from_str(&content).with_context(|| "Failed to parse tsconfig.json")?;

        Ok(manifest)
    }

    pub fn detect_include_paths(&self) -> Vec<String> {
        let mut dirs: HashSet<String> = HashSet::new();

        for pattern in &self.include {
            let cleaned = pattern.trim_start_matches("./");
            let dir = if cleaned.contains('/') {
                let parts: Vec<&str> = cleaned.split('/').collect();
                let mut dir_parts = Vec::new();
                for part in &parts {
                    if part.contains('*') || part.contains('?') {
                        break;
                    }
                    dir_parts.push(*part);
                }
                if dir_parts.is_empty() || dir_parts == [""] {
                    continue;
                }
                dir_parts.join("/")
            } else {
                continue;
            };
            if !dir.is_empty() {
                dirs.insert(ensure_trailing_slash(&dir));
            }
        }

        if dirs.is_empty() {
            dirs.insert("src/".to_string());
        }

        sorted_dirs(dirs)
    }
}

// -- pom.xml (Java / Maven) ---------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MavenManifest {
    pub group_id: Option<String>,
    pub artifact_id: Option<String>,
}

impl MavenManifest {
    pub fn parse(project_root: &Path) -> Result<Self> {
        let path = project_root.join("pom.xml");
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let group_id = content
            .lines()
            .skip_while(|l| !l.contains("<groupId>"))
            .find(|l| l.contains("<groupId>"))
            .and_then(|l| {
                let start = l.find("<groupId>")? + "<groupId>".len();
                let end = l.find("</groupId>")?;
                Some(l[start..end].trim().to_string())
            });

        let artifact_id = content
            .lines()
            .find(|l| l.contains("<artifactId>"))
            .and_then(|l| {
                let start = l.find("<artifactId>")? + "<artifactId>".len();
                let end = l.find("</artifactId>")?;
                Some(l[start..end].trim().to_string())
            });

        Ok(Self {
            group_id,
            artifact_id,
        })
    }

    pub fn detect_include_paths(&self, root: &Path) -> Vec<String> {
        let mut dirs: HashSet<String> = HashSet::new();

        let maven_dirs = [
            "src/main/java/",
            "src/main/resources/",
            "src/test/java/",
            "src/test/resources/",
        ];
        for dir in &maven_dirs {
            if root.join(dir).is_dir() {
                dirs.insert(dir.to_string());
            }
        }

        if dirs.is_empty() {
            dirs.insert("src/".to_string());
        }

        sorted_dirs(dirs)
    }
}

// -- CMakeLists.txt (C / C++ / CUDA) ------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CMakeManifest {
    pub project_name: Option<String>,
    #[serde(default)]
    pub subdirectories: Vec<String>,
}

impl CMakeManifest {
    pub fn parse(project_root: &Path) -> Result<Self> {
        let path = project_root.join("CMakeLists.txt");
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let project_name = content
            .lines()
            .find(|l| l.trim_start().starts_with("project("))
            .and_then(|l| {
                let start = l.find("project(")? + "project(".len();
                let rest = &l[start..];
                let end = rest.find(|c: char| c == ')' || c.is_whitespace())?;
                let name = rest[..end].trim().to_string();
                if name.is_empty() {
                    None
                } else {
                    Some(name)
                }
            });

        let subdirectories: Vec<String> = content
            .lines()
            .filter_map(|l| {
                let trimmed = l.trim();
                if !trimmed.starts_with("add_subdirectory(") {
                    return None;
                }
                let start = trimmed.find("add_subdirectory(")? + "add_subdirectory(".len();
                let rest = &trimmed[start..];
                #[allow(clippy::manual_pattern_char_comparison, reason = "clippy suggests ['')', ' ')] array but str::find does not accept const char arrays")]
                let end = rest.find(|c: char| c == ')' || c == ' ')?;
                let dir = rest[..end].trim().to_string();
                if dir.is_empty() {
                    None
                } else {
                    Some(dir)
                }
            })
            .collect();

        Ok(Self {
            project_name,
            subdirectories,
        })
    }

    pub fn detect_include_paths(&self) -> Vec<String> {
        let mut dirs: HashSet<String> = HashSet::new();

        for subdir in &self.subdirectories {
            dirs.insert(ensure_trailing_slash(subdir));
        }

        if dirs.is_empty() {
            dirs.insert("src/".to_string());
        }

        sorted_dirs(dirs)
    }
}

// -- Top-level dispatcher ------------------------------------------------------

pub fn detect_include_paths_from_root(root: &Path) -> Vec<String> {
    if let Ok(cargo) = CargoManifest::parse(root) {
        if cargo.package_name.is_some() {
            return cargo.detect_include_paths();
        }
    }

    if let Ok(pyproject) = PyprojectManifest::parse(root) {
        if pyproject.package_name.is_some() {
            return pyproject.detect_include_paths();
        }
    }

    if let Ok(go) = GoModuleManifest::parse(root) {
        if go.module_name.is_some() {
            return go.detect_include_paths(root);
        }
    }

    if let Ok(pkg) = PackageJsonManifest::parse(root) {
        if pkg.name.is_some() {
            let mut paths = pkg.detect_include_paths();

            if let Ok(ts) = TsconfigManifest::parse(root) {
                if !ts.include.is_empty() {
                    let ts_paths = ts.detect_include_paths();
                    let mut merged: HashSet<String> = paths.into_iter().chain(ts_paths).collect();
                    merged.insert("src/".to_string());
                    paths = sorted_dirs(merged);
                }
            }

            return paths;
        }
    }

    if let Ok(maven) = MavenManifest::parse(root) {
        if maven.artifact_id.is_some() {
            return maven.detect_include_paths(root);
        }
    }

    if let Ok(cmake) = CMakeManifest::parse(root) {
        if cmake.project_name.is_some() {
            return cmake.detect_include_paths();
        }
    }

    vec!["src/".to_string()]
}

// -- Tests ---------------------------------------------------------------------

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
