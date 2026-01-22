//! Find command implementation
//!
//! Finds a symbol by name, optionally limited to a specific file.

use anyhow::{Context, Result};
use globset::GlobBuilder;
use magellan::{CodeGraph, SymbolKind};
use magellan::graph::query;
use magellan::output::{JsonResponse, OutputFormat, FindResponse, Span, SymbolMatch, output_json};
use magellan::output::rich::{SpanContext, SpanChecksums};
use std::path::PathBuf;

/// Represents a found symbol with its file and node ID
struct FoundSymbol {
    name: String,
    kind: SymbolKind,
    kind_normalized: String,
    file: String,
    byte_start: usize,
    byte_end: usize,
    line: usize,
    col: usize,
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
    node_id: i64,
    symbol_id: Option<String>,
}

/// Format a SymbolKind for display
fn format_symbol_kind(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "Function",
        SymbolKind::Method => "Method",
        SymbolKind::Class => "Class",
        SymbolKind::Interface => "Interface",
        SymbolKind::Enum => "Enum",
        SymbolKind::Module => "Module",
        SymbolKind::Union => "Union",
        SymbolKind::Namespace => "Namespace",
        SymbolKind::TypeAlias => "TypeAlias",
        SymbolKind::Unknown => "Unknown",
    }
}

/// Resolve a file path against an optional root directory
fn resolve_path(file_path: &PathBuf, root: &Option<PathBuf>) -> String {
    if file_path.is_absolute() {
        return file_path.to_string_lossy().to_string();
    }

    if let Some(ref root) = root {
        root.join(file_path).to_string_lossy().to_string()
    } else {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| cwd.join(file_path).canonicalize().ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string_lossy().to_string())
    }
}

/// Find a symbol in a specific file by name
///
/// Returns the first matching symbol with its node ID and symbol_id
fn find_in_file(graph: &mut CodeGraph, file_path: &str, name: &str) -> Result<Option<FoundSymbol>> {
    let entries = query::symbol_nodes_in_file_with_ids(graph, file_path)?;

    for (node_id, symbol, symbol_id) in entries {
        if let Some(symbol_name) = &symbol.name {
            if symbol_name == name {
                return Ok(Some(FoundSymbol {
                    name: symbol_name.clone(),
                    kind: symbol.kind,
                    kind_normalized: symbol.kind_normalized.clone(),
                    file: symbol.file_path.to_string_lossy().to_string(),
                    byte_start: symbol.byte_start,
                    byte_end: symbol.byte_end,
                    line: symbol.start_line,
                    col: symbol.start_col,
                    start_line: symbol.start_line,
                    start_col: symbol.start_col,
                    end_line: symbol.end_line,
                    end_col: symbol.end_col,
                    node_id,
                    symbol_id,
                }));
            }
        }
    }

    Ok(None)
}

/// Find a symbol across all files by name
///
/// Returns all matching symbols
fn find_all_files(graph: &mut CodeGraph, name: &str) -> Result<Vec<FoundSymbol>> {
    let mut results = Vec::new();

    // Get all indexed files
    let file_nodes = graph.all_file_nodes()?;

    // Search each file for the symbol
    for file_path in file_nodes.keys() {
        let entries = query::symbol_nodes_in_file_with_ids(graph, file_path)?;
        for (node_id, symbol, symbol_id) in entries {
            if let Some(symbol_name) = &symbol.name {
                if symbol_name == name {
                    results.push(FoundSymbol {
                        name: symbol_name.clone(),
                        kind: symbol.kind.clone(),
                        kind_normalized: symbol.kind_normalized.clone(),
                        file: symbol.file_path.to_string_lossy().to_string(),
                        byte_start: symbol.byte_start,
                        byte_end: symbol.byte_end,
                        line: symbol.start_line,
                        col: symbol.start_col,
                        start_line: symbol.start_line,
                        start_col: symbol.start_col,
                        end_line: symbol.end_line,
                        end_col: symbol.end_col,
                        node_id,
                        symbol_id,
                    });
                    break; // Found in this file, move to next
                }
            }
        }
    }

    Ok(results)
}

