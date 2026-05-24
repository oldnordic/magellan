//! Multi-Database Context for Cross-Project Queries
//!
//! Provides a unified query interface over multiple project databases.
//! Each database is treated as an isolated project; cross-project edges
//! are runtime unions, not stored relationships.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::context::{
    affected_analysis, get_file_context, get_project_summary, get_symbol_detail,
    get_symbol_detail_recursive, impact_analysis, list_symbols, FileContext, ListQuery,
    PaginatedResult, ProjectSummary, SymbolDetail, SymbolListItem, SymbolRelation,
};
use crate::graph::CodeGraph;
use crate::output::{ProjectCalleeInfo, ProjectCallerInfo, ProjectSymbolMatch, Span};

/// A single graph connection with its project name
struct ProjectGraph {
    name: String,
    graph: CodeGraph,
}

/// Multi-database context for querying across project boundaries
pub struct MultiDbContext {
    projects: Vec<ProjectGraph>,
}

impl MultiDbContext {
    /// Open multiple databases from explicit paths
    pub fn from_paths(paths: &[PathBuf]) -> Result<Self> {
        let mut projects = Vec::with_capacity(paths.len());
        for path in paths {
            let name = project_name_from_path(path);
            match CodeGraph::open(path) {
                Ok(graph) => projects.push(ProjectGraph { name, graph }),
                Err(e) => eprintln!("Warning: skipping {}: {}", path.display(), e),
            }
        }
        Ok(Self { projects })
    }

    /// Discover databases in a directory
    pub fn from_directory(dir: &Path) -> Result<Self> {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "db") {
                paths.push(path);
            }
        }
        paths.sort();
        Self::from_paths(&paths)
    }

    /// Number of successfully opened projects
    pub fn project_count(&self) -> usize {
        self.projects.len()
    }

    /// Project names in order
    pub fn project_names(&self) -> Vec<String> {
        self.projects.iter().map(|p| p.name.clone()).collect()
    }

    /// Get summaries for all projects
    pub fn summaries(&mut self) -> Vec<(String, ProjectSummary)> {
        let mut results = Vec::with_capacity(self.projects.len());
        for project in &mut self.projects {
            match get_project_summary(&mut project.graph) {
                Ok(summary) => results.push((project.name.clone(), summary)),
                Err(e) => eprintln!("Warning: summary failed for {}: {}", project.name, e),
            }
        }
        results
    }

    /// Search for a symbol by name across all projects
    pub fn search_symbol(
        &mut self,
        name: &str,
        file: Option<&str>,
        depth: Option<usize>,
        include_callers: bool,
        include_callees: bool,
    ) -> Vec<ProjectSymbolMatch> {
        let mut results = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for project in &mut self.projects {
            let detail = match depth {
                Some(d) if d > 1 => get_symbol_detail_recursive(&mut project.graph, name, file, d),
                _ => get_symbol_detail(&mut project.graph, name, file),
            };

            let detail = match detail {
                Ok(d) => d,
                Err(_) => continue,
            };

            let key = format!("{}:{}:{}", project.name, detail.file, detail.line);
            if !seen.insert(key) {
                continue;
            }

            let span = Span {
                span_id: crate::output::Span::generate_id(
                    &detail.file,
                    detail.byte_start,
                    detail.byte_end,
                ),
                file_path: detail.file.clone(),
                byte_start: detail.byte_start,
                byte_end: detail.byte_end,
                start_line: detail.line,
                start_col: detail.start_col,
                end_line: detail.end_line,
                end_col: detail.end_col,
                context: None,
                semantics: None,
                relationships: None,
                checksums: None,
            };

            let callers = if include_callers {
                Some(
                    detail
                        .callers
                        .iter()
                        .map(|c| ProjectCallerInfo {
                            project: project.name.clone(),
                            name: c.name.clone(),
                            file_path: c.file.clone(),
                            line: c.line,
                            column: 0,
                            depth: c.depth,
                        })
                        .collect(),
                )
            } else {
                None
            };

            let callees = if include_callees {
                Some(
                    detail
                        .callees
                        .iter()
                        .map(|c| ProjectCalleeInfo {
                            project: project.name.clone(),
                            name: c.name.clone(),
                            file_path: c.file.clone(),
                            line: c.line as u32,
                            depth: c.depth,
                        })
                        .collect(),
                )
            } else {
                None
            };

            results.push(ProjectSymbolMatch {
                project: project.name.clone(),
                match_id: format!("{}::{}#{}", project.name, name, detail.line),
                span,
                name: name.to_string(),
                kind: detail.kind,
                parent: None,
                symbol_id: Some(format!("{}::{}#{}", project.name, name, detail.line)),
                callers,
                callees,
                source: None,
            });
        }

        results.sort_by(|a, b| {
            let score_a = score_match(name, &a.name);
            let score_b = score_match(name, &b.name);
            score_b.cmp(&score_a).then_with(|| {
                let callers_a = a.callers.as_ref().map(|c| c.len()).unwrap_or(0);
                let callers_b = b.callers.as_ref().map(|c| c.len()).unwrap_or(0);
                callers_b.cmp(&callers_a)
            })
        });
        results
    }

    /// List symbols across all projects with pagination
    pub fn list_symbols(&mut self, query: &ListQuery) -> Vec<(String, SymbolListItem)> {
        let mut all_items = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for project in &mut self.projects {
            let result = match list_symbols(&mut project.graph, query) {
                Ok(r) => r,
                Err(_) => continue,
            };

            for item in result.items {
                let key = format!("{}:{}:{}:{}", project.name, item.name, item.file, item.line);
                if seen.insert(key) {
                    all_items.push((project.name.clone(), item));
                }
            }
        }

        all_items
    }

    /// Impact analysis: all symbols that transitively call the target
    pub fn impact(
        &mut self,
        name: &str,
        file: Option<&str>,
        max_depth: usize,
    ) -> Vec<(String, SymbolRelation)> {
        let mut all_impacted = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for project in &mut self.projects {
            let impacted = match impact_analysis(&mut project.graph, name, file, max_depth) {
                Ok(i) => i,
                Err(_) => continue,
            };

            for relation in impacted {
                let key = format!("{}:{}:{}", project.name, relation.name, relation.file);
                if seen.insert(key) {
                    all_impacted.push((project.name.clone(), relation));
                }
            }
        }

        all_impacted
    }

    /// Affected analysis: all symbols that the target transitively calls
    pub fn affected(
        &mut self,
        name: &str,
        file: Option<&str>,
        max_depth: usize,
    ) -> Vec<(String, SymbolRelation)> {
        let mut all_affected = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for project in &mut self.projects {
            let affected = match affected_analysis(&mut project.graph, name, file, max_depth) {
                Ok(a) => a,
                Err(_) => continue,
            };

            for relation in affected {
                let key = format!("{}:{}:{}", project.name, relation.name, relation.file);
                if seen.insert(key) {
                    all_affected.push((project.name.clone(), relation));
                }
            }
        }

        all_affected
    }

    /// File context across all projects
    pub fn file_context(&mut self, file_path: &str) -> Vec<(String, FileContext)> {
        let mut results = Vec::new();
        for project in &mut self.projects {
            match get_file_context(&mut project.graph, file_path) {
                Ok(ctx) => results.push((project.name.clone(), ctx)),
                Err(_) => continue,
            }
        }
        results
    }
}

