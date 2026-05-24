//! Framework API — unified programmatic entry point for Magellan

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use crate::graph::query::SymbolQueryResult;
use crate::graph::CodeGraph;

// ---------------------------------------------------------------------------
// Registry reading (self-contained, no binary deps)
// ---------------------------------------------------------------------------

const DEFAULT_REGISTRY_PATH: &str = "/home/feanor/.config/magellan/registry.toml";

#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    project: Vec<RegistryEntry>,
}

#[derive(Debug, Deserialize)]
struct RegistryEntry {
    name: String,
    db: PathBuf,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

fn load_registry_entries(path: &Path) -> Result<Vec<RegistryEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    let file: RegistryFile = toml::from_str(&content)?;
    Ok(file.project)
}

// ---------------------------------------------------------------------------
// Intent detection (self-contained)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Intent {
    Find,
    Callers,
    Callees,
    Search,
    Other,
}

fn detect_intent(q: &str) -> Intent {
    let q = q.to_lowercase();
    if [
        "who calls",
        "who uses",
        "callers of",
        "who references",
        "who invokes",
        "dependencies of",
        "dependents of",
        "who depends on",
    ]
    .iter()
    .any(|p| q.contains(p))
    {
        return Intent::Callers;
    }
    if [
        "callees of",
        "calls from",
        "outgoing calls",
        "called by",
        "who is called by",
    ]
    .iter()
    .any(|p| q.contains(p))
    {
        return Intent::Callees;
    }
    if ["search", "semantic", "find code", "look for"]
        .iter()
        .any(|p| q.contains(p))
    {
        return Intent::Search;
    }
    if ["find", "locate", "where is", "show me", "what is"]
        .iter()
        .any(|p| q.contains(p))
    {
        return Intent::Find;
    }
    Intent::Other
}

fn extract_term(query: &str, prefixes: &[&str]) -> String {
    let q = query.to_lowercase();
    for prefix in prefixes {
        if let Some(rest) = q.strip_prefix(prefix) {
            let term = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !term.is_empty() {
                return term;
            }
        }
    }
    query.to_string()
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A symbol result annotated with the project it came from
#[derive(Debug, Clone)]
pub struct FrameworkSymbol {
    pub project: String,
    pub entity_id: i64,
    pub name: String,
    pub file_path: String,
    pub kind: String,
}

impl FrameworkSymbol {
    fn from_result(project: String, r: SymbolQueryResult) -> Self {
        Self {
            project,
            entity_id: r.entity_id,
            name: r.name,
            file_path: r.file_path,
            kind: r.kind,
        }
    }
}

/// Handle to a single project's graph within a `MagellanFramework`
pub struct ProjectHandle<'a> {
    pub name: String,
    graph: &'a CodeGraph,
}

impl<'a> ProjectHandle<'a> {
    /// Find all symbols with this exact name in the project
    pub fn find_symbols_by_name(&self, name: &str) -> Result<Vec<SymbolQueryResult>> {
        self.graph.search_symbols_by_name(name)
    }
}

// ---------------------------------------------------------------------------
// MagellanFramework
// ---------------------------------------------------------------------------

/// Unified framework wrapping multiple project databases
pub struct MagellanFramework {
    graphs: HashMap<String, CodeGraph>,
}

impl MagellanFramework {
    /// Open all enabled projects from the default registry file
    pub fn from_registry() -> Result<Self> {
        Self::from_registry_file(Path::new(DEFAULT_REGISTRY_PATH))
    }

    /// Open all enabled projects from a registry file at the given path
    pub fn from_registry_file(path: &Path) -> Result<Self> {
        let entries = load_registry_entries(path)?;
        let db_paths: Vec<(String, PathBuf)> = entries
            .into_iter()
            .filter(|e| e.enabled)
            .map(|e| (e.name, e.db))
            .collect();
        Self::from_db_paths(db_paths)
    }

    /// Open projects from an explicit list of `(name, db_path)` pairs
    pub fn from_db_paths(entries: Vec<(String, PathBuf)>) -> Result<Self> {
        let mut graphs = HashMap::new();
        for (name, db) in entries {
            match CodeGraph::open(&db) {
                Ok(g) => {
                    graphs.insert(name, g);
                }
                Err(e) => {
                    eprintln!("Warning: skipping {}: {}", db.display(), e);
                }
            }
        }
        Ok(Self { graphs })
    }

    /// Number of successfully opened project databases
    pub fn project_count(&self) -> usize {
        self.graphs.len()
    }

