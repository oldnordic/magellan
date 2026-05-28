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
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cargo_manifest_detect_include_paths_extracts_target_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_toml = r#"
[package]
name = "test-crate"

[[bin]]
name = "mybin"
path = "src/main.rs"

[[test]]
name = "integration"
path = "tests/integration.rs"

[[bench]]
name = "perf"
path = "benches/perf.rs"
"#;
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

        let manifest = CargoManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"tests/".to_string()));
        assert!(paths.contains(&"benches/".to_string()));
    }

    #[test]
    fn cargo_manifest_detect_include_paths_with_examples() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_toml = r#"
[package]
name = "test-crate"

[[example]]
name = "demo"
path = "examples/demo.rs"
"#;
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

        let manifest = CargoManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"examples/".to_string()));
    }

    #[test]
    fn cargo_manifest_detect_include_paths_with_lib_path() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_toml = r#"
[package]
name = "test-crate"

[lib]
path = "lib/my_crate.rs"
"#;
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

        let manifest = CargoManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert!(paths.contains(&"lib/".to_string()));
    }

    #[test]
    fn cargo_manifest_parse_extracts_examples_and_lib() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_toml = r#"
[package]
name = "test-crate"

[lib]
path = "src/lib.rs"

[[example]]
name = "basic"
path = "examples/basic.rs"

[[example]]
name = "advanced"
path = "examples/advanced/main.rs"
"#;
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

        let manifest = CargoManifest::parse(dir.path()).unwrap();

        assert!(manifest.targets.contains(&"src/lib.rs".to_string()));
        assert!(manifest.targets.contains(&"examples/basic.rs".to_string()));
        assert!(manifest
            .targets
            .contains(&"examples/advanced/main.rs".to_string()));
    }

    #[test]
    fn cargo_manifest_detect_include_paths_no_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_toml = r#"
[package]
name = "test-crate"

[[bin]]
name = "a"
path = "src/bin/a.rs"

[[bin]]
name = "b"
path = "src/bin/b.rs"

[[test]]
name = "t1"
path = "tests/t1.rs"
"#;
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

        let manifest = CargoManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert_eq!(paths.iter().filter(|p| *p == "src/").count(), 1);
        assert_eq!(paths.iter().filter(|p| *p == "tests/").count(), 1);
    }

    #[test]
    fn cargo_manifest_detect_include_paths_empty_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_toml = r#"
