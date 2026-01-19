//! Core graph operations for CodeGraph
//!
//! Provides file indexing and deletion operations.

use anyhow::Result;
use rusqlite::TransactionBehavior;
use std::path::{Path, PathBuf};

use sqlitegraph::GraphBackend;

use super::CodeGraph;
use super::query;

/// Deterministic reconcile outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    Deleted,
    Unchanged,
    Reindexed {
        symbols: usize,
        references: usize,
        calls: usize,
    },
}

/// Index a file into the graph (idempotent)
///
/// # Behavior
/// 1. Compute SHA-256 hash of file contents
/// 2. Upsert File node with path and hash
/// 3. DELETE all existing Symbol nodes and DEFINES edges for this file
/// 4. Detect language and parse symbols from source code
/// 5. Insert new Symbol nodes
/// 6. Create DEFINES edges from File to each Symbol
/// 7. Extract and store code chunks for each symbol
/// 8. Index calls (CALLS edges)
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path
/// * `source` - File contents as bytes
///
/// # Returns
/// Number of symbols indexed
pub fn index_file(graph: &mut CodeGraph, path: &str, source: &[u8]) -> Result<usize> {
    use crate::generation::CodeChunk;
    use crate::ingest::c::CParser;
    use crate::ingest::cpp::CppParser;
    use crate::ingest::java::JavaParser;
    use crate::ingest::javascript::JavaScriptParser;
    use crate::ingest::python::PythonParser;
    use crate::ingest::typescript::TypeScriptParser;
    use crate::ingest::{detect::Language, detect_language, Parser};

    let hash = graph.files.compute_hash(source);

    // Step 1: Find or create file node
    let file_id = graph.files.find_or_create_file_node(path, &hash)?;

    // Step 2: Delete all existing symbols for this file
    graph.symbols.delete_file_symbols(file_id)?;

    // Step 3: Detect language and parse symbols from source
    let path_buf = PathBuf::from(path);
    let language = detect_language(&path_buf);

    let symbol_facts = match language {
        Some(Language::Python) => {
            // Use Python parser
            let mut parser = PythonParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::Rust) => {
            // Use Rust parser
            let mut parser = Parser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::C) => {
            // Use C parser
            let mut parser = CParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::Cpp) => {
            // Use C++ parser
            let mut parser = CppParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::Java) => {
            // Use Java parser
            let mut parser = JavaParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::JavaScript) => {
            // Use JavaScript parser
            let mut parser = JavaScriptParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::TypeScript) => {
            // Use TypeScript parser
            let mut parser = TypeScriptParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        // Unknown language — return empty
        _ => Vec::new(),
    };

    // Step 4: Insert new symbol nodes and DEFINES edges
    for fact in &symbol_facts {
        let symbol_id = graph.symbols.insert_symbol_node(fact)?;
        graph.symbols.insert_defines_edge(file_id, symbol_id)?;
    }

    // Step 5: Extract and store code chunks for each symbol
    let source_str = std::str::from_utf8(source)
        .map_err(|e| anyhow::anyhow!("Source is not valid UTF-8: {}", e))?;

    let mut code_chunks = Vec::new();
    for fact in &symbol_facts {
        // Extract source content for this symbol's byte span
        if fact.byte_end <= source_str.len() {
            let content = source_str[fact.byte_start..fact.byte_end].to_string();

            let chunk = CodeChunk::new(
                path.to_string(),
                fact.byte_start,
                fact.byte_end,
                content,
                fact.name.clone(),
                Some(fact.kind_normalized.clone()),
            );

            code_chunks.push(chunk);
        }
    }

    // Store all code chunks in a single transaction
    if !code_chunks.is_empty() {
        let _ = graph.store_code_chunks(&code_chunks);
    }

    // Step 6: Index calls (all supported languages)
    if language.is_some() {
        let _ = super::calls::index_calls(graph, path, source);
    }

    Ok(symbol_facts.len())
}

/// Delete a file and all derived data from the graph
///
/// # Behavior
/// 1. Find File node by path
/// 2. Delete all DEFINES edges from File
/// 3. Delete all Symbol nodes that were defined by this File
/// 4. Delete all code chunks for this file
/// 5. Delete the File node itself
/// 6. Remove from in-memory index
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path to delete
pub fn delete_file(graph: &mut CodeGraph, path: &str) -> Result<()> {
    // Delegate to the authoritative deletion path.
    delete_file_facts(graph, path)
}

/// Delete ALL facts derived from a file path.
///
/// Semantics:
/// - Deletes Symbols defined by the file (via File -> DEFINES), plus edges touching those entities
/// - Deletes Reference nodes whose persisted file_path or embedded `ReferenceNode.file` matches
/// - Deletes Call nodes whose persisted file_path or embedded `CallNode.file` matches
/// - Deletes code chunks for the file
/// - Deletes the File node itself and removes it from in-memory index
///
/// Determinism:
/// - Any multi-entity deletion gathers candidate IDs, sorts ascending, deletes in that order.
/// - All deletions occur within an IMMEDIATE transaction for atomicity.
pub fn delete_file_facts(graph: &mut CodeGraph, path: &str) -> Result<()> {
    use crate::graph::schema::delete_edges_touching_entities;

    let mut conn = graph.chunks.connect()?;

    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| anyhow::anyhow!("Failed to start delete transaction: {}", e))?;

    // We'll gather all entity IDs we intend to delete and perform edge cleanup for them.
    let mut deleted_entity_ids: Vec<i64> = Vec::new();

    // 1) Symbols defined by this file (if file exists).
    if let Some(file_id) = graph.files.find_file_node(path)? {
        // Capture symbol IDs before deletion.
        let symbol_ids = graph.files.backend.neighbors(
            file_id.as_i64(),
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: Some("DEFINES".to_string()),
            },
        )?;

        let mut symbol_ids_sorted = symbol_ids;
        symbol_ids_sorted.sort_unstable();

        // Delete each symbol node (sqlitegraph deletes edges touching entity).
        for symbol_id in &symbol_ids_sorted {
            graph.files.backend.graph().delete_entity(*symbol_id)?;
        }

        deleted_entity_ids.extend(symbol_ids_sorted);

        // 4) Delete code chunks for this file.
        let _ = graph.chunks.delete_chunks_for_file(path);

        // 5) Delete the File node itself.
        graph
            .files
            .backend
            .graph()
            .delete_entity(file_id.as_i64())?;
        deleted_entity_ids.push(file_id.as_i64());

        // 2) References in this file.
        let reference_deleted = graph.references.delete_references_in_file(path)?;

        // 3) Calls in this file.
        let call_deleted = graph.calls.delete_calls_in_file(path)?;

        // Explicit edge cleanup for deleted IDs (symbols + file) to ensure no rows remain.
        deleted_entity_ids.sort_unstable();
        deleted_entity_ids.dedup();
        let _ = delete_edges_touching_entities(&tx, &deleted_entity_ids)?;

        // Silence unused warnings for counts (useful for debugging, kept deterministic).
        let _ = (reference_deleted, call_deleted);
    } else {
        // Even if there's no File node, we still delete chunks for this path string.
        let _ = graph.chunks.delete_chunks_for_file(path);

        // 2) References in this file.
        let reference_deleted = graph.references.delete_references_in_file(path)?;

        // 3) Calls in this file.
        let call_deleted = graph.calls.delete_calls_in_file(path)?;

        // Silence unused warnings for counts (useful for debugging, kept deterministic).
        let _ = (reference_deleted, call_deleted);
    }

    tx.commit()
        .map_err(|e| anyhow::anyhow!("Failed to commit delete transaction: {}", e))?;

    // Remove from in-memory index AFTER successful commit.
    graph.files.file_index.remove(path);

    Ok(())
}

