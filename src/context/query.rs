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
    /// Line number (1-indexed)
    pub line: usize,
    /// Byte offset where symbol starts in file
    #[serde(default)]
    pub byte_start: usize,
    /// Byte offset where symbol ends in file
    #[serde(default)]
    pub byte_end: usize,
    /// Column where symbol starts (0-indexed, bytes)
    #[serde(default)]
    pub start_col: usize,
    /// Line where symbol ends (1-indexed)
    #[serde(default)]
    pub end_line: usize,
    /// Column where symbol ends (0-indexed, bytes)
    #[serde(default)]
    pub end_col: usize,
    /// Signature (if available)
    pub signature: Option<String>,
    /// Documentation (if available)
    pub documentation: Option<String>,
    /// Caller symbols with metadata
    pub callers: Vec<SymbolRelation>,
    /// Callee symbols with metadata
    pub callees: Vec<SymbolRelation>,
    /// Related symbols (same module, etc.)
    pub related: Vec<String>,
}

/// A caller or callee relation with file/line metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRelation {
    /// Symbol name
    pub name: String,
    /// File path containing the symbol
    pub file: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Hop depth for recursive traversal (None = direct, Some(N) = N hops away)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
}

/// Blast score result for impact analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastScore {
    /// Computed blast score (direct + 0.5 * transitive)
    pub score: f64,
    /// Number of direct dependents (depth = 1)
    pub direct_count: usize,
    /// Number of transitive dependents (depth > 1)
    pub transitive_count: usize,
    /// Risk level classification
    pub risk_level: String,
    /// Percentage of total codebase affected
    pub risk_percent: f64,
    /// Total number of impacted symbols
    pub total_impacted: usize,
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
        let total_pages = total_items.div_ceil(page_size);
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

        // Slice items to only return the requested page
        let start_idx = (page.saturating_sub(1)) * page_size;
        let end_idx = (start_idx + page_size).min(items.len());
        let paged_items = if start_idx < items.len() {
            items
                .into_iter()
                .skip(start_idx)
                .take(end_idx - start_idx)
                .collect()
        } else {
            Vec::new()
        };

        Self {
            page,
            total_pages,
            page_size,
            total_items,
            next_cursor,
            prev_cursor,
            items: paged_items,
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
        graph
            .symbols_in_file(file)?
            .into_iter()
            .filter(|s| s.name.as_deref() == Some(symbol_name))
            .collect::<Vec<_>>()
    } else {
        // Search across all files using FTS5 symbol name index
        let results = graph.search_symbols_by_name(symbol_name)?;
        results
            .into_iter()
            .filter_map(|r| {
                graph.symbols_in_file(&r.file_path).ok().and_then(|syms| {
                    syms.into_iter()
                        .find(|s| s.name.as_deref() == Some(symbol_name))
                })
            })
            .collect::<Vec<_>>()
    };

    let symbol = symbols
        .first()
        .ok_or_else(|| anyhow::anyhow!("Symbol '{}' not found", symbol_name))?;

    // Get callers with file/line metadata
    let callers = graph
        .callers_of_symbol(&symbol.file_path.to_string_lossy(), symbol_name)?
        .into_iter()
        .map(|c| SymbolRelation {
            name: c.caller,
            file: c.file_path.to_string_lossy().to_string(),
            line: c.start_line,
            depth: None,
        })
        .collect();

    // Get callees with file/line metadata
    let callees = graph
        .calls_from_symbol(&symbol.file_path.to_string_lossy(), symbol_name)?
        .into_iter()
        .map(|c| SymbolRelation {
            name: c.callee,
            file: c.file_path.to_string_lossy().to_string(),
            line: c.start_line,
            depth: None,
        })
        .collect();

    // Get related symbols (same module)
    let related = graph
        .symbols_in_file(&symbol.file_path.to_string_lossy())?
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
        byte_start: symbol.byte_start,
        byte_end: symbol.byte_end,
        start_col: symbol.start_col,
        end_line: symbol.end_line,
        end_col: symbol.end_col,
        signature: None,     // Would come from LSP enrichment
        documentation: None, // Would come from LSP enrichment
        callers,
        callees,
        related,
    })
}