/// Textual relevance score for a query against a symbol name.
/// Returns 100 (exact), 75 (prefix), 50 (substring), or 0 (no match).
fn score_match(query: &str, name: &str) -> u32 {
    if name == query {
        100
    } else if name.starts_with(query) {
        75
    } else if name.contains(query) {
        50
    } else {
        0
    }
}

/// Extract project name from a database file path
fn project_name_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// Create a test DB with some indexed symbols and return its path
    fn create_test_db(dir: &std::path::Path, name: &str) -> PathBuf {
        let db_path = dir.join(format!("{}.db", name));
        let mut graph = CodeGraph::open(&db_path).unwrap();

        let source = r#"fn greet() { hello() }
fn hello() { println!("hi") }
struct AppConfig { name: String }
impl AppConfig { fn new() -> Self { Self { name: String::new() } } }"#;

        graph.index_file("src/lib.rs", source.as_bytes()).unwrap();
        db_path
    }

    /// Create a test DB with a different symbol set
    fn create_test_db_b(dir: &std::path::Path, name: &str) -> PathBuf {
        let db_path = dir.join(format!("{}.db", name));
        let mut graph = CodeGraph::open(&db_path).unwrap();

        let source = r#"fn process(data: &str) { parse(data) }
fn parse(data: &str) { data.len() }
fn greet() { println!("hello from project_b") }"#;

        graph.index_file("src/main.rs", source.as_bytes()).unwrap();
        db_path
    }

    // ── from_paths ──

    #[test]
    fn test_from_paths_opens_multiple_dbs() {
        let dir = tempdir().unwrap();
        let p1 = create_test_db(dir.path(), "project_a");
        let p2 = create_test_db_b(dir.path(), "project_b");

        let ctx = MultiDbContext::from_paths(&[p1, p2]).unwrap();
        assert_eq!(ctx.project_count(), 2);
        assert_eq!(ctx.project_names(), vec!["project_a", "project_b"]);
    }

    #[test]
    fn test_from_paths_skips_bad_db() {
        let dir = tempdir().unwrap();
        let good = create_test_db(dir.path(), "good_project");
        let bad = dir.path().join("not_a_db.txt");
        fs::write(&bad, "not a database").unwrap();

        let ctx = MultiDbContext::from_paths(&[good, bad]).unwrap();
        assert_eq!(ctx.project_count(), 1);
        assert_eq!(ctx.project_names(), vec!["good_project"]);
    }

    #[test]
    fn test_from_paths_empty_input() {
        let ctx = MultiDbContext::from_paths(&[]).unwrap();
        assert_eq!(ctx.project_count(), 0);
    }

    #[test]
    fn test_from_paths_all_bad() {
        let dir = tempdir().unwrap();
        let bad1 = dir.path().join("bad1.txt");
        let bad2 = dir.path().join("bad2.txt");
        fs::write(&bad1, "garbage").unwrap();
        fs::write(&bad2, "also garbage").unwrap();

        let ctx = MultiDbContext::from_paths(&[bad1, bad2]).unwrap();
        assert_eq!(ctx.project_count(), 0);
    }

    // ── from_directory ──

    #[test]
    fn test_from_directory_discovers_dbs() {
        let dir = tempdir().unwrap();
        create_test_db(dir.path(), "alpha");
        create_test_db_b(dir.path(), "beta");
        // Non-.db file should be ignored
        fs::write(dir.path().join("README.md"), "not a db").unwrap();

        let ctx = MultiDbContext::from_directory(dir.path()).unwrap();
        assert_eq!(ctx.project_count(), 2);
        // Sorted alphabetically: alpha < beta
        assert_eq!(ctx.project_names(), vec!["alpha", "beta"]);
    }

    #[test]
    fn test_from_directory_empty_directory() {
        let dir = tempdir().unwrap();
        let ctx = MultiDbContext::from_directory(dir.path()).unwrap();
        assert_eq!(ctx.project_count(), 0);
    }

    #[test]
    fn test_from_directory_missing_directory() {
        let result = MultiDbContext::from_directory(Path::new("/tmp/nonexistent_dir_hermes_test"));
        assert!(result.is_err());
    }

    // ── search_symbol ──

    #[test]
    fn test_search_symbol_cross_project() {
        let dir = tempdir().unwrap();
        let p1 = create_test_db(dir.path(), "proj_a");
        let p2 = create_test_db_b(dir.path(), "proj_b");

        let mut ctx = MultiDbContext::from_paths(&[p1, p2]).unwrap();
        // "greet" exists in both project_a and project_b
        let results = ctx.search_symbol("greet", None, None, false, false);
        assert_eq!(results.len(), 2);
        let projects: Vec<&str> = results.iter().map(|r| r.project.as_str()).collect();
        assert!(projects.contains(&"proj_a"));
        assert!(projects.contains(&"proj_b"));
    }

    #[test]
    fn test_search_symbol_single_project() {
        let dir = tempdir().unwrap();
        let p1 = create_test_db(dir.path(), "proj_a");
        let p2 = create_test_db_b(dir.path(), "proj_b");

        let mut ctx = MultiDbContext::from_paths(&[p1, p2]).unwrap();
        // "process" only exists in project_b
        let results = ctx.search_symbol("process", None, None, false, false);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project, "proj_b");
    }

    #[test]
    fn test_search_symbol_nonexistent() {
        let dir = tempdir().unwrap();
        let p1 = create_test_db(dir.path(), "proj_a");

        let mut ctx = MultiDbContext::from_paths(&[p1]).unwrap();
        let results = ctx.search_symbol("nonexistent_function_xyz", None, None, false, false);
        assert!(results.is_empty());
    }

    // ── score_match ──

    #[test]
    fn test_score_match_exact() {
        assert_eq!(score_match("parse_args", "parse_args"), 100);
        assert_eq!(score_match("greet", "greet"), 100);
    }

    #[test]
    fn test_score_match_prefix() {
        assert_eq!(score_match("parse", "parse_args"), 75);
        assert_eq!(score_match("greet", "greet_user"), 75);
    }

    #[test]
    fn test_score_match_substring() {
        assert_eq!(score_match("args", "parse_args"), 50);
        assert_eq!(score_match("user", "greet_user_admin"), 50);
    }

    #[test]
    fn test_score_match_no_match() {
        assert_eq!(score_match("xyz", "parse_args"), 0);
    }

    #[test]
    fn test_search_symbol_sorted_by_caller_count() {
        let dir = tempdir().unwrap();

        // proj_a: "process" is called once (by greet)
        let pa = dir.path().join("pa.db");
        {
            let mut g = CodeGraph::open(&pa).unwrap();
            g.index_file("src/a.rs", b"fn greet() { process() } fn process() {}")
                .unwrap();
        }

        // proj_b: "process" is called twice (by greet and parse)
        let pb = dir.path().join("pb.db");
        {
            let mut g = CodeGraph::open(&pb).unwrap();
            g.index_file(
                "src/b.rs",
                b"fn greet() { process() } fn parse() { process() } fn process() {}",
            )
            .unwrap();
        }

        let mut ctx = MultiDbContext::from_paths(&[pa, pb]).unwrap();
        let results = ctx.search_symbol("process", None, None, true, false);

        assert_eq!(results.len(), 2);
        let caller_counts: Vec<usize> = results
            .iter()
            .map(|r| r.callers.as_ref().map(|c| c.len()).unwrap_or(0))
            .collect();
        // Should be sorted descending: pb (2 callers) before pa (1 caller)
        assert!(
            caller_counts[0] >= caller_counts[1],
            "expected callers sorted descending, got {:?}",
            caller_counts
        );
    }

    // ── list_symbols ──

    #[test]
    fn test_list_symbols_deduplication() {
        let dir = tempdir().unwrap();
        let p1 = create_test_db(dir.path(), "proj_a");
        let p2 = create_test_db_b(dir.path(), "proj_b");

        let mut ctx = MultiDbContext::from_paths(&[p1, p2]).unwrap();
        let query = ListQuery {
            kind: Some("fn".to_string()),
            file_pattern: None,
            page: Some(1),
            page_size: Some(100),
            cursor: None,
        };
        let items = ctx.list_symbols(&query);

        // "greet" appears in both projects but should appear twice
        // (different projects, different files — dedup key includes project+file+line)
        let greets: Vec<&str> = items
            .iter()
            .filter(|(_, i)| i.name == "greet")
            .map(|(p, _)| p.as_str())
            .collect();
        assert_eq!(greets.len(), 2);
    }

    // ── impact ──

    #[test]
    fn test_impact_stays_within_project() {
        let dir = tempdir().unwrap();
        let p1 = create_test_db(dir.path(), "proj_a");
        let p2 = create_test_db_b(dir.path(), "proj_b");

        let mut ctx = MultiDbContext::from_paths(&[p1, p2]).unwrap();
        // "hello" is only called by "greet" in proj_a
        let impacted = ctx.impact("hello", None, 3);
        // All results should be from proj_a only
        for (project, _) in &impacted {
            assert_eq!(project, "proj_a");
        }
    }

    // ── affected ──

    #[test]
    fn test_affected_stays_within_project() {
        let dir = tempdir().unwrap();
        let p1 = create_test_db(dir.path(), "proj_a");
        let p2 = create_test_db_b(dir.path(), "proj_b");

        let mut ctx = MultiDbContext::from_paths(&[p1, p2]).unwrap();
        // "greet" in proj_a calls "hello" — affected should only return proj_a symbols
        let affected = ctx.affected("greet", None, 3);
        for (project, _) in &affected {
            assert_eq!(project, "proj_a");
        }
    }

    // ── summaries ──

    #[test]
    fn test_summaries_returns_all_projects() {
        let dir = tempdir().unwrap();
        let p1 = create_test_db(dir.path(), "alpha");
        let p2 = create_test_db_b(dir.path(), "beta");

        let mut ctx = MultiDbContext::from_paths(&[p1, p2]).unwrap();
        let summaries = ctx.summaries();
        assert_eq!(summaries.len(), 2);
        let names: Vec<&str> = summaries.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    // ── project_name_from_path ──

    #[test]
    fn test_project_name_extraction() {
        assert_eq!(
            project_name_from_path(Path::new("/some/path/magellan.db")),
            "magellan"
        );
        assert_eq!(project_name_from_path(Path::new("splice.db")), "splice");
        // No extension but has a stem — returns stem
        assert_eq!(project_name_from_path(Path::new("/noext")), "noext");
    }
}