/// Reconcile a file path against filesystem + content hash.
///
/// This is the deterministic primitive used by scan and watcher updates.
/// Behavior:
/// 1. If file doesn't exist → delete all facts, return Deleted
/// 2. If exists → compute hash, compare to stored
/// 3. If unchanged → return Unchanged without mutating DB
/// 4. If changed/new → delete facts, re-index, return Reindexed
pub fn reconcile_file_path(
    graph: &mut CodeGraph,
    path: &Path,
    path_key: &str,
) -> Result<ReconcileOutcome> {
    use std::fs;

    // 1) Check if file exists on filesystem
    if !path.exists() {
        delete_file_facts(graph, path_key)?;
        return Ok(ReconcileOutcome::Deleted);
    }

    // 2) Read file and compute hash
    let source = fs::read(path)?;
    let new_hash = graph.files.compute_hash(&source);

    // 3) Check if hash matches stored file node
    let unchanged = if let Some(file_id) = graph.files.find_file_node(path_key)? {
        let node = graph.files.backend.get_node(file_id.as_i64())?;
        let file_node: crate::graph::schema::FileNode =
            serde_json::from_value(node.data).unwrap_or_else(|_| crate::graph::schema::FileNode {
                path: path_key.to_string(),
                hash: String::new(),
                last_indexed_at: 0,
                last_modified: 0,
            });
        file_node.hash == new_hash
    } else {
        false // File doesn't exist in DB, needs to be indexed
    };

    // 4) If unchanged, skip reindexing
    if unchanged {
        return Ok(ReconcileOutcome::Unchanged);
    }

    // 5) Delete all existing facts for this file, then re-index
    delete_file_facts(graph, path_key)?;

    let symbols = index_file(graph, path_key, &source)?;
    query::index_references(graph, path_key, &source)?;

    // Count calls (index_file already indexed calls internally)
    let calls = graph
        .calls
        .backend
        .entity_ids()?
        .into_iter()
        .filter(|id| {
            graph
                .calls
                .backend
                .get_node(*id)
                .map(|n| n.kind == "Call")
                .unwrap_or(false)
        })
        .count();

    let references = graph
        .references
        .backend
        .entity_ids()?
        .into_iter()
        .filter(|id| {
            graph
                .references
                .backend
                .get_node(*id)
                .map(|n| n.kind == "Reference")
                .unwrap_or(false)
        })
        .count();

    Ok(ReconcileOutcome::Reindexed {
        symbols,
        references,
        calls,
    })
}
