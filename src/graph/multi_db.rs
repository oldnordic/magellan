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

/// Extract project name from a database file path
fn project_name_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