/// Get symbol detail with recursive caller/callee traversal to a given depth.
///
/// At depth=1, behaves like `get_symbol_detail` (single hop).
/// At depth=2, also includes callers-of-callers and callees-of-callees, etc.
///
/// The returned `SymbolDetail` has the same structure but callers/callees
/// arrays are flattened from all hops. Each `SymbolRelation` carries the
/// hop depth via its `line` field being 0 (meaning "indirect" — we repurpose
/// it since the call graph doesn't store line for cross-file refs).
///
/// Actually, we add a new field `depth` to `SymbolRelation` for this.
pub fn get_symbol_detail_recursive(
    graph: &mut CodeGraph,
    symbol_name: &str,
    file_path: Option<&str>,
    max_depth: usize,
) -> Result<SymbolDetail> {
    // First get the base symbol detail
    let mut detail = get_symbol_detail(graph, symbol_name, file_path)?;

    if max_depth <= 1 {
        return Ok(detail);
    }

    // BFS: (symbol_name, file_path, current_depth)
    let mut caller_queue: Vec<(String, String, usize)> = Vec::new();
    let mut callee_queue: Vec<(String, String, usize)> = Vec::new();

    // Seed queues with direct callers/callees
    for c in &detail.callers {
        caller_queue.push((c.name.clone(), c.file.clone(), 1));
    }
    for c in &detail.callees {
        callee_queue.push((c.name.clone(), c.file.clone(), 1));
    }

    let mut visited_callers: std::collections::HashSet<(String, String)> = detail
        .callers
        .iter()
        .map(|c| (c.name.clone(), c.file.clone()))
        .collect();
    let mut visited_callees: std::collections::HashSet<(String, String)> = detail
        .callees
        .iter()
        .map(|c| (c.name.clone(), c.file.clone()))
        .collect();

    // BFS for callers
    while let Some((name, _file, depth)) = caller_queue.pop() {
        if depth >= max_depth {
            continue;
        }
        // Use None for file_path: c.file is the call-site, not the definition.
        // Searching globally by name finds the actual definition.
        if let Ok(caller_detail) = get_symbol_detail(graph, &name, None) {
            for c in &caller_detail.callers {
                let key = (c.name.clone(), c.file.clone());
                if visited_callers.insert(key.clone()) {
                    detail.callers.push(SymbolRelation {
                        name: c.name.clone(),
                        file: c.file.clone(),
                        line: 0, // indirect — line not meaningful for BFS
                        depth: Some(depth + 1),
                    });
                    caller_queue.push((c.name.clone(), c.file.clone(), depth + 1));
                }
            }
        }
    }

    // BFS for callees
    while let Some((name, _file, depth)) = callee_queue.pop() {
        if depth >= max_depth {
            continue;
        }
        // Use None for file_path: c.file is the call-site, not the definition.
        // Searching globally by name finds the actual definition.
        if let Ok(callee_detail) = get_symbol_detail(graph, &name, None) {
            for c in &callee_detail.callees {
                let key = (c.name.clone(), c.file.clone());
                if visited_callees.insert(key.clone()) {
                    detail.callees.push(SymbolRelation {
                        name: c.name.clone(),
                        file: c.file.clone(),
                        line: 0,
                        depth: Some(depth + 1),
                    });
                    callee_queue.push((c.name.clone(), c.file.clone(), depth + 1));
                }
            }
        }
    }

    Ok(detail)
}

