//! Get command - Retrieve source code for symbols
//!
//! Usage: magellan get --db <FILE> --file <PATH> --symbol <NAME>

use anyhow::Result;
use std::path::PathBuf;

// Use the library items through the magellan library
use magellan::{CodeGraph, generate_execution_id, OutputFormat};
use magellan::output::{JsonResponse, Span, SymbolMatch, output_json};
use magellan::output::rich::{SpanContext, SpanSemantics, SpanChecksums};
use magellan::graph::query;
use serde::{Deserialize, Serialize};

/// Response for get command with rich span data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResponse {
    /// Symbol details
    pub symbol: SymbolMatch,
    /// Source code content
    pub content: String,
}

/// Detect programming language from file path extension
fn detect_language_from_path(path: &str) -> String {
    use std::path::Path;
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "rs" => "rust".to_string(),
        "py" => "python".to_string(),
        "js" => "javascript".to_string(),
        "ts" => "typescript".to_string(),
        "java" => "java".to_string(),
        "c" => "c".to_string(),
        "cpp" | "cc" | "cxx" | "hpp" => "cpp".to_string(),
        "go" => "go".to_string(),
        _ => "unknown".to_string(),
    }
}

pub fn run_get(
    db_path: PathBuf,
    file_path: String,
    symbol_name: String,
    output_format: OutputFormat,
    with_context: bool,
    with_semantics: bool,
    with_checksums: bool,
    context_lines: usize,
) -> Result<()> {
    // Build args for execution tracking
    let args = vec![
        "get".to_string(),
        "--db".to_string(),
        db_path.to_string_lossy().to_string(),
        "--file".to_string(),
        file_path.clone(),
        "--symbol".to_string(),
        symbol_name.clone(),
    ];

    let graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    let chunks = graph.get_code_chunks_for_symbol(&file_path, &symbol_name)?;

    if chunks.is_empty() {
        eprintln!("No code chunks found for symbol '{}' in file '{}'", symbol_name, file_path);
        graph.execution_log().finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Handle JSON output mode
    if output_format == OutputFormat::Json {
        // For JSON output, we need to get the symbol node to get span information
        // Then we can enrich it with rich span data
        let mut graph_mut = CodeGraph::open(&db_path)?;
        if let Ok(symbol_entries) = query::symbol_nodes_in_file_with_ids(&mut graph_mut, &file_path) {
            for (_node_id, symbol, symbol_id) in symbol_entries {
                if let Some(ref name) = symbol.name {
                    if name == &symbol_name {
                        // Found the symbol - create enriched span
                        let span = Span::new(
                            symbol.file_path.to_string_lossy().to_string(),
                            symbol.byte_start,
                            symbol.byte_end,
                            symbol.start_line,
                            symbol.start_col,
                            symbol.end_line,
                            symbol.end_col,
                        );

                        let mut enriched_span = span;

                        // Add context if requested
                        if with_context {
                            if let Some(context) = SpanContext::extract(
                                &symbol.file_path.to_string_lossy().to_string(),
                                symbol.start_line,
                                symbol.end_line,
                                context_lines,
                            ) {
                                enriched_span = enriched_span.with_context(context);
                            }
                        }

                        // Add semantics if requested
                        if with_semantics {
                            let kind = symbol.kind_normalized.clone();
                            let language = detect_language_from_path(&symbol.file_path.to_string_lossy().to_string());
                            enriched_span = enriched_span.with_semantics_from(kind, language);
                        }

                        // Add checksums if requested
                        if with_checksums {
                            let checksums = SpanChecksums::compute(
                                &symbol.file_path.to_string_lossy().to_string(),
                                symbol.byte_start,
                                symbol.byte_end,
                            );
                            enriched_span = enriched_span.with_checksums(checksums);
                        }

                        let symbol_match = SymbolMatch::new(
                            name.clone(),
                            symbol.kind_normalized.clone(),
                            enriched_span,
                            None,
                            symbol_id,
                        );

                        // Get the content from chunks
                        let content = chunks.iter()
                            .map(|c| c.content.clone())
                            .collect::<Vec<_>>()
                            .join("\n");

                        let response = GetResponse {
                            symbol: symbol_match,
                            content,
                        };

                        let json_response = JsonResponse::new(response, &exec_id);
                        output_json(&json_response)?;
                        break;
                    }
                }
            }
        }
        graph.execution_log().finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Human mode (existing behavior)
    for chunk in chunks {
        println!("// Symbol: {} in {}", chunk.symbol_name.as_ref().unwrap_or(&symbol_name), chunk.file_path);
        println!("// Kind: {}", chunk.symbol_kind.as_ref().unwrap_or(&"?".to_string()));
        println!("// Bytes: {}-{}", chunk.byte_start, chunk.byte_end);
        println!("{}", chunk.content);
        println!();
    }

    graph.execution_log().finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

pub fn run_get_file(db_path: PathBuf, file_path: String) -> Result<()> {
    // Build args for execution tracking
    let args = vec![
        "get-file".to_string(),
        "--db".to_string(),
        db_path.to_string_lossy().to_string(),
        "--file".to_string(),
        file_path.clone(),
    ];

    let graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    let chunks = graph.get_code_chunks(&file_path)?;

    if chunks.is_empty() {
        eprintln!("No code chunks found for file '{}'", file_path);
        graph.execution_log().finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    println!("// {} code chunks in {}", chunks.len(), file_path);
    println!();

    for chunk in chunks {
        let symbol = chunk.symbol_name.as_deref().unwrap_or("<unnamed>");
        let kind = chunk.symbol_kind.as_deref().unwrap_or("?");

        println!("// {} ({}) [{}-{}]", symbol, kind, chunk.byte_start, chunk.byte_end);
        println!("{}", chunk.content);
        println!();
    }

    graph.execution_log().finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}
