//! Find command implementation
//!
//! Finds a symbol by name, optionally limited to a specific file.
//! Now supports multiple backends: SQLite, Geometric, V3

use anyhow::{Context, Result};
use globset::GlobBuilder;
use magellan::backend_router::{BackendType, MagellanBackend};
use magellan::common::{detect_language_from_path, format_symbol_kind, resolve_path};
use magellan::graph::query;
use magellan::output::rich::{SpanChecksums, SpanContext};
use magellan::output::{
    output_json, CalleeInfo, CallerInfo, FindResponse, JsonResponse, OutputFormat, Span,
    SymbolMatch,
};
use magellan::{CodeGraph, SymbolKind};
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
    canonical_fqn: Option<String>,
    display_fqn: Option<String>,
}

/// Score a symbol based on relevance criteria
///
/// Scoring criteria (highest score first):
/// 1. Exact name match (+100) vs substring match (+10)
/// 2. Public API (+50) vs private/internal (0)
/// 3. Non-test files (+30) vs test files (0)
/// 4. Shorter FQN (+20) - prefer top-level definitions
/// 5. Kind priority (+10 for Functions/Structs, +5 for Modules, 0 for impl methods)
fn score_symbol(symbol: &FoundSymbol, query: &str) -> i32 {
    let mut score = 0;

    // 1. Exact match bonus vs substring match
    if symbol.name == query {
        score += 100;
    } else if symbol.name.contains(query) {
        score += 10;
    }

    // 2. Public API vs private/internal
    // Check if symbol name starts with '_' or is in a private module
    let is_private = symbol.name.starts_with('_')
        || symbol.file.contains("/private/")
        || symbol.file.contains("/internal/")
        || (symbol.file.contains("/mod.rs") && symbol.name.starts_with('_'));
    if !is_private {
        score += 50;
    }

    // 3. Non-test files vs test files
    let is_test_file = symbol.file.contains("/test")
        || symbol.file.contains("/tests/")
        || symbol.file.contains("_test.")
        || symbol.file.contains("_tests.");
    if !is_test_file {
        score += 30;
    }

    // 4. Shorter FQN - prefer top-level definitions
    // Score = 20 / (number of '::' in kind_normalized + 1)
    let scope_depth = symbol.kind_normalized.matches("::").count() as i32 + 1;
    score += 20 / scope_depth;

    // 5. Kind priority
    match symbol.kind {
        SymbolKind::Function | SymbolKind::Class => score += 10,
        SymbolKind::Module => score += 5,
        SymbolKind::Method => score += 0, // impl methods get no bonus
        _ => score += 2, // Other kinds get small bonus
    }

    score
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
                    canonical_fqn: symbol.canonical_fqn.clone(),
                    display_fqn: symbol.display_fqn.clone(),
                }));
            }
        }
    }

    Ok(None)
}

