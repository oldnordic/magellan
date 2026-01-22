//! Query command implementation
//!
//! Lists symbols in a file, optionally filtered by kind.

use anyhow::Result;
use magellan::{CodeGraph, SymbolFact, SymbolKind};
use magellan::output::{JsonResponse, OutputFormat, QueryResponse, Span, SymbolMatch, output_json};
use magellan::output::rich::{SpanContext, SpanChecksums};
use std::path::PathBuf;

const QUERY_EXPLAIN_TEXT: &str = r#"Query Selector Cheatsheet
--------------------------------
Selectors:
Required selectors:
  --file <path>            Absolute or root-relative path to inspect.

Optional filters:
  --kind <kind>            function|method|struct|trait|enum|mod|type_alias|union|namespace.
  --symbol <name>          Limit output to a specific symbol (case-sensitive).
  --show-extent            With --symbol, print byte + line/column ranges.

Related helpers:
  magellan refs --name <symbol> --path <file>      Show incoming/outgoing references.
  magellan find --name <symbol>                    Locate symbol across files.
  magellan find --list-glob \"test_*\"             Preview glob sets before bulk edits.

Examples:
  magellan query --db mag.db --file src/main.rs --kind function
  magellan query --db mag.db --file src/lib.rs --symbol main --show-extent
  magellan find  --db mag.db --list-glob \"handler_*\""#;