/// Get symbols that call the given symbol
pub fn get_callers(
    graph: &mut CodeGraph,
    symbol_name: &str,
    file_path: Option<&str>,
) -> Result<Vec<SymbolListItem>> {
    let symbols = if let Some(file) = file_path {
        graph
            .symbols_in_file(file)?
            .into_iter()
            .filter(|s| s.name.as_deref() == Some(symbol_name))
            .collect::<Vec<_>>()
    } else {
        let results = graph.search_symbols_by_name(symbol_name)?;
        results
            .into_iter()
            .filter_map(|r| {
                graph.symbols_in_file(&r.file_path).ok().and_then(|syms| {
                    syms.into_iter()
                        .find(|s| s.name.as_deref() == Some(symbol_name))
                })
            })
            .collect::<Vec<_>>()
    };

    let symbol = symbols
        .first()
        .ok_or_else(|| anyhow::anyhow!("Symbol '{}' not found", symbol_name))?;

    let callers = graph.callers_of_symbol(&symbol.file_path.to_string_lossy(), symbol_name)?;
    let items = callers
        .into_iter()
        .map(|c| SymbolListItem {
            name: c.caller,
            kind: "function".to_string(),
            file: c.file_path.to_string_lossy().to_string(),
            line: c.start_line,
        })
        .collect();

    Ok(items)
}

/// Get symbols called by the given symbol
pub fn get_callees(
    graph: &mut CodeGraph,
    symbol_name: &str,
    file_path: Option<&str>,
) -> Result<Vec<SymbolListItem>> {
    let symbols = if let Some(file) = file_path {
        graph
            .symbols_in_file(file)?
            .into_iter()
            .filter(|s| s.name.as_deref() == Some(symbol_name))
            .collect::<Vec<_>>()
    } else {
        let results = graph.search_symbols_by_name(symbol_name)?;
        results
            .into_iter()
            .filter_map(|r| {
                graph.symbols_in_file(&r.file_path).ok().and_then(|syms| {
                    syms.into_iter()
                        .find(|s| s.name.as_deref() == Some(symbol_name))
                })
            })
            .collect::<Vec<_>>()
    };

    let symbol = symbols
        .first()
        .ok_or_else(|| anyhow::anyhow!("Symbol '{}' not found", symbol_name))?;

    let callees = graph.calls_from_symbol(&symbol.file_path.to_string_lossy(), symbol_name)?;
    let items = callees
        .into_iter()
        .map(|c| SymbolListItem {
            name: c.callee,
            kind: "function".to_string(),
            file: c.file_path.to_string_lossy().to_string(),
            line: c.start_line,
        })
        .collect();

    Ok(items)
}