[package]
name = "minimal"
"#;
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

        let manifest = CargoManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert_eq!(paths, vec!["src/".to_string()]);
    }

    #[test]
    fn pyproject_manifest_parse_minimal() {
        let dir = tempfile::tempdir().unwrap();
        let pyproject = r#"
[project]
name = "my-pkg"
version = "0.1.0"
"#;
        fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

        let manifest = PyprojectManifest::parse(dir.path()).unwrap();
        assert_eq!(manifest.package_name, Some("my-pkg".to_string()));
        assert!(manifest.packages.is_empty());
        assert!(manifest.test_dirs.is_empty());
    }

    #[test]
    fn pyproject_manifest_parse_setuptools_packages() {
        let dir = tempfile::tempdir().unwrap();
        let pyproject = r#"
[project]
name = "my-pkg"

[tool.setuptools.packages.find]
where = ["src"]
"#;
        fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

        let manifest = PyprojectManifest::parse(dir.path()).unwrap();
        assert_eq!(manifest.packages, vec!["src".to_string()]);
    }

    #[test]
    fn pyproject_manifest_parse_pytest_testpaths() {
        let dir = tempfile::tempdir().unwrap();
        let pyproject = r#"
[project]
name = "my-pkg"

[tool.pytest.ini_options]
testpaths = ["tests", "integration"]
"#;
        fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

        let manifest = PyprojectManifest::parse(dir.path()).unwrap();
        assert_eq!(
            manifest.test_dirs,
            vec!["tests".to_string(), "integration".to_string()]
        );
    }

    #[test]
    fn pyproject_manifest_detect_include_paths() {
        let dir = tempfile::tempdir().unwrap();
        let pyproject = r#"
[project]
name = "my-pkg"

[tool.setuptools.packages.find]
where = ["src"]

[tool.pytest.ini_options]
testpaths = ["tests"]
"#;
        fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

        let manifest = PyprojectManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"tests/".to_string()));
    }

    #[test]
    fn pyproject_manifest_detect_include_paths_no_config() {
        let dir = tempfile::tempdir().unwrap();
        let pyproject = r#"
[project]
name = "my-pkg"
"#;
        fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

        let manifest = PyprojectManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();
        assert!(paths.contains(&"src/".to_string()));
    }

    #[test]
    fn pyproject_manifest_detect_include_paths_flat_layout() {
        let dir = tempfile::tempdir().unwrap();
        let pyproject = r#"
[project]
name = "my-pkg"

[tool.setuptools]
packages = ["my_pkg", "my_pkg.utils"]
"#;
        fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

        let manifest = PyprojectManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();
        assert!(!paths.is_empty());
    }

    #[test]
    fn go_module_manifest_parse() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module github.com/example/myapp\n\ngo 1.22\n",
        )
        .unwrap();

        let manifest = GoModuleManifest::parse(dir.path()).unwrap();
        assert_eq!(
            manifest.module_name,
            Some("github.com/example/myapp".to_string())
        );
    }

    #[test]
    fn go_module_manifest_detect_convention_dirs() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module example.com/app\n\ngo 1.22\n",
        )
        .unwrap();
        fs::create_dir(dir.path().join("cmd")).unwrap();
        fs::create_dir(dir.path().join("internal")).unwrap();

        let manifest = GoModuleManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths(dir.path());

        assert!(paths.contains(&"cmd/".to_string()));
        assert!(paths.contains(&"internal/".to_string()));
    }

    #[test]
    fn go_module_manifest_no_convention_dirs_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module example.com/app\n\ngo 1.22\n",
        )
        .unwrap();

        let manifest = GoModuleManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths(dir.path());

        assert_eq!(paths, vec!["src/".to_string()]);
    }

    #[test]
    fn package_json_manifest_parse() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = r#"{"name": "my-lib", "main": "src/index.js", "files": ["dist/", "README.md"]}"#;
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        let manifest = PackageJsonManifest::parse(dir.path()).unwrap();
        assert_eq!(manifest.name, Some("my-lib".to_string()));
        assert_eq!(manifest.main, Some("src/index.js".to_string()));
        assert_eq!(manifest.files, vec!["dist/", "README.md"]);
    }

    #[test]
    fn package_json_manifest_detect_include_paths_from_main() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = r#"{"name": "my-lib", "main": "lib/index.js"}"#;
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        let manifest = PackageJsonManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert!(paths.contains(&"lib/".to_string()));
    }

    #[test]
    fn package_json_manifest_detect_include_paths_from_files() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = r#"{"name": "my-lib", "files": ["src/", "dist/"]}"#;
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        let manifest = PackageJsonManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"dist/".to_string()));
    }

    #[test]
    fn tsconfig_manifest_parse_include() {
        let dir = tempfile::tempdir().unwrap();
        let tsconfig = r#"{"include": ["src/**/*", "tests/**/*"], "exclude": ["node_modules"]}"#;
        fs::write(dir.path().join("tsconfig.json"), tsconfig).unwrap();

        let manifest = TsconfigManifest::parse(dir.path()).unwrap();
        assert_eq!(manifest.include, vec!["src/**/*", "tests/**/*"]);
    }

    #[test]
    fn tsconfig_manifest_detect_include_paths() {
        let dir = tempfile::tempdir().unwrap();
        let tsconfig = r#"{"include": ["src/**/*", "tests/**/*.ts"]}"#;
        fs::write(dir.path().join("tsconfig.json"), tsconfig).unwrap();

        let manifest = TsconfigManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"tests/".to_string()));
    }

    #[test]
    fn tsconfig_manifest_detect_include_paths_dot_slash() {
        let dir = tempfile::tempdir().unwrap();
        let tsconfig = r#"{"include": ["./src/", "./lib/"]}"#;
        fs::write(dir.path().join("tsconfig.json"), tsconfig).unwrap();

        let manifest = TsconfigManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"lib/".to_string()));
    }

    #[test]
    fn maven_manifest_parse() {
        let dir = tempfile::tempdir().unwrap();
        let pom = r#"<?xml version="1.0"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
    <version>1.0</version>
</project>"#;
        fs::write(dir.path().join("pom.xml"), pom).unwrap();

        let manifest = MavenManifest::parse(dir.path()).unwrap();
        assert_eq!(manifest.group_id, Some("com.example".to_string()));
        assert_eq!(manifest.artifact_id, Some("my-app".to_string()));
    }

    #[test]
    fn maven_manifest_detect_include_paths() {
        let dir = tempfile::tempdir().unwrap();
        let pom = r#"<?xml version="1.0"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
</project>"#;
        fs::write(dir.path().join("pom.xml"), pom).unwrap();
        fs::create_dir_all(dir.path().join("src/main/java")).unwrap();
        fs::create_dir_all(dir.path().join("src/test/java")).unwrap();

        let manifest = MavenManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths(dir.path());

        assert!(paths.contains(&"src/main/java/".to_string()));
        assert!(paths.contains(&"src/test/java/".to_string()));
    }

    #[test]
    fn maven_manifest_no_dirs_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let pom = r#"<?xml version="1.0"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