/// Run the find command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `name` - Symbol name to find
/// * `root` - Optional root directory for resolving relative paths
/// * `path` - Optional file path to limit search
/// * `glob_pattern` - Optional glob pattern for listing symbols
/// * `output_format` - Output format (Human or Json)
///
/// # Displays
/// Human-readable symbol details or JSON output
pub fn run_find(
    db_path: PathBuf,
    name: Option<String>,
    root: Option<PathBuf>,
    path: Option<PathBuf>,
    glob_pattern: Option<String>,
    output_format: OutputFormat,
    with_context: bool,
    with_callers: bool,
    with_callees: bool,
    with_semantics: bool,
    with_checksums: bool,
    context_lines: usize,
) -> Result<()> {
    // Build args for execution tracking
    let mut args = vec!["find".to_string()];
    if let Some(ref n) = name {
        args.push("--name".to_string());
        args.push(n.clone());
    }
    if let Some(ref root_path) = root {
        args.push("--root".to_string());
        args.push(root_path.to_string_lossy().to_string());
    }
    if let Some(ref p) = path {
        args.push("--path".to_string());
        args.push(p.to_string_lossy().to_string());
    }
    if let Some(ref pattern) = glob_pattern {
        args.push("--list-glob".to_string());
        args.push(pattern.clone());
    }

    let graph = CodeGraph::open(&db_path)?;
    let exec_id = magellan::output::generate_execution_id();
    let root_str = root.as_ref().map(|p| p.to_string_lossy().to_string());
    let db_path_str = db_path.to_string_lossy().to_string();

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        root_str.as_deref(),
        &db_path_str,
    )?;

    // Helper to finish execution on return
    let finish_execution = |outcome: &str, error_msg: Option<String>| -> Result<()> {
        graph.execution_log().finish_execution(
            &exec_id,
            outcome,
            error_msg.as_deref(),
            0, 0, 0, // No indexing counts for find command
        )
    };

    if let Some(pattern) = glob_pattern {
        let mut graph_mut = CodeGraph::open(&db_path)?;
        finish_execution("success", None)?;
        return run_glob_listing(&mut graph_mut, &pattern, output_format, &exec_id);
    }

    let name =
        name.ok_or_else(|| anyhow::anyhow!("--name is required unless --list-glob is provided"))?;

    let mut graph_mut = CodeGraph::open(&db_path)?;
    let results = match path.as_ref() {
        Some(file_path) => {
            let path_str = resolve_path(file_path, &root);
            match find_in_file(&mut graph_mut, &path_str, &name)? {
                Some(symbol) => vec![symbol],
                None => vec![],
            }
        }
        None => find_all_files(&mut graph_mut, &name)?,
    };

    // Handle JSON output mode
    if output_format == OutputFormat::Json {
        finish_execution("success", None)?;
        return output_json_mode(&name, results, path.as_ref().map(|p| resolve_path(p, &root)), &exec_id, with_context, false, false, with_semantics, with_checksums, context_lines);
    }

    // Human mode (existing behavior)
    if results.is_empty() {
        println!("Symbol '{}' not found", name);
        println!(
            "Hint: use `magellan find --list-glob \"{}\"` to preview name variants.",
            name
        );
    } else if results.len() == 1 {
        let symbol = &results[0];
        println!("Found \"{}\":", name);
        println!("  File:     {}", symbol.file);
        println!(
            "  Kind:     {} [{}]",
            format_symbol_kind(&symbol.kind),
            symbol.kind_normalized
        );
        println!("  Location: Line {}, Column {}", symbol.line, symbol.col);
        println!("  Node ID:  {}", symbol.node_id);
    } else {
        println!("Found {} symbols named \"{}\":", results.len(), name);
        for (i, symbol) in results.iter().enumerate() {
            println!();
            println!("  [{}]", i + 1);
            println!("    File:     {}", symbol.file);
            println!(
                "    Kind:     {} [{}]",
                format_symbol_kind(&symbol.kind),
                symbol.kind_normalized
            );
            println!("    Location: Line {}, Column {}", symbol.line, symbol.col);
        }
    }

    finish_execution("success", None)?;
    Ok(())
}

