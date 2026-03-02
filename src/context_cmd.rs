//! Context command implementation
//!
//! Provides summarized, paginated context queries for LLMs.

use anyhow::Result;
use magellan::context::{build_context_index, get_or_build_context_index, get_file_context, get_symbol_detail, list_symbols, ListQuery};
use magellan::output::generate_execution_id;
use magellan::CodeGraph;
use std::path::PathBuf;

/// Run the context build command
pub fn run_context_build(db_path: PathBuf) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let _exec_id = generate_execution_id();

    build_context_index(&mut graph, &db_path)?;

    Ok(())
}

/// Run the context summary command
pub fn run_context_summary(db_path: PathBuf) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    // Get or build index
    let index = get_or_build_context_index(&mut graph, &db_path)?;

    // Print summary
    println!("{}", index.summary.description);
    println!();
    println!("Project: {} {}", index.summary.name, index.summary.version);
    println!("Language: {}", index.summary.language);
    println!("Files: {}", index.summary.total_files);
    println!("Symbols: {}", index.summary.total_symbols);
    println!();
    println!("Symbol Breakdown:");
    println!("  Functions: {}", index.summary.symbol_counts.functions);
    println!("  Methods: {}", index.summary.symbol_counts.methods);
    println!("  Structs: {}", index.summary.symbol_counts.structs);
    println!("  Traits: {}", index.summary.symbol_counts.traits);
    println!("  Enums: {}", index.summary.symbol_counts.enums);
    println!("  Modules: {}", index.summary.symbol_counts.modules);

    if !index.summary.entry_points.is_empty() {
        println!();
        println!("Entry Points:");
        for entry in &index.summary.entry_points {
            println!("  - {}", entry);
        }
    }

    Ok(())
}

/// Run the context list command
pub fn run_context_list(
    db_path: PathBuf,
    kind: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
    cursor: Option<String>,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    let query = ListQuery {
        kind,
        page,
        page_size,
        cursor,
        file_pattern: None,
    };

    let result = list_symbols(&mut graph, &query)?;

    // Print results
    println!("Page {} of {} ({} total symbols)", result.page, result.total_pages, result.total_items);
    println!();

    for item in &result.items {
        println!("  {}:{}  {}  ({})", item.file, item.line, item.name, item.kind);
    }

    // Print pagination info
    if let Some(ref next) = result.next_cursor {
        println!();
        println!("Next page: --cursor {}", next);
    }
    if let Some(ref prev) = result.prev_cursor {
        println!("Prev page: --cursor {}", prev);
    }

    Ok(())
}

/// Run the context symbol command
pub fn run_context_symbol(
    db_path: PathBuf,
    name: String,
    file: Option<String>,
    include_callers: bool,
    include_callees: bool,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    let detail = get_symbol_detail(&mut graph, &name, file.as_deref());

    match detail {
        Ok(detail) => {
            // Print symbol info
            println!("Symbol: {}", detail.name);
            println!("Kind: {}", detail.kind);
            println!("File: {}:{}", detail.file, detail.line);

            if let Some(ref sig) = detail.signature {
                println!("Signature: {}", sig);
            }

            if let Some(ref doc) = detail.documentation {
                println!("Documentation: {}", doc);
            }

            if include_callers && !detail.callers.is_empty() {
                println!();
                println!("Callers ({}):", detail.callers.len());
                for caller in &detail.callers {
                    println!("  - {}", caller);
                }
            }

            if include_callees && !detail.callees.is_empty() {
                println!();
                println!("Callees ({}):", detail.callees.len());
                for callee in &detail.callees {
                    println!("  - {}", callee);
                }
            }

            if !detail.related.is_empty() {
                println!();
                println!("Related symbols:");
                for related in &detail.related {
                    println!("  - {}", related);
                }
            }
        }
        Err(_) => {
            // Symbol not found - provide suggestions
            eprintln!("Error: Symbol '{}' not found", name);
            
            // Try to find similar symbols
            let all_symbols = graph.get_symbols_by_label(&name).unwrap_or_default();
            if !all_symbols.is_empty() {
                eprintln!();
                eprintln!("Found {} similar symbol(s) in other files:", all_symbols.len());
                for (i, sym) in all_symbols.iter().take(5).enumerate() {
                    eprintln!("  {}. {} (in {})", i + 1, sym.name, sym.file_path);
                }
                if all_symbols.len() > 5 {
                    eprintln!("  ... and {} more", all_symbols.len() - 5);
                }
                eprintln!();
                eprintln!("Try: magellan context symbol --db code.db --name \"{}\" --file <path>", name);
            } else {
                // Try fuzzy search - look for symbols containing the name
                eprintln!();
                eprintln!("No exact matches. Try searching with a different name.");
                eprintln!("Hint: Use 'magellan context list --kind fn' to list all functions");
            }
        }
    }

    Ok(())
}

/// Run the context file command
pub fn run_context_file(db_path: PathBuf, path: String) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    let context = get_file_context(&mut graph, &path)?;

    // Print file info
    println!("File: {}", context.path);
    println!("Language: {}", context.language);
    println!("Symbols: {}", context.symbol_count);
    println!();

    println!("Symbol Breakdown:");
    println!("  Functions: {}", context.symbol_counts.functions);
    println!("  Methods: {}", context.symbol_counts.methods);
    println!("  Structs: {}", context.symbol_counts.structs);
    println!("  Traits: {}", context.symbol_counts.traits);
    println!("  Enums: {}", context.symbol_counts.enums);
    println!();

    println!("Public Symbols:");
    for symbol in &context.public_symbols {
        println!("  - {}", symbol);
    }

    if !context.imports.is_empty() {
        println!();
        println!("Imports:");
        for import in &context.imports {
            println!("  - {}", import);
        }
    }

    Ok(())
}

/// Run the context server command
pub fn run_context_server(db_path: PathBuf, port: u16, host: String) -> Result<()> {
    use magellan::context::{run_context_server as run_server, ServerConfig};

    let config = ServerConfig {
        db_path,
        port,
        host,
        cors: true,
    };

    run_server(config)
}
