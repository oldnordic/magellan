//! Get command - Retrieve source code for symbols
//!
//! Usage: magellan get --db <FILE> --file <PATH> --symbol <NAME>

use anyhow::Result;
use std::path::PathBuf;

// Use the library items through the magellan library
use magellan::backend_router::MagellanBackend;
use magellan::common::detect_language_from_path;
use magellan::generation::schema::CodeChunk;
use magellan::graph::query;
use magellan::output::rich::{SpanChecksums, SpanContext};
use magellan::output::{output_json, JsonResponse, Span, SymbolMatch};
use magellan::{generate_execution_id, CodeGraph, OutputFormat};
use serde::{Deserialize, Serialize};

/// Response for get command with rich span data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResponse {
    /// Symbol details
    pub symbol: SymbolMatch,
    /// Source code content
    pub content: String,
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

    let backend = MagellanBackend::open(&db_path)?;
    let exec_id = generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    backend.start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    let chunks = backend.get_code_chunks_for_symbol(&file_path, &symbol_name)?;

    if chunks.is_empty() {
        eprintln!(
            "No code chunks found for symbol '{}' in file '{}'",
            symbol_name, file_path
        );
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Handle JSON output mode
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        // For Geo backend, use backend-neutral API
        #[cfg(feature = "geometric-backend")]
        if matches!(
            MagellanBackend::detect_type(&db_path),
            magellan::backend_router::BackendType::Geometric
        ) {
            // Geo backend: use backend-neutral API
            let symbols = backend.symbols_in_file(&file_path)?;
            if let Some(symbol_info) = symbols.iter().find(|s| s.name == symbol_name) {
                // Create span from symbol info
                let span = Span::new(
                    symbol_info.file_path.clone(),
                    symbol_info.byte_start as usize,
                    symbol_info.byte_end as usize,
                    symbol_info.start_line as usize,
                    symbol_info.start_col as usize,
                    symbol_info.end_line as usize,
                    symbol_info.end_col as usize,
                );

                let mut enriched_span = span;

                // Add context if requested
                if with_context {
                    if let Some(context) = SpanContext::extract(
                        &symbol_info.file_path,
                        symbol_info.start_line as usize,
                        symbol_info.end_line as usize,
                        context_lines,
                    ) {
                        enriched_span = enriched_span.with_context(context);
                    }
                }

                // Add semantics if requested
                if with_semantics {
                    let kind = format!("{:?}", symbol_info.kind);
                    let language = symbol_info
                        .language
                        .clone()
                        .unwrap_or_else(|| detect_language_from_path(&symbol_info.file_path));
                    enriched_span = enriched_span.with_semantics_from(kind, language);
                }

                // Add checksums if requested
                if with_checksums {
                    let checksums = SpanChecksums::compute(
                        &symbol_info.file_path,
                        symbol_info.byte_start as usize,
                        symbol_info.byte_end as usize,
                    );
                    enriched_span = enriched_span.with_checksums(checksums);
                }

                let symbol_match = SymbolMatch::new(
                    symbol_info.name.clone(),
                    format!("{:?}", symbol_info.kind),
                    enriched_span,
                    None,
                    Some(format!("{}", symbol_info.id)),
                );

                // Get the content from chunks
                let content = chunks
                    .iter()
                    .map(|c| c.content.clone())
                    .collect::<Vec<_>>()
                    .join("\n");

                let response = GetResponse {
                    symbol: symbol_match,
                    content,
                };

                let json_response = JsonResponse::new(response, &exec_id);
                output_json(&json_response, output_format)?;
                backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
                return Ok(());
            } else {
                eprintln!("Symbol '{}' not found in file '{}'", symbol_name, file_path);
                backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
                return Ok(());
            }
        }

        // For SQLite backend (or no Geo feature), use SQLite-specific query
        let mut graph = CodeGraph::open(&db_path)?;
        if let Ok(symbol_entries) = query::symbol_nodes_in_file_with_ids(&mut graph, &file_path) {
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
                                symbol.file_path.to_string_lossy().as_ref(),
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
                            let language = detect_language_from_path(
                                symbol.file_path.to_string_lossy().as_ref(),
                            );
                            enriched_span = enriched_span.with_semantics_from(kind, language);
                        }

                        // Add checksums if requested
                        if with_checksums {
                            let checksums = SpanChecksums::compute(
                                symbol.file_path.to_string_lossy().as_ref(),
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
                        let content = chunks
                            .iter()
                            .map(|c| c.content.clone())
                            .collect::<Vec<_>>()
                            .join("\n");

                        let response = GetResponse {
                            symbol: symbol_match,
                            content,
                        };

                        let json_response = JsonResponse::new(response, &exec_id);
                        output_json(&json_response, output_format)?;
                        break;
                    }
                }
            }
        }
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Human mode (backend-neutral)
    for chunk in chunks {
        println!(
            "// Symbol: {} in {}",
            chunk.symbol_name.as_ref().unwrap_or(&symbol_name),
            chunk.file_path
        );
        println!(
            "// Kind: {}",
            chunk.symbol_kind.as_ref().unwrap_or(&"?".to_string())
        );
        println!("// Bytes: {}-{}", chunk.byte_start, chunk.byte_end);
        println!("{}", chunk.content);
        println!();
    }

    backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
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

    let backend = MagellanBackend::open(&db_path)?;
    let exec_id = generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    backend.start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    let chunks = backend.get_code_chunks(&file_path)?;

    if chunks.is_empty() {
        eprintln!("No code chunks found for file '{}'", file_path);
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    println!("// {} code chunks in {}", chunks.len(), file_path);
    println!();

    for chunk in chunks {
        let symbol = chunk.symbol_name.as_deref().unwrap_or("<unnamed>");
        let kind = chunk.symbol_kind.as_deref().unwrap_or("?");

        println!(
            "// {} ({}) [{}-{}]",
            symbol, kind, chunk.byte_start, chunk.byte_end
        );
        println!("{}", chunk.content);
        println!();
    }

    backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

/// List all code chunks in the database.
///
/// Usage: magellan chunks --db <FILE> [--limit N] [--file PATTERN] [--kind KIND] [--output FORMAT]
pub fn run_chunks(
    db_path: PathBuf,
    output_format: OutputFormat,
    limit: Option<usize>,
    file_filter: Option<String>,
    kind_filter: Option<String>,
) -> Result<()> {
    // Build args for execution tracking
    let mut args = vec![
        "chunks".to_string(),
        "--db".to_string(),
        db_path.to_string_lossy().to_string(),
    ];
    if let Some(ref limit_val) = limit {
        args.push("--limit".to_string());
        args.push(limit_val.to_string());
    }
    if let Some(ref file) = file_filter {
        args.push("--file".to_string());
        args.push(file.clone());
    }
    if let Some(ref kind) = kind_filter {
        args.push("--kind".to_string());
        args.push(kind.clone());
    }

    // NOTE: file_filter and kind_filter are SQLite-specific features
    // For Geo backend, we currently get all chunks and filter
    let backend = MagellanBackend::open(&db_path)?;
    let exec_id = generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    backend.start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    // For now, we need to use SQLite directly for filtering
    // TODO: Add filtering support to MagellanBackend
    #[cfg(feature = "geometric-backend")]
    let chunks = if matches!(
        MagellanBackend::detect_type(&db_path),
        magellan::backend_router::BackendType::Geometric
    ) {
        // For Geo backend, get all chunks (no filtering support yet)
        // This is a known limitation - filtering is SQLite-only for now
        Vec::new() // Placeholder - would need to implement get_all_chunks()
    } else {
        // Fallback to SQLite for filtering
        use rusqlite::Connection;
        let conn = Connection::open(&db_path)?;

        let mut query = String::from(
            "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                 symbol_name, symbol_kind, created_at
                 FROM code_chunks
                 WHERE 1=1",
        );

        let mut params: Vec<String> = Vec::new();

        if let Some(ref file_pattern) = file_filter {
            query.push_str(&format!(" AND file_path LIKE ?{}", params.len() + 1));
            params.push(format!("%{}%", file_pattern));
        }

        if let Some(ref kind) = kind_filter {
            query.push_str(&format!(" AND symbol_kind = ?{}", params.len() + 1));
            params.push(kind.to_string());
        }

        query.push_str(" ORDER BY file_path, byte_start");

        if let Some(limit_val) = limit {
            query.push_str(&format!(" LIMIT {}", limit_val));
        }

        let mut stmt = conn.prepare(&query)?;
        let params_ref: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let chunk_iter = stmt.query_map(params_ref.as_slice(), |row| {
            Ok(CodeChunk {
                id: Some(row.get(0)?),
                file_path: row.get(1)?,
                byte_start: row.get(2)?,
                byte_end: row.get(3)?,
                content: row.get(4)?,
                content_hash: row.get(5)?,
                symbol_name: row.get(6)?,
                symbol_kind: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let chunks: Result<Vec<CodeChunk>, _> = chunk_iter.collect();
        chunks?
    };

    #[cfg(not(feature = "geometric-backend"))]
    let chunks = {
        // SQLite-only path
        use rusqlite::Connection;
        let conn = Connection::open(&db_path)?;

        let mut query = String::from(
            "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                 symbol_name, symbol_kind, created_at
                 FROM code_chunks
                 WHERE 1=1",
        );

        let mut params: Vec<String> = Vec::new();

        if let Some(ref file_pattern) = file_filter {
            query.push_str(&format!(" AND file_path LIKE ?{}", params.len() + 1));
            params.push(format!("%{}%", file_pattern));
        }

        if let Some(ref kind) = kind_filter {
            query.push_str(&format!(" AND symbol_kind = ?{}", params.len() + 1));
            params.push(kind.to_string());
        }

        query.push_str(" ORDER BY file_path, byte_start");

        if let Some(limit_val) = limit {
            query.push_str(&format!(" LIMIT {}", limit_val));
        }

        let mut stmt = conn.prepare(&query)?;
        let params_ref: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let chunk_iter = stmt.query_map(params_ref.as_slice(), |row| {
            Ok(CodeChunk {
                id: Some(row.get(0)?),
                file_path: row.get(1)?,
                byte_start: row.get(2)?,
                byte_end: row.get(3)?,
                content: row.get(4)?,
                content_hash: row.get(5)?,
                symbol_name: row.get(6)?,
                symbol_kind: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let chunks: Result<Vec<CodeChunk>, _> = chunk_iter.collect();
        chunks?
    };

    if chunks.is_empty() {
        eprintln!("No code chunks found in database");
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Handle JSON output
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(chunks, &exec_id);
        output_json(&json_response, output_format)?;
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Human output
    println!("// {} code chunks in database", chunks.len());
    println!();

    for chunk in chunks {
        let symbol = chunk.symbol_name.as_deref().unwrap_or("<unnamed>");
        let kind = chunk.symbol_kind.as_deref().unwrap_or("?");
        let preview: String = chunk
            .content
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();

        println!(
            "{}: {} ({}) [{}-{}]",
            chunk.file_path, symbol, kind, chunk.byte_start, chunk.byte_end
        );
        println!("  {}", preview);
        println!();
    }

    backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

/// Get a code chunk by file path and byte range.
///
/// Usage: magellan chunk-by-span --db <FILE> --file <PATH> --start <N> --end <N> [--output FORMAT]
pub fn run_chunk_by_span(
    db_path: PathBuf,
    file_path: String,
    byte_start: usize,
    byte_end: usize,
    output_format: OutputFormat,
) -> Result<()> {
    // Build args for execution tracking
    let args = vec![
        "chunk-by-span".to_string(),
        "--db".to_string(),
        db_path.to_string_lossy().to_string(),
        "--file".to_string(),
        file_path.clone(),
        "--start".to_string(),
        byte_start.to_string(),
        "--end".to_string(),
        byte_end.to_string(),
    ];

    let backend = MagellanBackend::open(&db_path)?;
    let exec_id = generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    backend.start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    let chunk = backend.get_code_chunk_by_span(&file_path, byte_start, byte_end)?;

    let chunk = match chunk {
        Some(c) => c,
        None => {
            eprintln!(
                "No code chunk found at {}:{}-{}",
                file_path, byte_start, byte_end
            );
            backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
            return Ok(());
        }
    };

    // Handle JSON output
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(chunk, &exec_id);
        output_json(&json_response, output_format)?;
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Human output
    let symbol = chunk.symbol_name.as_deref().unwrap_or("<unnamed>");
    let kind = chunk.symbol_kind.as_deref().unwrap_or("?");

    println!("// File: {}", chunk.file_path);
    println!("// Symbol: {} ({})", symbol, kind);
    println!("// Bytes: {}-{}", chunk.byte_start, chunk.byte_end);
    println!("// Hash: {}", chunk.content_hash);
    println!();
    println!("{}", chunk.content);

    backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

/// Get all code chunks for a symbol name (global search across files).
///
/// Usage: magellan chunk-by-symbol --db <FILE> --symbol <NAME> [--file PATTERN] [--output FORMAT]
pub fn run_chunk_by_symbol(
    db_path: PathBuf,
    symbol_name: String,
    output_format: OutputFormat,
    file_filter: Option<String>,
) -> Result<()> {
    // Build args for execution tracking
    let mut args = vec![
        "chunk-by-symbol".to_string(),
        "--db".to_string(),
        db_path.to_string_lossy().to_string(),
        "--symbol".to_string(),
        symbol_name.clone(),
    ];
    if let Some(ref file) = file_filter {
        args.push("--file".to_string());
        args.push(file.clone());
    }

    let backend = MagellanBackend::open(&db_path)?;
    let exec_id = generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    backend.start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    // For global symbol search, we need to use SQLite directly for now
    // Geo backend requires file_path for chunk lookup
    // TODO: Implement global chunk search in Geo backend
    let chunks = {
        use rusqlite::Connection;
        let conn = Connection::open(&db_path)?;

        let mut query = String::from(
            "SELECT id, file_path, byte_start, byte_end, content, content_hash, \
             symbol_name, symbol_kind, created_at \
             FROM code_chunks \
             WHERE symbol_name = ?1",
        );

        let mut params: Vec<String> = vec![symbol_name.clone()];

        if let Some(ref file_pattern) = file_filter {
            query.push_str(&format!(" AND file_path LIKE ?{}", params.len() + 1));
            params.push(format!("%{}%", file_pattern));
        }

        query.push_str(" ORDER BY file_path, byte_start");

        let mut stmt = conn.prepare(&query)?;

        // Build params as references for rusqlite
        let params_ref: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let chunk_iter = stmt.query_map(params_ref.as_slice(), |row| {
            Ok(CodeChunk {
                id: Some(row.get(0)?),
                file_path: row.get(1)?,
                byte_start: row.get(2)?,
                byte_end: row.get(3)?,
                content: row.get(4)?,
                content_hash: row.get(5)?,
                symbol_name: row.get(6)?,
                symbol_kind: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let chunks: Result<Vec<CodeChunk>, _> = chunk_iter.collect();
        chunks?
    };

    if chunks.is_empty() {
        eprintln!("No code chunks found for symbol '{}'", symbol_name);
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Handle JSON output
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(chunks, &exec_id);
        output_json(&json_response, output_format)?;
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Human output - group by file
    println!("// {} chunks for symbol '{}'", chunks.len(), symbol_name);
    println!();

    // Group chunks by file path
    let mut chunks_by_file: std::collections::HashMap<String, Vec<&CodeChunk>> =
        std::collections::HashMap::new();
    for chunk in &chunks {
        chunks_by_file
            .entry(chunk.file_path.clone())
            .or_default()
            .push(chunk);
    }

    for (file_path, file_chunks) in chunks_by_file.iter() {
        println!("// File: {}", file_path);
        for chunk in file_chunks {
            let kind = chunk.symbol_kind.as_deref().unwrap_or("?");
            println!("  {} [{}-{}]", kind, chunk.byte_start, chunk.byte_end);
            for line in chunk.content.lines() {
                println!("    {}", line);
            }
            println!();
        }
    }

    backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}
