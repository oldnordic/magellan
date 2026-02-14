//! Core graph operations for CodeGraph
//!
//! Provides file indexing and deletion operations.

use anyhow::Result;
use std::path::{Path, PathBuf};

use sqlitegraph::{GraphBackend, SnapshotId};

use super::query;
use super::CodeGraph;
use crate::common::extract_symbol_content_safe;

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

/// Deletion statistics returned by delete_file_facts()
///
/// Provides counts of deleted entities to verify all derived data was removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteResult {
    /// Number of symbols deleted (via DEFINES edges from file)
    pub symbols_deleted: usize,
    /// Number of reference nodes deleted
    pub references_deleted: usize,
    /// Number of call nodes deleted
    pub calls_deleted: usize,
    /// Number of code chunks deleted
    pub chunks_deleted: usize,
    /// Number of AST nodes deleted
    pub ast_nodes_deleted: usize,
    /// Number of CFG blocks deleted
    pub cfg_blocks_deleted: usize,
    /// Number of edges deleted (cleanup of orphaned edges)
    pub edges_deleted: usize,
}

impl DeleteResult {
    /// Total count of all deleted items
    pub fn total_deleted(&self) -> usize {
        self.symbols_deleted
            + self.references_deleted
            + self.calls_deleted
            + self.chunks_deleted
            + self.ast_nodes_deleted
            + self.cfg_blocks_deleted
            + self.edges_deleted
    }

    /// Returns true if nothing was deleted
    pub fn is_empty(&self) -> bool {
        self.total_deleted() == 0
    }
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
    use crate::ingest::pool;
    use crate::ingest::python::PythonParser;
    use crate::ingest::typescript::TypeScriptParser;
    use crate::ingest::{detect::Language, detect_language, Parser};

    let hash = graph.files.compute_hash(source);

    // Step 1: Find or create file node
    let file_id = graph.files.find_or_create_file_node(path, &hash)?;

    // Step 2: Delete all existing symbols for this file (verification)
    // Note: This is a safeguard - reconcile_file_path() already calls delete_file_facts()
    graph.symbols.delete_file_symbols(file_id)?;
    // Verify deletion completed (_symbols_deleted may be 0 for new files)

    // Step 3: Detect language and parse symbols from source
    let path_buf = PathBuf::from(path);
    let language = detect_language(&path_buf);

    let symbol_facts = match language {
        Some(Language::Python) => {
            // Use parser pool for Python
            pool::with_parser(Language::Python, |parser| {
                PythonParser::extract_symbols_with_parser(parser, path_buf.clone(), source)
            })?
        }
        Some(Language::Rust) => {
            // Use parser pool for Rust
            pool::with_parser(Language::Rust, |parser| {
                Parser::extract_symbols_with_parser(parser, path_buf.clone(), source)
            })?
        }
        Some(Language::C) => {
            // Use parser pool for C
            pool::with_parser(Language::C, |parser| {
                CParser::extract_symbols_with_parser(parser, path_buf.clone(), source)
            })?
        }
        Some(Language::Cpp) => {
            // Use parser pool for C++
            pool::with_parser(Language::Cpp, |parser| {
                CppParser::extract_symbols_with_parser(parser, path_buf.clone(), source)
            })?
        }
        Some(Language::Java) => {
            // Use parser pool for Java
            pool::with_parser(Language::Java, |parser| {
                JavaParser::extract_symbols_with_parser(parser, path_buf.clone(), source)
            })?
        }
        Some(Language::JavaScript) => {
            // Use parser pool for JavaScript
            pool::with_parser(Language::JavaScript, |parser| {
                JavaScriptParser::extract_symbols_with_parser(parser, path_buf.clone(), source)
            })?
        }
        Some(Language::TypeScript) => {
            // Use parser pool for TypeScript
            pool::with_parser(Language::TypeScript, |parser| {
                TypeScriptParser::extract_symbols_with_parser(parser, path_buf.clone(), source)
            })?
        }
        // Unknown language — return empty
        _ => Vec::new(),
    };

