use base64::Engine;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;

/// Project-level summary (~50 tokens)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    /// Project name (from Cargo.toml or directory)
    pub name: String,
    /// Project version
    pub version: String,
    /// Primary language
    pub language: String,
    /// Total files indexed
    pub total_files: usize,
    /// Total symbols indexed
    pub total_symbols: usize,
    /// Symbol breakdown by kind
    pub symbol_counts: SymbolCounts,
    /// Entry points (main functions, etc.)
    pub entry_points: Vec<String>,
    /// Brief description
    pub description: String,
}

/// Symbol counts by kind
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SymbolCounts {
    pub functions: usize,
    pub methods: usize,
    pub structs: usize,
    pub traits: usize,
    pub enums: usize,
    pub modules: usize,
    pub other: usize,
}

/// File-level context (~100 tokens)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContext {
    /// File path
    pub path: String,
    /// Language
    pub language: String,
    /// Total symbols in file
    pub symbol_count: usize,
    /// Public symbols
    pub public_symbols: Vec<String>,
    /// Symbol breakdown
    pub symbol_counts: SymbolCounts,
    /// Dependencies (imports)
    pub imports: Vec<String>,
}

/// Symbol detail (~150-500 tokens)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDetail {
    /// Symbol name
    pub name: String,
    /// Symbol kind (fn, struct, etc.)
    pub kind: String,
    /// File containing this symbol
    pub file: String,
    /// Line number
    pub line: usize,
    /// Signature (if available)
    pub signature: Option<String>,
    /// Documentation (if available)
    pub documentation: Option<String>,
    /// Caller symbols
    pub callers: Vec<String>,
    /// Callee symbols
    pub callees: Vec<String>,
    /// Related symbols (same module, etc.)
    pub related: Vec<String>,
}

/// Paginated result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    /// Current page (1-indexed)
    pub page: usize,
    /// Total pages available
    pub total_pages: usize,
    /// Items per page
    pub page_size: usize,
    /// Total items across all pages
    pub total_items: usize,
    /// Cursor for next page (base64 encoded)
    pub next_cursor: Option<String>,
    /// Cursor for previous page
    pub prev_cursor: Option<String>,
    /// Items on this page
    pub items: Vec<T>,
}

impl<T> PaginatedResult<T> {
    /// Create a new paginated result
    pub fn new(items: Vec<T>, page: usize, page_size: usize, total_items: usize) -> Self {
        let total_pages = (total_items + page_size - 1) / page_size;
        let next_cursor = if page < total_pages {
            Some(base64::engine::general_purpose::STANDARD.encode(format!("page={}", page + 1)))
        } else {
            None
        };
        let prev_cursor = if page > 1 {
            Some(base64::engine::general_purpose::STANDARD.encode(format!("page={}", page - 1)))
        } else {
            None
        };

        Self {
            page,
            total_pages,
            page_size,
            total_items,
            next_cursor,
            prev_cursor,
            items,
        }
    }

    /// Create empty result
    pub fn empty(page: usize, page_size: usize) -> Self {
        Self::new(Vec::new(), page, page_size, 0)
    }
}

/// Query for listing symbols
#[derive(Debug, Clone, Deserialize)]
pub struct ListQuery {
    /// Filter by symbol kind (fn, struct, etc.)
    pub kind: Option<String>,
    /// Filter by file path pattern
    pub file_pattern: Option<String>,
    /// Page number (1-indexed)
    pub page: Option<usize>,
    /// Page size (default: 50)
    pub page_size: Option<usize>,
    /// Cursor for pagination (overrides page)
    pub cursor: Option<String>,
}

impl Default for ListQuery {
    fn default() -> Self {
        Self {
            kind: None,
            file_pattern: None,
            page: Some(1),
            page_size: Some(50),
            cursor: None,
        }
    }
}

