//! Get command - Retrieve source code for symbols
//!
//! Usage: magellan get --db <FILE> --file <PATH> --symbol <NAME>

use anyhow::Result;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

// Use the library items through the magellan library
use magellan::common::detect_language_from_path;
use magellan::graph::query;
use magellan::generation::schema::CodeChunk;
use magellan::output::rich::{SpanChecksums, SpanContext};
use magellan::output::{output_json, JsonResponse, Span, SymbolMatch};
use magellan::{detect_backend_format, BackendFormat, ChunkStore, CodeGraph, generate_execution_id, OutputFormat};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// Response for get command with rich span data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResponse {
    /// Symbol details
    pub symbol: SymbolMatch,
    /// Source code content
    pub content: String,
}

/// Query chunks from either SQLite or Native-V2 backend
///
/// Detects the backend format and uses the appropriate query method:
/// - SQLite: Direct SQL query via rusqlite::Connection
/// - Native-V2: ChunkStore with KV backend (prefix scan)
fn query_chunks_from_db(
    db_path: &Path,
    file_filter: Option<&str>,
    kind_filter: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<CodeChunk>> {
    match detect_backend_format(db_path)? {
        BackendFormat::Sqlite => {
            // Use existing SQL query (lines 277-327)
            let conn = Connection::open(db_path)?;

            let mut query = String::from(
                "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                     symbol_name, symbol_kind, created_at
                     FROM code_chunks
                     WHERE 1=1"
            );

            let mut params: Vec<String> = Vec::new();

            if let Some(file_pattern) = file_filter {
                query.push_str(&format!(" AND file_path LIKE ?{}", params.len() + 1));
                params.push(format!("%{}%", file_pattern));
            }

            if let Some(kind) = kind_filter {
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

            let chunk_iter = stmt.query_map(
                params_ref.as_slice(),
                |row| {
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
                },
            )?;

            let chunks: Result<Vec<CodeChunk>, _> = chunk_iter.collect();
            chunks.map_err(|e| e.into())
        }
        BackendFormat::NativeV2 => {
            // Use ChunkStore with KV backend
            #[cfg(feature = "native-v2")]
            {
                use sqlitegraph::NativeGraphBackend;

                let backend = Rc::new(NativeGraphBackend::open(db_path)?);
                let chunks = ChunkStore::with_kv_backend(backend);

                // Get all chunks and filter in-memory (acceptable for small datasets)
                let mut all_chunks = chunks.get_all_chunks()?;

                // Apply filters
                if let Some(file_pattern) = file_filter {
                    all_chunks.retain(|c| c.file_path.contains(file_pattern));
                }

                if let Some(kind) = kind_filter {
                    all_chunks.retain(|c| c.symbol_kind.as_deref() == Some(kind));
                }

                // Apply limit
                if let Some(limit_val) = limit {
                    all_chunks.truncate(limit_val);
                }

                Ok(all_chunks)
            }
            #[cfg(not(feature = "native-v2"))]
            {
                Err(anyhow::anyhow!("Native V2 backend requires 'native-v2' feature"))
            }
        }
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
        eprintln!(
            "No code chunks found for symbol '{}' in file '{}'",
            symbol_name, file_path
        );
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Handle JSON output mode
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        // For JSON output, we need to get the symbol node to get span information
        // Then we can enrich it with rich span data
        let mut graph_mut = CodeGraph::open(&db_path)?;
        if let Ok(symbol_entries) = query::symbol_nodes_in_file_with_ids(&mut graph_mut, &file_path)
        {
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
                            let language = detect_language_from_path(
                                &symbol.file_path.to_string_lossy().to_string(),
                            );
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
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Human mode (existing behavior)
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

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
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
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
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

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
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

    // Backend-aware query (SQL for SQLite, KV for Native-V2)
    let chunks = query_chunks_from_db(&db_path, file_filter.as_deref(), kind_filter.as_deref(), limit)?;

    if chunks.is_empty() {
        eprintln!("No code chunks found in database");
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Handle JSON output
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(chunks, &exec_id);
        output_json(&json_response, output_format)?;
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
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

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
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

    let chunk = graph.get_code_chunk_by_span(&file_path, byte_start, byte_end)?;

    let chunk = match chunk {
        Some(c) => c,
        None => {
            eprintln!(
                "No code chunk found at {}:{}-{}",
                file_path, byte_start, byte_end
            );
            graph
                .execution_log()
                .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
            return Ok(());
        }
    };

    // Handle JSON output
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(chunk, &exec_id);
        output_json(&json_response, output_format)?;
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
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

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
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

    // Direct SQL query - search globally across all files
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
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    let chunk_iter = stmt.query_map(
        params_ref.as_slice(),
        |row| {
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
        },
    )?;

    let chunks: Result<Vec<CodeChunk>, _> = chunk_iter.collect();
    let chunks = chunks?;

    if chunks.is_empty() {
        eprintln!("No code chunks found for symbol '{}'", symbol_name);
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    // Handle JSON output
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(chunks, &exec_id);
        output_json(&json_response, output_format)?;
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
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

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "native-v2")]
    use magellan::generation::schema::CodeChunk;
    #[cfg(feature = "native-v2")]
    use sqlitegraph::{NativeGraphBackend, GraphBackend};
    #[cfg(feature = "native-v2")]
    use std::rc::Rc;
    #[cfg(feature = "native-v2")]
    use tempfile::TempDir;

    /// Test: magellan chunks command works on Native-V2 backend
    ///
    /// This test verifies that the CLI command can retrieve chunks from a
    /// Native-V2 KV backend. The query_chunks_from_db() function should use
    /// ChunkStore::get_all_chunks() which has KV support (line 749 in generation/mod.rs).
    #[cfg(feature = "native-v2")]
    #[test]
    fn test_magellan_chunks_command() {
        let temp_dir = TempDir::new().unwrap();
        let native_db = temp_dir.path().join("test_native.db");

        // Create Native-V2 backend
        let backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
        let chunks_store = ChunkStore::with_kv_backend(backend);

        // Add test chunks
        let chunks = vec![
            CodeChunk::new(
                "test.rs".to_string(),
                0,
                15,
                "fn test1() {}".to_string(),
                Some("test1".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "test.rs".to_string(),
                15,
                30,
                "fn test2() {}".to_string(),
                Some("test2".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "test.rs".to_string(),
                30,
                50,
                "struct TestStruct {}".to_string(),
                Some("TestStruct".to_string()),
                Some("Struct".to_string()),
            ),
        ];

        for chunk in &chunks {
            chunks_store.store_chunk(chunk).unwrap();
        }

        // Run magellan chunks command on Native-V2 backend
        // Should succeed without errors
        let result = run_chunks(
            native_db.clone(),
            OutputFormat::Human,
            None,  // no limit
            None,  // no file filter
            None,  // no kind filter
        );

        assert!(result.is_ok(), "magellan chunks should succeed on Native-V2: {:?}", result.err());
    }

    /// Test: magellan chunks --file filter works on Native-V2 backend
    ///
    /// This test verifies that the --file filter correctly filters chunks by
    /// file path pattern on Native-V2 backend. The in-memory filtering in
    /// query_chunks_from_db() (lines 106-107) should handle this.
    #[cfg(feature = "native-v2")]
    #[test]
    fn test_magellan_chunks_with_file_filter() {
        let temp_dir = TempDir::new().unwrap();
        let native_db = temp_dir.path().join("test_native.db");

        let backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
        let chunks_store = ChunkStore::with_kv_backend(backend);

        // Add chunks from different files
        let chunks = vec![
            CodeChunk::new(
                "src/main.rs".to_string(),
                0,
                20,
                "fn main() {}".to_string(),
                Some("main".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "src/lib.rs".to_string(),
                0,
                20,
                "fn helper() {}".to_string(),
                Some("helper".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "tests/test.rs".to_string(),
                0,
                20,
                "fn test() {}".to_string(),
                Some("test".to_string()),
                Some("Function".to_string()),
            ),
        ];

        for chunk in &chunks {
            chunks_store.store_chunk(chunk).unwrap();
        }

        // Test --file filter with "main" pattern
        // Should match src/main.rs
        let result = run_chunks(
            native_db.clone(),
            OutputFormat::Human,
            None,
            Some("main".to_string()),  // --file main
            None,
        );

        assert!(result.is_ok(), "--file filter should work on Native-V2: {:?}", result.err());
    }

    /// Test: magellan chunks --kind filter works on Native-V2 backend
    ///
    /// This test verifies that the --kind filter correctly filters chunks by
    /// symbol kind on Native-V2 backend. The in-memory filtering in
    /// query_chunks_from_db() (lines 110-112) should handle this.
    #[cfg(feature = "native-v2")]
    #[test]
    fn test_magellan_chunks_with_kind_filter() {
        let temp_dir = TempDir::new().unwrap();
        let native_db = temp_dir.path().join("test_native.db");

        let backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
        let chunks_store = ChunkStore::with_kv_backend(backend);

        // Add chunks with different kinds
        let chunks = vec![
            CodeChunk::new(
                "test.rs".to_string(),
                0,
                20,
                "fn function1() {}".to_string(),
                Some("function1".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "test.rs".to_string(),
                20,
                40,
                "fn function2() {}".to_string(),
                Some("function2".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "test.rs".to_string(),
                40,
                60,
                "struct MyStruct {}".to_string(),
                Some("MyStruct".to_string()),
                Some("Struct".to_string()),
            ),
        ];

        for chunk in &chunks {
            chunks_store.store_chunk(chunk).unwrap();
        }

        // Test --kind Function filter
        // Should only return Function kind chunks
        let result = run_chunks(
            native_db.clone(),
            OutputFormat::Human,
            None,
            None,
            Some("Function".to_string()),  // --kind Function
        );

        assert!(result.is_ok(), "--kind filter should work on Native-V2: {:?}", result.err());
    }

    /// Test: magellan chunks --limit works on Native-V2 backend
    ///
    /// This test verifies that the --limit option correctly limits the number
    /// of chunks returned on Native-V2 backend. The in-memory filtering in
    /// query_chunks_from_db() (lines 115-117) should handle this.
    #[cfg(feature = "native-v2")]
    #[test]
    fn test_magellan_chunks_with_limit() {
        let temp_dir = TempDir::new().unwrap();
        let native_db = temp_dir.path().join("test_native.db");

        let backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
        let chunks_store = ChunkStore::with_kv_backend(backend);

        // Add multiple chunks
        let chunks = vec![
            CodeChunk::new(
                "test.rs".to_string(),
                0,
                20,
                "fn chunk1() {}".to_string(),
                Some("chunk1".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "test.rs".to_string(),
                20,
                40,
                "fn chunk2() {}".to_string(),
                Some("chunk2".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "test.rs".to_string(),
                40,
                60,
                "fn chunk3() {}".to_string(),
                Some("chunk3".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "test.rs".to_string(),
                60,
                80,
                "fn chunk4() {}".to_string(),
                Some("chunk4".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "test.rs".to_string(),
                80,
                100,
                "fn chunk5() {}".to_string(),
                Some("chunk5".to_string()),
                Some("Function".to_string()),
            ),
        ];

        for chunk in &chunks {
            chunks_store.store_chunk(chunk).unwrap();
        }

        // Test --limit 2
        // Should only return 2 chunks
        let result = run_chunks(
            native_db.clone(),
            OutputFormat::Human,
            Some(2),  // --limit 2
            None,
            None,
        );

        assert!(result.is_ok(), "--limit option should work on Native-V2: {:?}", result.err());
    }

    /// Test: magellan chunks combined filters work on Native-V2 backend
    ///
    /// This test verifies that multiple filters (--file, --kind, --limit) work
    /// correctly together on Native-V2 backend.
    #[cfg(feature = "native-v2")]
    #[test]
    fn test_magellan_chunks_combined_filters() {
        let temp_dir = TempDir::new().unwrap();
        let native_db = temp_dir.path().join("test_native.db");

        let backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
        let chunks_store = ChunkStore::with_kv_backend(backend);

        // Add chunks with different files and kinds
        let chunks = vec![
            CodeChunk::new(
                "src/main.rs".to_string(),
                0,
                20,
                "fn main() {}".to_string(),
                Some("main".to_string()),
                Some("Function".to_string()),
            ),
            CodeChunk::new(
                "src/main.rs".to_string(),
                20,
                40,
                "struct Config {}".to_string(),
                Some("Config".to_string()),
                Some("Struct".to_string()),
            ),
            CodeChunk::new(
                "src/lib.rs".to_string(),
                0,
                20,
                "fn helper() {}".to_string(),
                Some("helper".to_string()),
                Some("Function".to_string()),
            ),
        ];

        for chunk in &chunks {
            chunks_store.store_chunk(chunk).unwrap();
        }

        // Test combined filters: --file main --kind Function --limit 1
        // Should return only the main function from src/main.rs
        let result = run_chunks(
            native_db.clone(),
            OutputFormat::Human,
            Some(1),  // --limit 1
            Some("main".to_string()),  // --file main
            Some("Function".to_string()),  // --kind Function
        );

        assert!(result.is_ok(), "Combined filters should work on Native-V2: {:?}", result.err());
    }
}