/// Parse a string into a SymbolKind (case-insensitive)
///
/// # Arguments
/// * `s` - String to parse
///
/// # Returns
/// Some(SymbolKind) if recognized, None otherwise
///
/// # Supported values (case-insensitive):
/// - "function", "fn" → Function
/// - "method" → Method
/// - "class", "struct" → Class
/// - "interface", "trait" → Interface
/// - "enum" → Enum
/// - "module", "mod" → Module
/// - "union" → Union
/// - "namespace", "ns" → Namespace
/// - "type", "typealias", "typealias" → TypeAlias
fn parse_symbol_kind(s: &str) -> Option<SymbolKind> {
    match s.to_lowercase().as_str() {
        "function" | "fn" => Some(SymbolKind::Function),
        "method" => Some(SymbolKind::Method),
        "class" | "struct" => Some(SymbolKind::Class),
        "interface" | "trait" => Some(SymbolKind::Interface),
        "enum" => Some(SymbolKind::Enum),
        "module" | "mod" => Some(SymbolKind::Module),
        "union" => Some(SymbolKind::Union),
        "namespace" | "ns" => Some(SymbolKind::Namespace),
        "type" | "typealias" | "type alias" => Some(SymbolKind::TypeAlias),
        _ => None,
    }
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
///
/// # Arguments
/// * `file_path` - The file path (may be relative or absolute)
/// * `root` - Optional root directory for resolving relative paths
///
/// # Returns
/// Absolute path string
///
/// # Behavior
/// - If `file_path` is absolute, return it as-is
/// - If `root` is provided, resolve `file_path` relative to `root`
/// - If `root` is None and `file_path` is relative, canonicalize from current directory
fn resolve_path(file_path: &PathBuf, root: &Option<PathBuf>) -> String {
    if file_path.is_absolute() {
        return file_path.to_string_lossy().to_string();
    }

    // file_path is relative - need to resolve it
    if let Some(ref root) = root {
        // Resolve relative to explicit root (NO GUESSING)
        root.join(file_path).to_string_lossy().to_string()
    } else {
        // No explicit root - try canonicalizing from current directory
        // This may fail if the file doesn't exist from current directory
        std::env::current_dir()
            .ok()
            .and_then(|cwd| cwd.join(file_path).canonicalize().ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string_lossy().to_string())
    }
}

pub fn run_query(
    db_path: PathBuf,
    file_path: Option<PathBuf>,
    root: Option<PathBuf>,
    kind_str: Option<String>,
    explain: bool,
    symbol: Option<String>,
    show_extent: bool,
    output_format: OutputFormat,
    with_context: bool,
    with_callers: bool,
    with_callees: bool,
    with_semantics: bool,
    with_checksums: bool,
    context_lines: usize,
) -> Result<()> {
    // Build args for execution tracking
    let mut args = vec!["query".to_string()];
    if let Some(ref fp) = file_path {
        args.push("--file".to_string());
        args.push(fp.to_string_lossy().to_string());
    }
    if let Some(ref root_path) = root {
        args.push("--root".to_string());
        args.push(root_path.to_string_lossy().to_string());
    }
    if let Some(ref kind) = kind_str {
        args.push("--kind".to_string());
        args.push(kind.clone());
    }
    if explain {
        args.push("--explain".to_string());
    }
    if let Some(ref sym) = symbol {
        args.push("--symbol".to_string());
        args.push(sym.clone());
    }
    if show_extent {
        args.push("--show-extent".to_string());
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
    let finish_execution = |graph: &CodeGraph, outcome: &str, error_msg: Option<String>| -> Result<()> {
        graph.execution_log().finish_execution(
            &exec_id,
            outcome,
            error_msg.as_deref(),
            0, 0, 0, // No indexing counts for query command
        )
    };

    if explain {
        println!("{}", QUERY_EXPLAIN_TEXT);
        finish_execution(&graph, "success", None)?;
        return Ok(());
    }

    if show_extent && symbol.is_none() {
        let err_msg = "--show-extent requires --symbol <name>".to_string();
        finish_execution(&graph, "error", Some(err_msg.clone()))?;
        anyhow::bail!(err_msg);
    }

    // Parse kind filter if provided
    let kind_filter = match kind_str {
        Some(ref s) => match parse_symbol_kind(s) {
            Some(k) => Some(k),
            None => {
                let err_msg = format!("Unknown symbol kind: '{}'. Valid kinds: function, method, class, interface, enum, module, union, namespace, typealias", s);
                finish_execution(&graph, "error", Some(err_msg.clone()))?;
                anyhow::bail!(err_msg);
            }
        },
        None => None,
    };

    let file_path = match file_path {
        Some(fp) => fp,
        None => {
            let err_msg = "--file is required unless --explain is used".to_string();
            finish_execution(&graph, "error", Some(err_msg.clone()))?;
            anyhow::bail!(err_msg);
        }
    };

    let path_str = resolve_path(&file_path, &root);

    // Handle JSON output mode - use symbol_nodes_in_file_with_ids for symbol_id propagation
    if output_format == OutputFormat::Json {
        let mut graph_mut = CodeGraph::open(&db_path)?;
        let mut symbols_with_ids = magellan::graph::query::symbol_nodes_in_file_with_ids(&mut graph_mut, &path_str)?;

        // Apply kind filter
        if let Some(ref filter_kind) = kind_filter {
            symbols_with_ids.retain(|(_, fact, _)| fact.kind == *filter_kind);
        }

        // Apply symbol name filter
        if let Some(ref symbol_name) = symbol {
            symbols_with_ids.retain(|(_, fact, _)| fact.name.as_deref() == Some(symbol_name.as_str()));
        }

        let symbols_with_ids: Vec<(SymbolFact, Option<String>)> =
            symbols_with_ids.into_iter().map(|(_, fact, symbol_id)| (fact, symbol_id)).collect();

        finish_execution(&graph, "success", None)?;
        return output_json_mode(&path_str, symbols_with_ids, kind_str, show_extent, &symbol, &mut graph_mut, &exec_id, with_context, false, false, with_semantics, with_checksums, context_lines);
    }

    // Human mode - use existing flow
    let mut graph_mut = CodeGraph::open(&db_path)?;
    let mut symbols = graph_mut.symbols_in_file_with_kind(&path_str, kind_filter)?;

    if let Some(ref symbol_name) = symbol {
        symbols.retain(|s| s.name.as_deref() == Some(symbol_name.as_str()));
    }

    // Human mode (existing behavior)
    println!("{}:", path_str);

    if symbols.is_empty() {
        println!("  (no symbols found)");
        match symbol {
            Some(ref sym) => println!(
                "  Hint: verify the symbol name or run `magellan find --list-glob \"{}\"`.",
                sym
            ),
            None => println!("  Hint: run `magellan query --explain` for selector syntax."),
        }
        finish_execution(&graph, "success", None)?;
        return Ok(());
    }

    for symbol in &symbols {
        let kind_str = format_symbol_kind(&symbol.kind);
        let name = symbol.name.as_deref().unwrap_or("(unnamed)");
        println!(
            "  Line {:4}: {:12} {:<} [{}]",
            symbol.start_line, kind_str, name, symbol.kind_normalized
        );
    }

    if show_extent {
        if let Some(ref symbol_name) = symbol {
            let mut extents = graph_mut.symbol_extents(&path_str, symbol_name)?;
            if extents.is_empty() {
                println!("  (no extent info found for '{}')", symbol_name);
                finish_execution(&graph, "success", None)?;
                return Ok(());
            }
            println!();
            println!("Symbol Extents for '{}':", symbol_name);
            extents.sort_by(|(_, a), (_, b)| {
                a.start_line
                    .cmp(&b.start_line)
                    .then_with(|| a.start_col.cmp(&b.start_col))
            });
            for (node_id, fact) in extents {
                print_extent_block(node_id, &fact);
            }
        }
    }

    finish_execution(&graph, "success", None)?;
    Ok(())
}

/// Output query results in JSON format
fn output_json_mode(
    path_str: &str,
    mut symbols_with_ids: Vec<(SymbolFact, Option<String>)>,
    kind_str: Option<String>,
    _show_extent: bool,
    _symbol: &Option<String>,
    _graph: &mut CodeGraph,
    exec_id: &str,
    with_context: bool,
    _with_callers: bool,
    _with_callees: bool,
    with_semantics: bool,
    with_checksums: bool,
    context_lines: usize,
) -> Result<()> {
    // Sort deterministically: by file_path, start_line, start_col, name
    symbols_with_ids.sort_by(|(a, _), (b, _)| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.start_line.cmp(&b.start_line))
            .then_with(|| a.start_col.cmp(&b.start_col))
            .then_with(|| a.name.as_deref().cmp(&b.name.as_deref()))
    });

    // Convert (SymbolFact, Option<symbol_id>) to SymbolMatch with rich span data
    let symbol_matches: Vec<SymbolMatch> = symbols_with_ids
        .into_iter()
        .map(|(s, symbol_id)| {
            let file_path = s.file_path.to_string_lossy().to_string();
            let mut span = Span::new(
                file_path.clone(),
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
                    &file_path,
                    s.start_line,
                    s.end_line,
                    context_lines,
                ) {
                    span = span.with_context(context);
                }
            }

            // Add semantics if requested
            if with_semantics {
                let language = detect_language_from_path(&file_path);
                span = span.with_semantics_from(s.kind_normalized.clone(), language);
            }

            // Add checksums if requested
            if with_checksums {
                let checksums = SpanChecksums::compute(
                    &file_path,
                    s.byte_start,
                    s.byte_end,
                );
                span = span.with_checksums(checksums);
            }

            SymbolMatch::new(
                s.name.unwrap_or_else(|| "(unnamed)".to_string()),
                s.kind_normalized,
                span,
                None, // parent not tracked yet
                symbol_id, // stable symbol ID from graph
            )
        })
        .collect();

    let response = QueryResponse {
        symbols: symbol_matches,
        file_path: path_str.to_string(),
        kind_filter: kind_str,
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response)?;

    Ok(())
}

fn print_extent_block(node_id: i64, symbol: &magellan::SymbolFact) {
    let name = symbol.name.as_deref().unwrap_or("(unnamed)");
    println!("  Node ID: {}", node_id);
    println!(
        "    {} [{}] at {}",
        name,
        symbol.kind_normalized,
        symbol.file_path.to_string_lossy()
    );
    println!("    Byte Range: {}..{}", symbol.byte_start, symbol.byte_end);
    println!(
        "    Line Range: {}:{} -> {}:{}",
        symbol.start_line, symbol.start_col, symbol.end_line, symbol.end_col
    );
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