</project>"#;
        fs::write(dir.path().join("pom.xml"), pom).unwrap();

        let manifest = MavenManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths(dir.path());

        assert_eq!(paths, vec!["src/".to_string()]);
    }

    #[test]
    fn cmake_manifest_parse() {
        let dir = tempfile::tempdir().unwrap();
        let cmake = r#"cmake_minimum_required(VERSION 3.20)
project(MyProject)

add_subdirectory(src)
add_subdirectory(tests)
add_subdirectory(lib)
"#;
        fs::write(dir.path().join("CMakeLists.txt"), cmake).unwrap();

        let manifest = CMakeManifest::parse(dir.path()).unwrap();
        assert_eq!(manifest.project_name, Some("MyProject".to_string()));
        assert_eq!(manifest.subdirectories, vec!["src", "tests", "lib"]);
    }

    #[test]
    fn cmake_manifest_detect_include_paths() {
        let dir = tempfile::tempdir().unwrap();
        let cmake = r#"cmake_minimum_required(VERSION 3.20)
project(MyProject)
add_subdirectory(src)
add_subdirectory(include)
"#;
        fs::write(dir.path().join("CMakeLists.txt"), cmake).unwrap();

        let manifest = CMakeManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"include/".to_string()));
    }

    #[test]
    fn cmake_manifest_no_subdirs_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let cmake = r#"cmake_minimum_required(VERSION 3.20)
project(MyProject)
"#;
        fs::write(dir.path().join("CMakeLists.txt"), cmake).unwrap();

        let manifest = CMakeManifest::parse(dir.path()).unwrap();
        let paths = manifest.detect_include_paths();

        assert_eq!(paths, vec!["src/".to_string()]);
    }

    #[test]
    fn detect_include_paths_prefers_cargo_over_pyproject() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            r#"[package]
name = "rust-project"

[[test]]
name = "t"
path = "tests/t.rs"
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            r#"[project]
name = "py-part"

[tool.setuptools.packages.find]
where = ["python_src"]
"#,
        )
        .unwrap();

        let paths = detect_include_paths_from_root(dir.path());

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"tests/".to_string()));
    }

    #[test]
    fn detect_include_paths_falls_back_to_pyproject() {
        let dir = tempfile::tempdir().unwrap();
        let pyproject = r#"
[project]
name = "my-pkg"

[tool.setuptools.packages.find]
where = ["src"]

[tool.pytest.ini_options]
testpaths = ["tests"]
"#;
        fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

        let paths = detect_include_paths_from_root(dir.path());

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"tests/".to_string()));
    }

    #[test]
    fn detect_include_paths_go_module() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module example.com/app\n\ngo 1.22\n",
        )
        .unwrap();
        fs::create_dir(dir.path().join("cmd")).unwrap();
        fs::create_dir(dir.path().join("pkg")).unwrap();

        let paths = detect_include_paths_from_root(dir.path());

        assert!(paths.contains(&"cmd/".to_string()));
        assert!(paths.contains(&"pkg/".to_string()));
    }

    #[test]
    fn detect_include_paths_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = r#"{"name": "my-lib", "main": "src/index.js", "files": ["dist/"]}"#;
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        let paths = detect_include_paths_from_root(dir.path());

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"dist/".to_string()));
    }

    #[test]
    fn detect_include_paths_package_json_with_tsconfig() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name": "my-ts-lib"}"#).unwrap();
        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"include": ["src/", "tests/"]}"#,
        )
        .unwrap();

        let paths = detect_include_paths_from_root(dir.path());

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"tests/".to_string()));
    }

    #[test]
    fn detect_include_paths_maven() {
        let dir = tempfile::tempdir().unwrap();
        let pom = r#"<?xml version="1.0"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
</project>"#;
        fs::write(dir.path().join("pom.xml"), pom).unwrap();
        fs::create_dir_all(dir.path().join("src/main/java")).unwrap();

        let paths = detect_include_paths_from_root(dir.path());

        assert!(paths.contains(&"src/main/java/".to_string()));
    }

    #[test]
    fn detect_include_paths_cmake() {
        let dir = tempfile::tempdir().unwrap();
        let cmake = r#"cmake_minimum_required(VERSION 3.20)
project(MyProject)
add_subdirectory(src)
add_subdirectory(lib)
"#;
        fs::write(dir.path().join("CMakeLists.txt"), cmake).unwrap();

        let paths = detect_include_paths_from_root(dir.path());

        assert!(paths.contains(&"src/".to_string()));
        assert!(paths.contains(&"lib/".to_string()));
    }

    #[test]
    fn detect_include_paths_no_manifest_returns_src_default() {
        let dir = tempfile::tempdir().unwrap();

        let paths = detect_include_paths_from_root(dir.path());

        assert_eq!(paths, vec!["src/".to_string()]);
    }
}