    /// Sorted names of all opened projects
    pub fn project_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.graphs.keys().cloned().collect();
        names.sort();
        names
    }

    /// Search for symbols by exact name across all projects
    pub fn find(&self, name: &str) -> Result<Vec<FrameworkSymbol>> {
        let mut results = Vec::new();
        for (proj_name, graph) in &self.graphs {
            match graph.search_symbols_by_name(name) {
                Ok(syms) => {
                    for s in syms {
                        results.push(FrameworkSymbol::from_result(proj_name.clone(), s));
                    }
                }
                Err(e) => {
                    eprintln!("Warning: find failed for {}: {}", proj_name, e);
                }
            }
        }
        Ok(results)
    }

    /// Get a read-only handle to a single named project
    pub fn project(&self, name: &str) -> Option<ProjectHandle<'_>> {
        self.graphs
            .get_key_value(name)
            .map(|(k, graph)| ProjectHandle {
                name: k.clone(),
                graph,
            })
    }

    /// Route a natural-language query and return a formatted text response
    pub fn ask(&self, query: &str) -> Result<String> {
        let intent = detect_intent(query);
        let term = match intent {
            Intent::Find => {
                extract_term(query, &["find", "locate", "where is", "show me", "what is"])
            }
            Intent::Callers => extract_term(
                query,
                &["who calls", "who uses", "callers of", "who references"],
            ),
            Intent::Callees => extract_term(query, &["callees of", "calls from"]),
            Intent::Search | Intent::Other => query.to_string(),
        };

        let syms = self.find(&term)?;
        if syms.is_empty() {
            return Ok(format!("No symbols found matching '{}'", term));
        }
        let lines: Vec<String> = syms
            .iter()
            .map(|s| format!("  {} ({}) in {}:{}", s.name, s.kind, s.project, s.file_path))
            .collect();
        Ok(lines.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_db(dir: &TempDir, name: &str, source: &[u8]) -> PathBuf {
        let db_path = dir.path().join(format!("{}.db", name));
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();
        graph.index_file("src/lib.rs", source).unwrap();
        drop(graph);
        db_path
    }

    #[test]
    fn test_from_empty_entries() {
        let fw = MagellanFramework::from_db_paths(vec![]).unwrap();
        assert_eq!(fw.project_count(), 0);
        assert!(fw.project_names().is_empty());
    }

    #[test]
    fn test_find_on_empty_returns_empty() {
        let fw = MagellanFramework::from_db_paths(vec![]).unwrap();
        assert!(fw.find("anything").unwrap().is_empty());
    }

    #[test]
    fn test_project_missing_returns_none() {
        let fw = MagellanFramework::from_db_paths(vec![]).unwrap();
        assert!(fw.project("nonexistent").is_none());
    }

    #[test]
    fn test_find_across_real_db() {
        let dir = tempfile::tempdir().unwrap();
        let db = make_db(&dir, "proj", b"pub fn target_fn() {}");

        let fw = MagellanFramework::from_db_paths(vec![("testproj".into(), db)]).unwrap();

        let results = fw.find("target_fn").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project, "testproj");
        assert_eq!(results[0].name, "target_fn");
    }

    #[test]
    fn test_project_handle_find_symbols() {
        let dir = tempfile::tempdir().unwrap();
        let db = make_db(&dir, "myproj", b"fn main() {} fn helper() {}");

        let fw = MagellanFramework::from_db_paths(vec![("myproj".into(), db)]).unwrap();

        let handle = fw.project("myproj").unwrap();
        let syms = handle.find_symbols_by_name("main").unwrap();
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "main");
    }

    #[test]
    fn test_ask_find_intent() {
        let dir = tempfile::tempdir().unwrap();
        let db = make_db(&dir, "askproj", b"pub fn my_special_fn() {}");

        let fw = MagellanFramework::from_db_paths(vec![("askproj".into(), db)]).unwrap();

        let response = fw.ask("find my_special_fn").unwrap();
        assert!(
            response.contains("my_special_fn"),
            "Expected symbol name in response, got: {}",
            response
        );
    }

    #[test]
    fn test_ask_no_results() {
        let fw = MagellanFramework::from_db_paths(vec![]).unwrap();
        let response = fw.ask("find nonexistent_symbol").unwrap();
        assert!(response.contains("No symbols found"));
    }

    #[test]
    fn test_project_names_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let db_z = make_db(&dir, "z_proj", b"fn z() {}");
        let db_a = make_db(&dir, "a_proj", b"fn a() {}");

        let fw = MagellanFramework::from_db_paths(vec![
            ("z_proj".into(), db_z),
            ("a_proj".into(), db_a),
        ])
        .unwrap();

        let names = fw.project_names();
        assert_eq!(names, vec!["a_proj", "z_proj"]);
    }
}