/// List symbols with pagination
pub fn list_symbols(
    graph: &mut CodeGraph,
    query: &ListQuery,
) -> Result<PaginatedResult<SymbolListItem>> {
    let page = query
        .cursor
        .as_ref()
        .and_then(|c| base64::engine::general_purpose::STANDARD.decode(c).ok())
        .and_then(|d| String::from_utf8(d).ok())
        .and_then(|s| {
            s.strip_prefix("page=")
                .and_then(|p| p.parse::<usize>().ok())
        })
        .unwrap_or(query.page.unwrap_or(1));

    let page_size = query.page_size.unwrap_or(50);

    // Get all symbols or filter by kind
    let all_symbols = if let Some(ref kind) = query.kind {
        graph
            .get_symbols_by_label(kind)?
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

    Ok(PaginatedResult::new(
        all_symbols,
        page,
        page_size,
        total_items,
    ))
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
        if let Some(ext) = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
        {
            *counts.entry(ext.to_string()).or_insert(0) += 1;
        }
    }

    // Find most common extension
    let primary = counts
        .iter()
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

/// Impact analysis: find all symbols that (transitively) call the given symbol
/// Returns a list of (name, file, line, depth) tuples representing the blast radius
pub fn impact_analysis(
    graph: &mut CodeGraph,
    symbol_name: &str,
    file_path: Option<&str>,
    max_depth: usize,
) -> Result<Vec<SymbolRelation>> {
    let detail = get_symbol_detail(graph, symbol_name, file_path)?;

    let mut impacted: Vec<SymbolRelation> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();

    // BFS over callers
    let mut queue: std::collections::VecDeque<(String, usize)> = std::collections::VecDeque::new();
    for c in &detail.callers {
        let key = format!("{}:{}", c.name, c.file);
        if visited.insert(key) {
            impacted.push(SymbolRelation {
                name: c.name.clone(),
                file: c.file.clone(),
                line: c.line,
                depth: Some(1),
            });
            queue.push_back((c.name.clone(), 1));
        }
    }

    while let Some((name, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        // Look up this caller's own callers
        match get_symbol_detail(graph, &name, None) {
            Ok(caller_detail) => {
                for c in &caller_detail.callers {
                    let key = format!("{}:{}", c.name, c.file);
                    if visited.insert(key) {
                        impacted.push(SymbolRelation {
                            name: c.name.clone(),
                            file: c.file.clone(),
                            line: c.line,
                            depth: Some(depth + 1),
                        });
                        queue.push_back((c.name.clone(), depth + 1));
                    }
                }
            }
            Err(_) => continue,
        }
    }

    // Sort by depth then name for deterministic output
    impacted.sort_by(|a, b| {
        a.depth
            .unwrap_or(0)
            .cmp(&b.depth.unwrap_or(0))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(impacted)
}

/// Affected analysis: find all symbols that the given symbol (transitively) calls
/// Returns a list of (name, file, line, depth) tuples representing the dependency reach
pub fn affected_analysis(
    graph: &mut CodeGraph,
    symbol_name: &str,
    file_path: Option<&str>,
    max_depth: usize,
) -> Result<Vec<SymbolRelation>> {
    let detail = get_symbol_detail(graph, symbol_name, file_path)?;

    let mut affected: Vec<SymbolRelation> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();

    // BFS over callees
    let mut queue: std::collections::VecDeque<(String, usize)> = std::collections::VecDeque::new();
    for c in &detail.callees {
        let key = format!("{}:{}", c.name, c.file);
        if visited.insert(key) {
            affected.push(SymbolRelation {
                name: c.name.clone(),
                file: c.file.clone(),
                line: c.line,
                depth: Some(1),
            });
            queue.push_back((c.name.clone(), 1));
        }
    }

    while let Some((name, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        match get_symbol_detail(graph, &name, None) {
            Ok(callee_detail) => {
                for c in &callee_detail.callees {
                    let key = format!("{}:{}", c.name, c.file);
                    if visited.insert(key) {
                        affected.push(SymbolRelation {
                            name: c.name.clone(),
                            file: c.file.clone(),
                            line: c.line,
                            depth: Some(depth + 1),
                        });
                        queue.push_back((c.name.clone(), depth + 1));
                    }
                }
            }
            Err(_) => continue,
        }
    }

    affected.sort_by(|a, b| {
        a.depth
            .unwrap_or(0)
            .cmp(&b.depth.unwrap_or(0))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(affected)
}

/// Compute blast score from impacted symbols
/// Returns single score metric (direct + 0.5 * transitive) with risk classification
pub fn compute_blast_score(
    graph: &mut CodeGraph,
    symbol_name: &str,
    file_path: Option<&str>,
    max_depth: usize,
) -> Result<BlastScore> {
    let impacted = impact_analysis(graph, symbol_name, file_path, max_depth)?;

    let direct_count = impacted.iter().filter(|r| r.depth == Some(1)).count();

    let transitive_count = impacted
        .iter()
        .filter(|r| r.depth.map(|d| d > 1).unwrap_or(false))
        .count();

    // codeindex formula: direct + 0.5 * transitive
    let score = (direct_count as f64) + (0.5 * transitive_count as f64);

    // Get total file count for risk percentage
    let total_files = match graph.count_files() {
        Ok(count) => count as f64,
        Err(_) => 1.0, // Avoid division by zero
    };

    // Calculate unique affected files for risk percentage
    let unique_files: std::collections::HashSet<_> =
        impacted.iter().map(|r| r.file.as_str()).collect();

    let risk_percent = if total_files > 0.0 {
        (unique_files.len() as f64 / total_files) * 100.0
    } else {
        0.0
    };

    let risk_level = match risk_percent {
        p if p >= 20.0 => "HIGH",
        p if p >= 10.0 => "MEDIUM",
        _ => "LOW",
    };

    Ok(BlastScore {
        score,
        direct_count,
        transitive_count,
        risk_level: risk_level.to_string(),
        risk_percent,
        total_impacted: impacted.len(),
    })
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