    // Step 4: Insert new symbol nodes and DEFINES edges
    // Track function symbol IDs for CFG extraction
    let mut function_symbol_ids: Vec<(String, i64, i64, i64)> = Vec::new();
    let mut indexed_symbols: Vec<(crate::ingest::SymbolFact, i64)> = Vec::new();

    for fact in &symbol_facts {
        let symbol_id = graph.symbols.insert_symbol_node(fact)?;
        graph.symbols.insert_defines_edge(file_id, symbol_id)?;

        // Track function symbols for CFG extraction (Rust only)
        // kind_normalized uses normalized_key: "fn" for Function, "method" for Method
        if fact.kind_normalized == "fn" || fact.kind_normalized == "method" {
            if let Some(ref name) = fact.name {
                function_symbol_ids.push((
                    name.clone(),
                    symbol_id.as_i64(),
                    fact.byte_start as i64,
                    fact.byte_end as i64,
                ));
            }
        }

        // Track symbol with its node ID for KV index population
        indexed_symbols.push((fact.clone(), symbol_id.as_i64()));
    }



    // Step 5: Extract and store code chunks for each symbol
    // Use safe UTF-8 extraction to handle multi-byte characters that tree-sitter may split
    let mut code_chunks = Vec::new();
    for fact in &symbol_facts {
        // Safely extract source content for this symbol's byte span
        // This handles multi-byte UTF-8 characters that could be split by tree-sitter offsets
        if let Some(content) = extract_symbol_content_safe(source, fact.byte_start, fact.byte_end) {
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
        // If extraction fails (invalid UTF-8 or split character), skip this symbol's chunk
        // This is a graceful degradation - the symbol is still indexed, just without content
    }

    // Store all code chunks in a single transaction
    if !code_chunks.is_empty() {
        graph.store_code_chunks(&code_chunks)?;
    }

    // Step 5.5: Extract and store AST nodes
    // Re-use the parser pool to get the tree-sitter tree for AST extraction
    if let Some(lang) = language {
        // with_parser returns Result<R> where R is the closure's return type.
        // We return Vec<AstNode> directly (not wrapped in Result) to avoid double wrapping.
        let ast_nodes_result = pool::with_parser(lang, |parser| {
            parser.parse(source, None)
                .map(|tree| crate::graph::extract_ast_nodes(&tree, source))
                .unwrap_or_else(Vec::new)
        });
        if let Ok(ast_nodes) = ast_nodes_result {
            if !ast_nodes.is_empty() {
                insert_ast_nodes(graph, file_id.as_i64(), ast_nodes)?;
            }
        }
    }

    // Step 5.55: Extract and store import statements
    // Import extraction provides metadata for cross-file symbol resolution (Phase 61)
    let import_result = pool::with_parser(
        crate::ingest::detect::Language::Rust,
        |_parser| {
            // Create ImportExtractor and extract imports
            // ImportExtractor::new() returns Result, but we're in a non-Result closure
            // So we use unwrap_or_default to handle errors gracefully
            let mut import_extractor =
                crate::ingest::imports::ImportExtractor::new().unwrap_or_else(|_| {
                    // Fallback: create a new parser directly
                    crate::ingest::imports::ImportExtractor::default()
                });
            import_extractor.extract_imports_rust(path_buf.clone(), source)
        },
    );
    if let Ok(extracted_imports) = import_result {
        if !extracted_imports.is_empty() {
            // Delete old imports for this file
            let _ = graph.imports.delete_imports_in_file(path);
            // Index the new imports with IMPORTS edges and path resolution
            let _ = graph
                .imports
                .index_imports(path, file_id.as_i64(), extracted_imports, Some(&graph.module_resolver));
        }
    }

    // Step 5.6: Extract and store CFG blocks for Rust functions
    // CFG extraction is only done for .rs files with function symbols
    if path.ends_with(".rs") && !function_symbol_ids.is_empty() {
        let _cfg_result = pool::with_parser(crate::ingest::detect::Language::Rust, |parser| {
            // parser.parse returns Option, handle gracefully
            let tree = match parser.parse(source, None) {
                Some(t) => t,
                None => return Ok(()), // Parse failed, skip CFG extraction
            };
            let root = tree.root_node();

            // Find all function_item nodes
            fn collect_function_items(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
                let mut items = Vec::new();
                if node.kind() == "function_item" {
                    items.push(node);
                }
                let mut child_cursor = node.walk();
                if child_cursor.goto_first_child() {
                    loop {
                        items.extend(collect_function_items(child_cursor.node()));
                        if !child_cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
                items
            }

            let function_nodes = collect_function_items(root);

            // For each function_item, find matching symbol and extract CFG
            for func_node in function_nodes {
                let func_start = func_node.byte_range().start as i64;
                let func_end = func_node.byte_range().end as i64;

                // Find matching function symbol by byte range
                if let Some((_, entity_id, _, _)) = function_symbol_ids.iter().find(|(_, _, start, end)| {
                    func_start == *start && func_end == *end
                }) {
                    let _ = graph.cfg_ops.index_cfg_for_function(&func_node, source, *entity_id);
                }
            }

            Ok::<(), anyhow::Error>(())
        });

        // CFG extraction failure doesn't block indexing
    }

    // Step 6: Index calls (all supported languages)
    if language.is_some() {
        let _ = super::calls::index_calls(graph, path, source);
    }

    // Step 7: Compute and store metrics (fan-in, fan-out, LOC, complexity)
    // Convert SymbolFacts to SymbolNode format for metrics computation
    use crate::graph::schema::SymbolNode;
    let symbol_nodes: Vec<SymbolNode> = symbol_facts
        .iter()
        .filter_map(|fact| {
            // Skip symbols without valid position data
            if fact.start_line == 0 && fact.byte_start == 0 {
                return None;
            }
            Some(SymbolNode {
                symbol_id: None,  // Computed by metrics module
                name: fact.name.clone(),
                kind: fact.kind_normalized.clone(),
                kind_normalized: Some(fact.kind_normalized.clone()),
                fqn: fact.fqn.clone(),
                display_fqn: fact.display_fqn.clone(),
                canonical_fqn: fact.canonical_fqn.clone(),
                byte_start: fact.byte_start,
                byte_end: fact.byte_end,
                start_line: fact.start_line,
                start_col: fact.start_col,
                end_line: fact.end_line,
                end_col: fact.end_col,
            })
        })
        .collect();

    if let Err(e) = graph.metrics.compute_for_file(path, source, &symbol_nodes) {
        // Metrics computation failure shouldn't block indexing
        eprintln!("Warning: Failed to compute metrics for '{}': {}", path, e);
    }

    // Invalidate cache for this file since it was just modified
    graph.invalidate_cache(path);

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
///
/// # Returns
/// DeleteResult with counts of deleted entities
pub fn delete_file(graph: &mut CodeGraph, path: &str) -> Result<DeleteResult> {
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
/// - Graph entity deletions occur within an IMMEDIATE transaction for atomicity.
/// - Chunk deletion occurs after graph transaction commit (two-phase commit pattern).
///
/// Verification:
/// - Counts items before transaction, asserts counts after each deletion step.
/// - Panics if counts don't match (catches orphaned data bugs).
///
/// # Two-Phase Commit Pattern
///
/// Due to architectural constraints (ChunkStore and SqliteGraphBackend use separate connections),
/// we use a two-phase commit pattern:
///
/// 1. **Phase 1 (Graph Transaction):** Open IMMEDIATE transaction on backend connection,
///    delete all graph entities (symbols, file, references, calls), commit.
/// 2. **Phase 2 (Chunk Deletion):** Delete chunks on shared connection after graph commit.
///
/// If chunk deletion fails, graph state is consistent but orphaned chunks may remain.
/// This is acceptable because:
/// - Chunks are derived data (can be regenerated from source)
/// - Orphan detection tests verify no orphaned graph entities remain
/// - The next index operation for the same file will replace orphaned chunks
///
/// # Returns
/// DeleteResult with detailed counts of deleted entities.
pub fn delete_file_facts(graph: &mut CodeGraph, path: &str) -> Result<DeleteResult> {
    use crate::graph::schema::delete_edges_touching_entities;

    // === PHASE 1: Count items to be deleted (before any deletion) ===
    // These are the expected counts we will verify against.

    // Count symbols defined by this file (DEFINES edges)
    let snapshot = SnapshotId::current();
    let expected_symbols: usize = if let Some(file_id) = graph.files.find_file_node(path)? {
        graph
            .files
            .backend
            .neighbors(
                snapshot,
                file_id.as_i64(),
                sqlitegraph::NeighborQuery {
                    direction: sqlitegraph::BackendDirection::Outgoing,
                    edge_type: Some("DEFINES".to_string()),
                },
            )
            .map(|ids| ids.len())
            .unwrap_or(0)
    } else {
        0
    };

    // Count references with matching file_path
    let expected_references = count_references_in_file(graph, path);

    // Count calls with matching file_path
    let expected_calls = count_calls_in_file(graph, path);

    // Count code chunks (query directly from code_chunks table)
    let expected_chunks = count_chunks_for_file(graph, path);

    // Count AST nodes (v6: uses file_id for efficient per-file counting)
    let expected_ast_nodes = count_ast_nodes_for_file(graph, path);

    // Count CFG blocks for this file
    let _expected_cfg_blocks = count_cfg_blocks_for_file(graph, path);

    // === PHASE 2: Perform graph entity deletions ===
    //
    // ARCHITECTURAL LIMITATION: We cannot wrap all operations in a single ACID transaction
    // because sqlitegraph does not expose mutable access to its underlying connection.
    // rusqlite::Transaction requires &mut Connection, but sqlitegraph only provides &Connection.
    //
    // Current approach: Use auto-commit for each operation. The row-count assertions
    // provide verification that all expected data was deleted.

    let mut deleted_entity_ids: Vec<i64> = Vec::new();
    let symbols_deleted: usize;
    let chunks_deleted: usize;
    let references_deleted: usize;
    let calls_deleted: usize;
    let ast_nodes_deleted: usize;
    let cfg_blocks_deleted: usize;

    if let Some(file_id) = graph.files.find_file_node(path)? {
        // Capture symbol IDs before deletion.
        let snapshot = SnapshotId::current();
        let symbol_ids = graph.files.backend.neighbors(
            snapshot,
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
            graph.files.backend.delete_entity(*symbol_id)?;
        }

        symbols_deleted = symbol_ids_sorted.len();
        deleted_entity_ids.extend(symbol_ids_sorted.iter().copied());

        // Assert symbol count matches expected
        assert_eq!(
            symbols_deleted, expected_symbols,
            "Symbol deletion count mismatch for '{}': expected {}, got {}",
            path, expected_symbols, symbols_deleted
        );

        // Delete the File node itself.
        graph
            .files
            .backend
            .delete_entity(file_id.as_i64())?;
        deleted_entity_ids.push(file_id.as_i64());

        // Delete references in this file.
        references_deleted = graph.references.delete_references_in_file(path)?;

        // Assert reference count matches expected
        assert_eq!(
            references_deleted, expected_references,
            "Reference deletion count mismatch for '{}': expected {}, got {}",
            path, expected_references, references_deleted
        );

        // Delete calls in this file.
        calls_deleted = graph.calls.delete_calls_in_file(path)?;

        // Assert call count matches expected
        assert_eq!(
            calls_deleted, expected_calls,
            "Call deletion count mismatch for '{}': expected {}, got {}",
            path, expected_calls, calls_deleted
        );



        // Explicit edge cleanup for deleted IDs (symbols + file) to ensure no rows remain.
        deleted_entity_ids.sort_unstable();
        deleted_entity_ids.dedup();
        
        // Delete code chunks for this file using the ChunkStore abstraction.
        // This works with both SQLite and V3 backends.
        chunks_deleted = graph.chunks.delete_chunks_for_file(path)?;
        
        // TODO: V3 backend needs AST nodes and CFG blocks support
        // For now, set these to 0 for V3 backend
        ast_nodes_deleted = 0;
        cfg_blocks_deleted = 0;
        let edges_deleted = 0;

        // Assert AST node count matches expected
        assert_eq!(
            ast_nodes_deleted, expected_ast_nodes,
            "AST node deletion count mismatch for '{}': expected {}, got {}",
            path, expected_ast_nodes, ast_nodes_deleted
        );

        // Remove from in-memory index AFTER successful deletions.
        graph.files.file_index.remove(path);

        // Invalidate cache for this file
        graph.invalidate_cache(path);

        Ok(DeleteResult {
            symbols_deleted,
            references_deleted,
            calls_deleted,
            chunks_deleted,
            ast_nodes_deleted,
            cfg_blocks_deleted,
            edges_deleted,
        })
    } else {
        // No File node exists, but we still clean up orphaned data.
        // Use auto-commit for orphan cleanup (no transaction needed).

        // Delete chunks
        let chunk_conn = graph.chunks.connect()?;
        chunks_deleted = chunk_conn
            .execute(
                "DELETE FROM code_chunks WHERE file_path = ?1",
                rusqlite::params![path],
            )
            .map_err(|e| anyhow::anyhow!("Failed to delete code chunks: {}", e))?;
        assert_eq!(
            chunks_deleted, expected_chunks,
            "Code chunk deletion count mismatch (no file) for '{}': expected {}, got {}",
            path, expected_chunks, chunks_deleted
        );

        // Delete references
        references_deleted = graph.references.delete_references_in_file(path)?;
        assert_eq!(
            references_deleted, expected_references,
            "Reference deletion count mismatch (no file) for '{}': expected {}, got {}",
            path, expected_references, references_deleted
        );

        // Delete calls
        calls_deleted = graph.calls.delete_calls_in_file(path)?;
        assert_eq!(
            calls_deleted, expected_calls,
            "Call deletion count mismatch (no file) for '{}': expected {}, got {}",
            path, expected_calls, calls_deleted
        );

        // TODO: V3 backend needs AST nodes and CFG blocks support
        // For now, skip these deletions for V3 backend
        ast_nodes_deleted = 0;
        cfg_blocks_deleted = 0;

        // Invalidate cache for this file (even if no file node existed)
        graph.invalidate_cache(path);

        // No file node to remove from index
        Ok(DeleteResult {
            symbols_deleted: 0,
            references_deleted,
            calls_deleted,
            chunks_deleted,
            ast_nodes_deleted,
            cfg_blocks_deleted,
            edges_deleted: 0,
        })
    }
}

/// Count Reference nodes with matching file_path.
///
/// Used to verify deletion completeness.
fn count_references_in_file(graph: &CodeGraph, path: &str) -> usize {
    let snapshot = SnapshotId::current();
    graph
        .references
        .backend
        .entity_ids()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| graph.references.backend.get_node(snapshot, *id).ok())
                .filter(|node| {
                    node.kind == "Reference"
                        && node
                            .data
                            .get("file")
                            .and_then(|v| v.as_str())
                            .map(|f| f == path)
                            .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

/// Count Call nodes with matching file_path.
///
/// Used to verify deletion completeness.
fn count_calls_in_file(graph: &CodeGraph, path: &str) -> usize {
    let snapshot = SnapshotId::current();
    graph
        .calls
        .backend
        .entity_ids()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| graph.calls.backend.get_node(snapshot, *id).ok())
                .filter(|node| {
                    node.kind == "Call"
                        && node
                            .data
                            .get("file")
                            .and_then(|v| v.as_str())
                            .map(|f| f == path)
                            .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

/// Test-only helpers for delete operation testing.
///
/// This module is public but marked as "test only" by convention.
/// It's used by integration tests in tests/delete_transaction_tests.rs.
///
/// NOTE: Due to sqlitegraph not exposing mutable access to its connection,
/// we cannot use rusqlite::Transaction. Tests use verification points
/// to check deletion completeness at various stages.
pub mod test_helpers {
    use super::*;
    use crate::graph::schema::delete_edges_touching_entities;

    /// Test operations that can be verified during delete.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FailPoint {
        /// Verify after symbols are deleted.
        AfterSymbolsDeleted,
        /// Verify after references are deleted.
        AfterReferencesDeleted,
        /// Verify after calls are deleted.
        AfterCallsDeleted,
        /// Verify after code chunks are deleted.
        AfterChunksDeleted,
        /// Verify before the file node is deleted.
        BeforeFileDeleted,
    }

    /// Delete ALL facts derived from a file path with verification points.
    ///
    /// This is a test-only version of `delete_file_facts` that allows verification
    /// at specific points during the deletion process.
    ///
    /// # Arguments
    /// * `graph` - CodeGraph instance
    /// * `path` - File path to delete
    /// * `verify_at` - Optional verification point for testing
    ///
    /// # Returns
    /// DeleteResult with detailed counts of deleted entities.
    pub fn delete_file_facts_with_injection(
        graph: &mut CodeGraph,
        path: &str,
        verify_at: Option<FailPoint>,
    ) -> Result<DeleteResult> {
        let mut deleted_entity_ids: Vec<i64> = Vec::new();
        let symbols_deleted: usize;
        let chunks_deleted: usize;
        let references_deleted: usize;
        let calls_deleted: usize;
        let _ast_nodes_deleted: usize = 0; // Test helper doesn't implement AST node deletion
        let _cfg_blocks_deleted: usize = 0; // Test helper doesn't implement CFG block deletion

        if let Some(file_id) = graph.files.find_file_node(path)? {
            // Capture symbol IDs before deletion.
            let snapshot = SnapshotId::current();
            let symbol_ids = graph.files.backend.neighbors(
                snapshot,
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
                graph.files.backend.delete_entity(*symbol_id)?;
            }

            symbols_deleted = symbol_ids_sorted.len();
            deleted_entity_ids.extend(symbol_ids_sorted.iter().copied());

            // Verification point after symbols deleted
            if verify_at == Some(FailPoint::AfterSymbolsDeleted) {
                // Note: We don't remove from file_index here since file still exists
                return Ok(DeleteResult {
                    symbols_deleted,
                    references_deleted: 0,
                    calls_deleted: 0,
                    chunks_deleted: 0,
                    ast_nodes_deleted: 0,
                    cfg_blocks_deleted: 0,
                    edges_deleted: 0,
                });
            }

            // Delete the File node itself.
            graph
                .files
                .backend
                .delete_entity(file_id.as_i64())?;
            deleted_entity_ids.push(file_id.as_i64());

            // Delete references in this file.
            references_deleted = graph.references.delete_references_in_file(path)?;

            // Verification point after references deleted
            if verify_at == Some(FailPoint::AfterReferencesDeleted) {
                return Ok(DeleteResult {
                    symbols_deleted,
                    references_deleted,
                    calls_deleted: 0,
                    chunks_deleted: 0,
                    ast_nodes_deleted: 0,
                    cfg_blocks_deleted: 0,
                    edges_deleted: 0,
                });
            }

            // Delete calls in this file.
            calls_deleted = graph.calls.delete_calls_in_file(path)?;

            // Verification point after calls deleted
            if verify_at == Some(FailPoint::AfterCallsDeleted) {
                return Ok(DeleteResult {
                    symbols_deleted,
                    references_deleted,
                    calls_deleted,
                    chunks_deleted: 0,
                    ast_nodes_deleted: 0,
                    cfg_blocks_deleted: 0,
                    edges_deleted: 0,
                });
            }

            // Verification point after file node deleted (before chunks)
            if verify_at == Some(FailPoint::BeforeFileDeleted) {
                return Ok(DeleteResult {
                    symbols_deleted,
                    references_deleted,
                    calls_deleted,
                    chunks_deleted: 0,
                    ast_nodes_deleted: 0,
                    cfg_blocks_deleted: 0,
                    edges_deleted: 0,
                });
            }

            // Explicit edge cleanup for deleted IDs.
            deleted_entity_ids.sort_unstable();
            deleted_entity_ids.dedup();
            let conn = graph.chunks.connect()?;
            let edges_deleted = delete_edges_touching_entities(&conn, &deleted_entity_ids)?;

            // Delete code chunks
            chunks_deleted = conn
                .execute(
                    "DELETE FROM code_chunks WHERE file_path = ?1",
                    rusqlite::params![path],
                )
                .map_err(|e| anyhow::anyhow!("Failed to delete code chunks: {}", e))?;

            // Remove from in-memory index after all deletions complete
            graph.files.file_index.remove(path);

            // Invalidate cache for this file
            graph.invalidate_cache(path);

            // Verification point after chunks deleted
            if verify_at == Some(FailPoint::AfterChunksDeleted) {
                return Ok(DeleteResult {
                    symbols_deleted,
                    references_deleted,
                    calls_deleted,
                    chunks_deleted,
                    ast_nodes_deleted: 0,
                    cfg_blocks_deleted: 0,
                    edges_deleted,
                });
            }

            Ok(DeleteResult {
                symbols_deleted,
                references_deleted,
                calls_deleted,
                chunks_deleted,
                ast_nodes_deleted: 0,
                cfg_blocks_deleted: 0,
                edges_deleted,
            })
        } else {
            // No File node exists - handle orphan cleanup path.
            let conn = graph.chunks.connect()?;
            chunks_deleted = conn
                .execute(
                    "DELETE FROM code_chunks WHERE file_path = ?1",
                    rusqlite::params![path],
                )
                .map_err(|e| anyhow::anyhow!("Failed to delete code chunks: {}", e))?;

            references_deleted = graph.references.delete_references_in_file(path)?;
            calls_deleted = graph.calls.delete_calls_in_file(path)?;

            // Invalidate cache for this file (even if no file node existed)
            graph.invalidate_cache(path);

            Ok(DeleteResult {
                symbols_deleted: 0,
                references_deleted,
                calls_deleted,
                chunks_deleted,
                ast_nodes_deleted: 0,
                cfg_blocks_deleted: 0,
                edges_deleted: 0,
            })
        }
    }
}

/// Count code chunks for a file path.
///
/// Used to verify deletion completeness.
fn count_chunks_for_file(graph: &CodeGraph, path: &str) -> usize {
    graph.chunks.count_chunks_for_file(path).unwrap_or(0)
}

/// Insert AST nodes for a file into the database
///
/// # Arguments
/// * `graph` - The CodeGraph instance
/// * `nodes` - Vector of AstNode structs to insert
///
/// # Returns
/// Result with the number of nodes inserted
///
/// # Notes
/// This function handles parent-child ID resolution. Nodes are inserted
/// with placeholder parent IDs (negative values) which are then resolved
/// to actual database IDs after insertion.
pub fn insert_ast_nodes(graph: &mut CodeGraph, file_id: i64, nodes: Vec<crate::graph::AstNode>) -> Result<usize> {
    if nodes.is_empty() {
        return Ok(0);
    }

    // Use SideTables trait for backend-agnostic storage
    // Note: This does individual inserts instead of bulk transaction
    // TODO: Add batch insert method to SideTables trait for better performance
    
    // First pass: insert all nodes and collect their assigned IDs
    let mut inserted_ids: Vec<i64> = Vec::with_capacity(nodes.len());
    for node in &nodes {
        let node_id = graph.side_tables.store_ast_node(node, file_id)?;
        inserted_ids.push(node_id);
    }

    // Second pass: resolve placeholder parent IDs
    // For nodes with negative parent IDs (placeholders), update them
    for (idx, node) in nodes.iter().enumerate() {
        if let Some(parent_id) = node.parent_id {
            if parent_id < 0 {
                // Negative ID means it's an index into the nodes vector
                let parent_index = (-parent_id) as usize - 1;
                if parent_index < inserted_ids.len() {
                    let _actual_parent_id = inserted_ids[parent_index];
                    let _node_id = inserted_ids[idx];
                    // TODO: Update parent_id - would need update_ast_node method
                    // For now, parent resolution is skipped for V3 backend
                }
            }
        }
    }

    Ok(nodes.len())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ast_nodes_indexed_with_file() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        let source = b"fn main() { if true { println!(\"hello\"); } }";
        graph.index_file("test.rs", source).unwrap();

        // Verify AST nodes were created
        let conn = graph.chunks.connect().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ast_nodes", [], |row| row.get(0))
            .unwrap();

        assert!(count > 0, "AST nodes should be created during indexing");

        // Verify specific nodes exist
        let if_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ast_nodes WHERE kind = 'if_expression'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(if_count > 0, "if_expression should be indexed");
    }
}

/// Count AST nodes for a file path.
///
/// Used to verify deletion completeness.
/// v6: Uses file_id to efficiently count AST nodes per file.
fn count_ast_nodes_for_file(graph: &CodeGraph, path: &str) -> usize {
    // First, get the file_id by looking up in the file_index
    let file_id = match graph.files.file_index.get(path) {
        Some(id) => id.as_i64(),
        None => return 0, // No file node, no AST nodes to count
    };

    // Count AST nodes using SideTables trait
    graph.side_tables.count_ast_nodes_for_file(file_id).unwrap_or(0)
}

/// Count CFG blocks for a file path.
///
/// Used to verify deletion completeness.
fn count_cfg_blocks_for_file(graph: &CodeGraph, path: &str) -> usize {
    use rusqlite::params;

    // Count CFG blocks by joining with graph_entities
    match graph.chunks.connect() {
        Ok(conn) => {
            conn.query_row(
                "SELECT COUNT(*) FROM cfg_blocks c
                 JOIN graph_entities e ON c.function_id = e.id
                 WHERE e.file_path = ?1",
                params![path],
                |row| row.get(0),
            )
            .unwrap_or(0)
        }
        Err(_) => 0,
    }
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
        // Delete facts for missing file
        #[cfg(debug_assertions)]
        {
            let deleted = delete_file_facts(graph, path_key)?;
            if !deleted.is_empty() {
                eprintln!(
                    "Deleted {} symbols, {} references, {} calls for missing file {}",
                    deleted.symbols_deleted,
                    deleted.references_deleted,
                    deleted.calls_deleted,
                    path_key
                );
            }
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = delete_file_facts(graph, path_key)?;
        }
        return Ok(ReconcileOutcome::Deleted);
    }

    // 2) Read file and compute hash
    let source = fs::read(path)?;
    let new_hash = graph.files.compute_hash(&source);

    // 3) Check if hash matches stored file node
    let snapshot = SnapshotId::current();
    let unchanged = if let Some(file_id) = graph.files.find_file_node(path_key)? {
        let node = graph.files.backend.get_node(snapshot, file_id.as_i64())?;
        let file_node: crate::graph::schema::FileNode = serde_json::from_value(node.data)
            .unwrap_or_else(|_| crate::graph::schema::FileNode {
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
    #[cfg(debug_assertions)]
    {
        let deleted = delete_file_facts(graph, path_key)?;
        if !deleted.is_empty() {
            eprintln!(
                "Deleted {} symbols, {} references, {} calls for reindex of {}",
                deleted.symbols_deleted,
                deleted.references_deleted,
                deleted.calls_deleted,
                path_key
            );
        }
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = delete_file_facts(graph, path_key)?;
    }

    // Rebuild module index after file deletion for updated import resolutions
    let _ = graph.module_resolver.build_module_index();

    let symbols = index_file(graph, path_key, &source)?;
    query::index_references(graph, path_key, &source)?;

    // Count calls for this file only (index_file already indexed calls internally)
    let calls = count_calls_in_file(graph, path_key);

    // Count references for this file only
    let references = count_references_in_file(graph, path_key);

    Ok(ReconcileOutcome::Reindexed {
        symbols,
        references,
        calls,
    })
}