/// Find a symbol across all files by name
///
/// Returns matching symbols sorted by relevance score (highest first),
/// limited to top 10 results.
fn find_all_files(graph: &mut CodeGraph, name: &str) -> Result<Vec<FoundSymbol>> {
    let mut results = Vec::new();

    // Get all indexed files
    let file_nodes = graph.all_file_nodes()?;

    // Search each file for the symbol (exact or substring match)
    for file_path in file_nodes.keys() {
        let entries = query::symbol_nodes_in_file_with_ids(graph, file_path)?;
        for (node_id, symbol, symbol_id) in entries {
            if let Some(symbol_name) = &symbol.name {
                // Match exact name or substring
                if symbol_name == name || symbol_name.contains(name) {
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
                        canonical_fqn: symbol.canonical_fqn.clone(),
                        display_fqn: symbol.display_fqn.clone(),
                    });
                }
            }
        }
    }

    // Score and sort by relevance (highest score first)
    results.sort_by(|a, b| {
        let score_a = score_symbol(a, name);
        let score_b = score_symbol(b, name);
        score_b.cmp(&score_a) // Descending order
    });

    // Limit to top 10 results
    results.truncate(10);

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
/// * `symbol_id` - Optional stable SymbolId for precise lookup
/// * `ambiguous_name` - Optional display FQN to show all candidates
/// * `first` - Use first match when ambiguous (deprecated)
/// * `output_format` - Output format (Human or Json)
/// * `with_callers` - Include callers of found symbols
/// * `with_callees` - Include callees of found symbols
///
/// # Displays
/// Human-readable symbol details or JSON output
pub fn run_find(
    db_path: PathBuf,
    name: Option<String>,
    root: Option<PathBuf>,
    path: Option<PathBuf>,
    glob_pattern: Option<String>,
    symbol_id: Option<String>,
    ambiguous_name: Option<String>,
    first: bool,
    output_format: OutputFormat,
    with_context: bool,
    with_callers: bool,
    with_callees: bool,
    with_semantics: bool,
    with_checksums: bool,
    context_lines: usize,
) -> Result<()> {
    // Check if this is a geometric database and route accordingly
    if MagellanBackend::detect_type(&db_path) == BackendType::Geometric {
        // For geometric databases, use the geometric backend directly
        return run_find_geometric(db_path, name, output_format);
    }

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
    if let Some(ref sid) = symbol_id {
        args.push("--symbol-id".to_string());
        args.push(sid.clone());
    }
    if let Some(ref amb_name) = ambiguous_name {
        args.push("--ambiguous".to_string());
        args.push(amb_name.clone());
    }
    if first {
        args.push("--first".to_string());
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
            0,
            0,
            0, // No indexing counts for find command
        )
    };

    if let Some(pattern) = glob_pattern {
        let mut graph_mut = CodeGraph::open(&db_path)?;
        finish_execution("success", None)?;
        return run_glob_listing(&mut graph_mut, &pattern, output_format, &exec_id);
    }

    // Handle --symbol-id precise lookup
    if let Some(sid) = symbol_id {
        let mut graph_lookup = CodeGraph::open(&db_path)?;
        match query::find_by_symbol_id(&mut graph_lookup, &sid)? {
            Some(symbol) => {
                // Output single result in human mode
                finish_execution("success", None)?;
                println!("Found symbol ID: {}", sid);
                if let Some(name) = &symbol.name {
                    println!("  Name:     {}", name);
                }
                println!("  Kind:     {}", symbol.kind);
                if let Some(canon) = &symbol.canonical_fqn {
                    println!("  Canonical: {}", canon);
                }
                if let Some(display) = &symbol.display_fqn {
                    println!("  Display:  {}", display);
                }
                println!(
                    "  Location: Line {}, Column {}",
                    symbol.start_line, symbol.start_col
                );
                return Ok(());
            }
            None => {
                finish_execution("success", None)?;
                eprintln!("Symbol ID '{}' not found", sid);
                return Ok(());
            }
        }
    }

    // Handle --ambiguous symbol name query (show all candidates)
    if let Some(amb_name) = ambiguous_name {
        let mut graph_lookup = CodeGraph::open(&db_path)?;
        let candidates = query::get_ambiguous_candidates(&mut graph_lookup, &amb_name)?;

        if candidates.is_empty() {
            finish_execution("success", None)?;
            eprintln!("No symbols found with name '{}'", amb_name);
            return Ok(());
        }

        // Display candidates with SymbolId and canonical FQN
        for (entity_id, symbol) in candidates.iter().enumerate() {
            let sid = symbol.1.symbol_id.as_deref().unwrap_or("<none>");
            let canon = symbol.1.canonical_fqn.as_deref().unwrap_or("<none>");
            eprintln!("  [{}]", entity_id + 1);
            eprintln!("    Symbol ID: {}", sid);
            eprintln!("    Canonical: {}", canon);
            eprintln!("    Name: {}", symbol.1.name.as_deref().unwrap_or("<none>"));
            eprintln!("    Kind: {}", symbol.1.kind);
        }

        finish_execution("success", None)?;
        return Ok(());
    }

    let name = name.ok_or_else(|| {
        anyhow::anyhow!(
            "--name is required unless --list-glob, --symbol-id, or --ambiguous is provided"
        )
    })?;

    // Detect backend type and route accordingly
    let backend_type = MagellanBackend::detect_type(&db_path);

    match backend_type {
        BackendType::Geometric => {
            // Use geometric backend
            let backend = MagellanBackend::open(&db_path)?;
            match backend.find_symbol_by_fqn(&name) {
                Ok(Some(info)) => {
                    finish_execution("success", None)?;
                    println!("Found symbol: {}", info.fqn);
                    println!("  Name:     {}", info.name);
                    println!("  Kind:     {:?}", info.kind);
                    println!("  File:     {}", info.file_path);
                    println!(
                        "  Location: Line {}, Column {}",
                        info.start_line, info.start_col
                    );
                    return Ok(());
                }
                Ok(None) => {
                    finish_execution("success", None)?;
                    eprintln!("Symbol '{}' not found", name);
                    return Ok(());
                }
                Err(e) => {
                    finish_execution("error", Some(e.to_string()))?;
                    return Err(e);
                }
            }
        }
        BackendType::SQLite | BackendType::NativeV3 => {
            // Use SQLite backend (existing behavior)
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
            if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
                finish_execution("success", None)?;
                return output_json_mode(
                    &mut graph_mut,
                    &name,
                    results,
                    path.as_ref().map(|p| resolve_path(p, &root)),
                    &exec_id,
                    output_format,
                    with_context,
                    with_callers,
                    with_callees,
                    with_semantics,
                    with_checksums,
                    context_lines,
                );
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
                // Multiple results
                if first {
                    // Emit deprecation warning
                    eprintln!(
                        "WARNING: --first is deprecated. Use --symbol-id for precise lookups."
                    );
                    let symbol = &results[0];
                    println!("Found \"{}\" (using first match):", name);
                    println!("  File:     {}", symbol.file);
                    println!(
                        "  Kind:     {} [{}]",
                        format_symbol_kind(&symbol.kind),
                        symbol.kind_normalized
                    );
                    println!("  Location: Line {}, Column {}", symbol.line, symbol.col);
                    println!("  Node ID:  {}", symbol.node_id);
                } else {
                    // Ambiguous, no --first: show ranked list of candidates
                    eprintln!(
                        "Ambiguous symbol name '{}': found {} candidates",
                        name,
                        results.len()
                    );
                    eprintln!();
                    eprintln!("Top matches:");

                    // Show up to 10 candidates with numbered options
                    let display_count = results.len().min(10);
                    for (i, symbol) in results.iter().take(display_count).enumerate() {
                        let fqn = symbol
                            .display_fqn
                            .as_ref()
                            .or(symbol.canonical_fqn.as_ref())
                            .map(|s| s.as_str())
                            .unwrap_or("<unknown>");
                        let sid = symbol.symbol_id.as_deref().unwrap_or("<none>");

                        eprintln!(
                            "  [{}] {} ({}) in {}:{}",
                            i + 1,
                            symbol.name,
                            symbol.kind_normalized,
                            symbol.file,
                            symbol.line
                        );
                        eprintln!("      Symbol ID: {}", sid);
                        eprintln!("      FQN: {}", fqn);
                    }

                    if results.len() > 10 {
                        eprintln!();
                        eprintln!("  ... and {} more", results.len() - 10);
                    }

                    eprintln!();
                    eprintln!("Use --path <file> to disambiguate, or --symbol-id <id> for precise lookup");
                }
            }

            finish_execution("success", None)?;
            Ok(())
        }
    }
}