/// Output find results in JSON format
fn output_json_mode(
    query_name: &str,
    mut results: Vec<FoundSymbol>,
    file_filter: Option<String>,
    exec_id: &str,
    with_context: bool,
    _with_callers: bool,
    _with_callees: bool,
    with_semantics: bool,
    with_checksums: bool,
    context_lines: usize,
) -> Result<()> {
    // Sort deterministically: by file_path, start_line, start_col
    results.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.start_line.cmp(&b.start_line))
            .then_with(|| a.start_col.cmp(&b.start_col))
    });

    // Convert FoundSymbol to SymbolMatch with rich span data
    let matches: Vec<SymbolMatch> = results
        .into_iter()
        .map(|s| {
            let mut span = Span::new(
                s.file.clone(),
                s.byte_start,
                s.byte_end,
                s.start_line,
                s.start_col,
                s.end_line,
                s.end_col,
            );

            // Add context if requested
            if with_context {
                if let Some(context) = SpanContext::extract(
                    &s.file,
                    s.start_line,
                    s.end_line,
                    context_lines,
                ) {
                    span = span.with_context(context);
                }
            }

            // Add semantics if requested
            if with_semantics {
                let language = detect_language_from_path(&s.file);
                span = span.with_semantics_from(s.kind_normalized.clone(), language);
            }

            // Add checksums if requested
            if with_checksums {
                let checksums = SpanChecksums::compute(
                    &s.file,
                    s.byte_start,
                    s.byte_end,
                );
                span = span.with_checksums(checksums);
            }

            // Note: callers/callees population would require symbol_id and graph access
            // This is a placeholder for future implementation with CallOps
            SymbolMatch::new(s.name, s.kind_normalized, span, None, s.symbol_id)
        })
        .collect();

    let response = FindResponse {
        matches,
        query_name: query_name.to_string(),
        file_filter,
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response)?;

    Ok(())
}

/// Detect language from file path extension
fn detect_language_from_path(path: &str) -> String {
    use std::path::Path;
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "rs" => "rust".to_string(),
        "py" => "python".to_string(),
        "js" => "javascript".to_string(),
        "ts" | "tsx" => "typescript".to_string(),
        "java" => "java".to_string(),
        "c" => "c".to_string(),
        "cpp" | "cc" | "cxx" | "hpp" => "cpp".to_string(),
        "go" => "go".to_string(),
        "rb" => "ruby".to_string(),
        "php" => "php".to_string(),
        _ => "unknown".to_string(),
    }
}

fn run_glob_listing(graph: &mut CodeGraph, pattern: &str, output_format: OutputFormat, exec_id: &str) -> Result<()> {
    let glob_matcher = GlobBuilder::new(pattern)
        .case_insensitive(false)
        .build()
        .with_context(|| format!("Invalid glob pattern '{}'", pattern))?
        .compile_matcher();

    let mut matches = Vec::new();
    let file_nodes = graph.all_file_nodes()?;

    for file_path in file_nodes.keys() {
        let entries = query::symbol_nodes_in_file_with_ids(graph, file_path)?;
        for (node_id, fact, symbol_id) in entries {
            if let Some(name) = &fact.name {
                if glob_matcher.is_match(name) {
                    matches.push(FoundSymbol {
                        name: name.clone(),
                        kind: fact.kind.clone(),
                        kind_normalized: fact.kind_normalized.clone(),
                        file: file_path.clone(),
                        byte_start: fact.byte_start,
                        byte_end: fact.byte_end,
                        line: fact.start_line,
                        col: fact.start_col,
                        start_line: fact.start_line,
                        start_col: fact.start_col,
                        end_line: fact.end_line,
                        end_col: fact.end_col,
                        node_id,
                        symbol_id,
                    });
                }
            }
        }
    }

    matches.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.line.cmp(&b.line))
    });

    // Handle JSON output for glob listing
    if output_format == OutputFormat::Json {
        let json_matches: Vec<SymbolMatch> = matches
            .into_iter()
            .map(|s| {
                let span = Span::new(
                    s.file.clone(),
                    s.byte_start,
                    s.byte_end,
                    s.start_line,
                    s.start_col,
                    s.end_line,
                    s.end_col,
                );
                SymbolMatch::new(s.name, s.kind_normalized, span, None, s.symbol_id)
            })
            .collect();

        let response = FindResponse {
            matches: json_matches,
            query_name: pattern.to_string(),
            file_filter: None,
        };

        let json_response = JsonResponse::new(response, exec_id);
        output_json(&json_response)?;
        return Ok(());
    }

    // Human mode
    if matches.is_empty() {
        println!("No symbols matched glob '{}'.", pattern);
        println!("Hint: run `magellan query --explain` for selector guidance.");
        return Ok(());
    }

    println!("Matched {} symbols for glob '{}':", matches.len(), pattern);
    for symbol in matches {
        println!("  Node ID: {}", symbol.node_id);
        println!(
            "    {} [{}] in {}:{} ({})",
            symbol.name,
            symbol.kind_normalized,
            symbol.file,
            symbol.line,
            format_symbol_kind(&symbol.kind)
        );
    }

    Ok(())
}