/// Get project summary
pub fn get_project_summary(graph: &mut CodeGraph) -> Result<ProjectSummary> {
    let total_files = graph.count_files()?;
    let total_symbols = graph.count_symbols()?;

    // Get symbol counts by label
    let mut counts = SymbolCounts::default();
    
    for label in &["fn", "method", "struct", "trait", "enum", "mod"] {
        let symbols = graph.get_symbols_by_label(label)?;
        let count = symbols.len();
        
        match *label {
            "fn" => counts.functions = count,
            "method" => counts.methods = count,
            "struct" => counts.structs = count,
            "trait" => counts.traits = count,
            "enum" => counts.enums = count,
            "mod" => counts.modules = count,
            _ => counts.other += count,
        }
    }

    // Detect project info from Cargo.toml if available
    let (name, version) = detect_project_info()?;
    
    // Detect primary language
    let language = detect_primary_language(graph)?;

    // Find entry points
    let entry_points = find_entry_points(graph)?;

    // Generate description
    let description = format!(
        "{} {} written in {}, {} files, {} symbols ({} functions, {} structs)",
        name, version, language, total_files, total_symbols, counts.functions, counts.structs
    );

    Ok(ProjectSummary {
        name,
        version,
        language,
        total_files,
        total_symbols,
        symbol_counts: counts,
        entry_points,
        description,
    })
}

/// Get file context
pub fn get_file_context(graph: &mut CodeGraph, file_path: &str) -> Result<FileContext> {
    let symbols = graph.symbols_in_file(file_path)?;
    
    let mut counts = SymbolCounts::default();
    let mut public_symbols = Vec::new();

    for symbol in &symbols {
        let kind = symbol.kind_normalized.as_str();

        match kind {
            "fn" => counts.functions += 1,
            "method" => counts.methods += 1,
            "struct" => counts.structs += 1,
            "trait" => counts.traits += 1,
            "enum" => counts.enums += 1,
            "mod" => counts.modules += 1,
            _ => counts.other += 1,
        }

        // Check if symbol is public (simple heuristic: not starting with _)
        if let Some(ref name) = symbol.name {
            if !name.starts_with('_') {
                public_symbols.push(format!("{}:{}", kind, name));
            }
        }
    }

    // Detect language
    let language = crate::common::detect_language_from_path(file_path);

    // Imports not yet implemented - would require additional graph queries
    let imports = Vec::new();

    Ok(FileContext {
        path: file_path.to_string(),
        language,
        symbol_count: symbols.len(),
        public_symbols,
        symbol_counts: counts,
        imports,
    })
}

/// Get symbol detail
pub fn get_symbol_detail(
    graph: &mut CodeGraph,
    symbol_name: &str,
    file_path: Option<&str>,
) -> Result<SymbolDetail> {
    // Find the symbol
    let symbols = if let Some(file) = file_path {
        graph.symbols_in_file(file)?
            .into_iter()
            .filter(|s| s.name.as_deref() == Some(symbol_name))
            .collect::<Vec<_>>()
    } else {
        // Search across all files
        let results = graph.get_symbols_by_label(symbol_name)?;
        results.into_iter()
            .filter_map(|r| {
                graph.symbols_in_file(&r.file_path).ok()
                    .and_then(|syms| syms.into_iter().find(|s| s.name.as_deref() == Some(symbol_name)))
            })
            .collect::<Vec<_>>()
    };

    let symbol = symbols.first()
        .ok_or_else(|| anyhow::anyhow!("Symbol '{}' not found", symbol_name))?;

    // Get callers
    let callers = graph.callers_of_symbol(&symbol.file_path.to_string_lossy(), symbol_name)?
        .iter()
        .map(|c| c.caller.clone())
        .collect();

    // Get callees
    let callees = graph.calls_from_symbol(&symbol.file_path.to_string_lossy(), symbol_name)?
        .iter()
        .map(|c| c.callee.clone())
        .collect();

    // Get related symbols (same module)
    let related = graph.symbols_in_file(&symbol.file_path.to_string_lossy())?
        .iter()
        .filter(|s| s.name.as_deref() != Some(symbol_name))
        .filter_map(|s| s.name.clone())
        .take(10)
        .collect();

    Ok(SymbolDetail {
        name: symbol_name.to_string(),
        kind: symbol.kind_normalized.clone(),
        file: symbol.file_path.to_string_lossy().to_string(),
        line: symbol.start_line,
        signature: None, // Would come from LSP enrichment
        documentation: None, // Would come from LSP enrichment
        callers,
        callees,
        related,
    })
}