/// Find command for geometric backend databases
fn run_find_geometric(
    db_path: PathBuf,
    name: Option<String>,
    _output_format: OutputFormat,
) -> Result<()> {
    use magellan::backend_router::MagellanBackend;

    let backend = MagellanBackend::open(&db_path)?;

    let search_name = match name {
        Some(n) => n,
        None => {
            eprintln!("Error: --name is required for geometric databases");
            return Ok(());
        }
    };

    // Try to find by FQN first
    match backend.find_symbol_by_fqn(&search_name) {
        Ok(Some(info)) => {
            // Human-readable output (simplified for now)
            println!("Found symbol: {}", info.fqn);
            println!("  Name:     {}", info.name);
            println!("  Kind:     {:?}", info.kind);
            println!("  File:     {}", info.file_path);
            println!(
                "  Location: Line {}, Column {}",
                info.start_line, info.start_col
            );
            return Ok(());
        }
        Ok(None) => {
            // FQN not found, try searching by simple name
        }
        Err(e) => {
            eprintln!("Error searching geometric database: {}", e);
            return Err(e);
        }
    }

    // Try finding by simple name
    match backend.find_symbols_by_name(&search_name) {
        Ok(symbols) if !symbols.is_empty() => {
            if symbols.len() == 1 {
                let info = &symbols[0];
                println!("Found symbol: {}", info.fqn);
                println!("  Name:     {}", info.name);
                println!("  Kind:     {:?}", info.kind);
                println!("  File:     {}", info.file_path);
                println!(
                    "  Location: Line {}, Column {}",
                    info.start_line, info.start_col
                );
            } else {
                println!("Found {} symbols matching '{}'", symbols.len(), search_name);
                for (i, info) in symbols.iter().enumerate() {
                    println!("\n  [{}] {}", i + 1, info.fqn);
                    println!("      Name:     {}", info.name);
                    println!("      Kind:     {:?}", info.kind);
                    println!("      File:     {}", info.file_path);
                    println!(
                        "      Location: Line {}, Column {}",
                        info.start_line, info.start_col
                    );
                }
            }
            Ok(())
        }
        Ok(_) => {
            println!("Symbol '{}' not found in geometric database", search_name);
            println!(
                "Hint: Use `magellan geometric query --db {} --function <fqn>` for CFG queries",
                db_path.display()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Error searching geometric database: {}", e);
            Err(e)
        }
    }
}

/// Output find results in JSON format
fn output_json_mode(
    graph: &mut CodeGraph,
    query_name: &str,
    mut results: Vec<FoundSymbol>,
    file_filter: Option<String>,
    exec_id: &str,
    output_format: OutputFormat,
    with_context: bool,
    with_callers: bool,
    with_callees: bool,
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
    let mut matches = Vec::new();
    for s in results {
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
            if let Some(context) =
                SpanContext::extract(&s.file, s.start_line, s.end_line, context_lines)
            {
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
            let checksums = SpanChecksums::compute(&s.file, s.byte_start, s.byte_end);
            span = span.with_checksums(checksums);
        }

        // Fetch callers if requested
        let callers = if with_callers {
            match graph.callers_of_symbol(&s.file, &s.name) {
                Ok(call_facts) => Some(
                    call_facts
                        .into_iter()
                        .map(|fact| CallerInfo {
                            name: fact.caller,
                            file_path: fact.file_path.to_string_lossy().to_string(),
                            line: fact.start_line,
                            column: fact.start_col,
                        })
                        .collect(),
                ),
                Err(_) => None,
            }
        } else {
            None
        };

        // Fetch callees if requested
        let callees = if with_callees {
            match graph.calls_from_symbol(&s.file, &s.name) {
                Ok(call_facts) => Some(
                    call_facts
                        .into_iter()
                        .map(|fact| CalleeInfo {
                            name: fact.callee,
                            file_path: fact.file_path.to_string_lossy().to_string(),
                        })
                        .collect(),
                ),
                Err(_) => None,
            }
        } else {
            None
        };

        let mut symbol_match = SymbolMatch::new(s.name, s.kind_normalized, span, None, s.symbol_id);
        symbol_match.callers = callers;
        symbol_match.callees = callees;

        matches.push(symbol_match);
    }
    let matches: Vec<SymbolMatch> = matches;

    let response = FindResponse {
        matches,
        query_name: query_name.to_string(),
        file_filter,
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response, output_format)?;

    Ok(())
}

fn run_glob_listing(
    graph: &mut CodeGraph,
    pattern: &str,
    output_format: OutputFormat,
    exec_id: &str,
) -> Result<()> {
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
                        canonical_fqn: fact.canonical_fqn.clone(),
                        display_fqn: fact.display_fqn.clone(),
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
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
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
        output_json(&json_response, output_format)?;
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
        if let Some(id) = &symbol.symbol_id {
            println!("    SymbolId: {}", id);
        }
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