/// List symbols with pagination
pub fn list_symbols(
    graph: &mut CodeGraph,
    query: &ListQuery,
) -> Result<PaginatedResult<SymbolListItem>> {
    let page = query.cursor
        .as_ref()
        .and_then(|c| base64::engine::general_purpose::STANDARD.decode(c).ok())
        .and_then(|d| String::from_utf8(d).ok())
        .and_then(|s| s.strip_prefix("page=").map(|p| p.parse::<usize>().ok()).flatten())
        .unwrap_or(query.page.unwrap_or(1));
    
    let page_size = query.page_size.unwrap_or(50);

    // Get all symbols or filter by kind
    let all_symbols = if let Some(ref kind) = query.kind {
        graph.get_symbols_by_label(kind)?
            .into_iter()
            .map(|r| SymbolListItem {
                name: r.name,
                kind: kind.clone(),
                file: r.file_path,
                line: 0, // SymbolQueryResult doesn't have line info
            })
            .collect::<Vec<_>>()
    } else {
        // Get symbols from all files
        let files = graph.all_file_nodes()?;
        let mut items = Vec::new();
        
        for (file_path, _) in files {
            if let Ok(symbols) = graph.symbols_in_file(&file_path) {
                for symbol in symbols {
                    if let Some(ref name) = symbol.name {
                        items.push(SymbolListItem {
                            name: name.clone(),
                            kind: symbol.kind_normalized.clone(),
                            file: file_path.clone(),
                            line: symbol.start_line,
                        });
                    }
                }
            }
        }
        items
    };

    let total_items = all_symbols.len();
    let start = (page - 1) * page_size;
    let end = std::cmp::min(start + page_size, total_items);

    let items = if start < total_items {
        all_symbols[start..end].to_vec()
    } else {
        Vec::new()
    };

    Ok(PaginatedResult::new(items, page, page_size, total_items))
}

/// Symbol list item for pagination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolListItem {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
}

// Helper functions

fn detect_project_info() -> Result<(String, String)> {
    // Try to find Cargo.toml
    let cargo_toml = std::path::Path::new("Cargo.toml");
    if cargo_toml.exists() {
        if let Ok(content) = std::fs::read_to_string(cargo_toml) {
            let name = content
                .lines()
                .find(|l| l.starts_with("name = "))
                .and_then(|l| l.split('"').nth(1))
                .unwrap_or("unknown")
                .to_string();
            
            let version = content
                .lines()
                .find(|l| l.starts_with("version = "))
                .and_then(|l| l.split('"').nth(1))
                .unwrap_or("0.1.0")
                .to_string();
            
            return Ok((name, version));
        }
    }
    
    // Fallback to directory name
    let dir_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    
    Ok((dir_name, "0.1.0".to_string()))
}

fn detect_primary_language(graph: &mut CodeGraph) -> Result<String> {
    // Count files by extension
    let files = graph.all_file_nodes()?;
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    
    for (path, _) in files {
        if let Some(ext) = std::path::Path::new(&path).extension()
            .and_then(|e| e.to_str())
        {
            *counts.entry(ext.to_string()).or_insert(0) += 1;
        }
    }
    
    // Find most common extension
    let primary = counts.iter()
        .max_by_key(|(_, &count)| count)
        .map(|(ext, _)| ext.as_str())
        .unwrap_or("unknown");
    
    let language = match primary {
        "rs" => "Rust",
        "py" => "Python",
        "c" | "h" => "C",
        "cpp" | "hpp" | "cc" => "C++",
        "java" => "Java",
        "js" | "mjs" => "JavaScript",
        "ts" | "tsx" => "TypeScript",
        _ => "Unknown",
    };
    
    Ok(language.to_string())
}

fn find_entry_points(graph: &mut CodeGraph) -> Result<Vec<String>> {
    let mut entry_points = Vec::new();
    
    // Look for main functions
    if let Ok(mains) = graph.get_symbols_by_label("main") {
        for m in mains {
            entry_points.push(format!("{} ({})", m.name, m.file_path));
        }
    }
    
    // Look for lib.rs
    if let Ok(libs) = graph.get_symbols_by_label("lib") {
        for l in libs {
            entry_points.push(format!("{} ({})", l.name, l.file_path));
        }
    }
    
    Ok(entry_points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paginated_result_creation() {
        let items: Vec<i32> = (1..101).collect();
        let result = PaginatedResult::new(items, 1, 50, 100);
        
        assert_eq!(result.page, 1);
        assert_eq!(result.total_pages, 2);
        assert_eq!(result.page_size, 50);
        assert_eq!(result.total_items, 100);
        assert!(result.next_cursor.is_some());
        assert!(result.prev_cursor.is_none());
        assert_eq!(result.items.len(), 50);
    }

    #[test]
    fn test_list_query_default() {
        let query = ListQuery::default();
        assert_eq!(query.page, Some(1));
        assert_eq!(query.page_size, Some(50));
        assert!(query.kind.is_none());
    }
}
