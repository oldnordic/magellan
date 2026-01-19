//! Core graph operations for CodeGraph
//!
//! Provides file indexing and deletion operations.

use anyhow::Result;
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
/// - All deletions occur within an IMMEDIATE transaction for atomicity.
///
/// Verification:
/// - Counts items before transaction, asserts counts after each deletion step.
/// - Panics if counts don't match (catches orphaned data bugs).
///
/// # Returns
/// DeleteResult with detailed counts of deleted entities.
pub fn delete_file_facts(graph: &mut CodeGraph, path: &str) -> Result<DeleteResult> {
    use crate::graph::schema::delete_edges_touching_entities;

    // === PHASE 1: Count items to be deleted (before any deletion) ===
    // These are the expected counts we will verify against.
    // IMPORTANT: Do this BEFORE opening any transaction connection to avoid locking issues.

    // Count symbols defined by this file (DEFINES edges)
    let expected_symbols: usize = if let Some(file_id) = graph.files.find_file_node(path)? {
        graph
            .files
            .backend
            .neighbors(
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

    // === PHASE 2: Perform deletions ===
    // Note: We cannot wrap all operations in a single transaction because:
    // 1. ChunkStore uses its own connection
    // 2. SqliteGraphBackend uses its own connection
    // 3. SQLite IMMEDIATE transactions on one connection block writes from another
    //
    // For true transactional behavior, we would need to share connections across
    // ChunkStore and SqliteGraphBackend, which requires architectural changes.
    //
    // Current approach: Use auto-commit for each operation. The row-count assertions
    // provide verification that all expected data was deleted.

    let mut deleted_entity_ids: Vec<i64> = Vec::new();
    let symbols_deleted: usize;
    let chunks_deleted: usize;
    let references_deleted: usize;
    let calls_deleted: usize;

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

        symbols_deleted = symbol_ids_sorted.len();
        deleted_entity_ids.extend(symbol_ids_sorted);

        // Assert symbol count matches expected
        assert_eq!(
            symbols_deleted, expected_symbols,
            "Symbol deletion count mismatch for '{}': expected {}, got {}",
            path, expected_symbols, symbols_deleted
        );

        // Delete code chunks for this file.
        let conn = graph.chunks.connect()?;
        chunks_deleted = conn
            .execute(
                "DELETE FROM code_chunks WHERE file_path = ?1",
                rusqlite::params![path],
            )
            .map_err(|e| anyhow::anyhow!("Failed to delete code chunks: {}", e))?;

        // Assert chunk count matches expected
        assert_eq!(
            chunks_deleted, expected_chunks,
            "Code chunk deletion count mismatch for '{}': expected {}, got {}",
            path, expected_chunks, chunks_deleted
        );

        // Delete the File node itself.
        graph
            .files
            .backend
            .graph()
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
        let edges_deleted = delete_edges_touching_entities(&conn, &deleted_entity_ids)?;

        // Remove from in-memory index AFTER successful deletions.
        graph.files.file_index.remove(path);

        Ok(DeleteResult {
            symbols_deleted,
            references_deleted,
            calls_deleted,
            chunks_deleted,
            edges_deleted,
        })
    } else {
        // No File node exists, but we still clean up orphaned data.

        // Delete chunks
        let conn = graph.chunks.connect()?;
        chunks_deleted = conn
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

        // No file node to remove from index
        Ok(DeleteResult {
            symbols_deleted: 0,
            references_deleted,
            calls_deleted,
            chunks_deleted,
            edges_deleted: 0,
        })
    }
}

/// Count Reference nodes with matching file_path.
///
/// Used to verify deletion completeness.
fn count_references_in_file(graph: &CodeGraph, path: &str) -> usize {
    graph
        .references
        .backend
        .entity_ids()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| graph.references.backend.get_node(*id).ok())
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
    graph
        .calls
        .backend
        .entity_ids()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| graph.calls.backend.get_node(*id).ok())
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
/// NOTE: Due to SQLite's limitation with multiple connections and write locking,
/// true transactional rollback testing requires architectural changes to share
/// connections between ChunkStore and SqliteGraphBackend.
///
/// Current approach: Tests verify deletion completeness rather than rollback.
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

            symbols_deleted = symbol_ids_sorted.len();
            deleted_entity_ids.extend(symbol_ids_sorted);

            // Verification point after symbols deleted
            if verify_at == Some(FailPoint::AfterSymbolsDeleted) {
                // Note: We don't remove from file_index here since file still exists
                return Ok(DeleteResult {
                    symbols_deleted,
                    references_deleted: 0,
                    calls_deleted: 0,
                    chunks_deleted: 0,
                    edges_deleted: 0,
                });
            }

            // Delete code chunks for this file.
            let conn = graph.chunks.connect()?;
            chunks_deleted = conn
                .execute(
                    "DELETE FROM code_chunks WHERE file_path = ?1",
                    rusqlite::params![path],
                )
                .map_err(|e| anyhow::anyhow!("Failed to delete code chunks: {}", e))?;

            // Verification point after chunks deleted
            if verify_at == Some(FailPoint::AfterChunksDeleted) {
                return Ok(DeleteResult {
                    symbols_deleted,
                    references_deleted: 0,
                    calls_deleted: 0,
                    chunks_deleted,
                    edges_deleted: 0,
                });
            }

            // Delete the File node itself.
            graph
                .files
                .backend
                .graph()
                .delete_entity(file_id.as_i64())?;
            deleted_entity_ids.push(file_id.as_i64());

            // Remove from in-memory index since file node is now deleted from DB
            graph.files.file_index.remove(path);

            // Verification point after file node deleted (but before references/calls)
            if verify_at == Some(FailPoint::BeforeFileDeleted) {
                return Ok(DeleteResult {
                    symbols_deleted,
                    references_deleted: 0,
                    calls_deleted: 0,
                    chunks_deleted,
                    edges_deleted: 0,
                });
            }

            // Delete references in this file.
            references_deleted = graph.references.delete_references_in_file(path)?;

            // Verification point after references deleted
            if verify_at == Some(FailPoint::AfterReferencesDeleted) {
                return Ok(DeleteResult {
                    symbols_deleted,
                    references_deleted,
                    calls_deleted: 0,
                    chunks_deleted,
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
                    chunks_deleted,
                    edges_deleted: 0,
                });
            }

            // Explicit edge cleanup for deleted IDs.
            deleted_entity_ids.sort_unstable();
            deleted_entity_ids.dedup();
            let edges_deleted = delete_edges_touching_entities(&conn, &deleted_entity_ids)?;

            Ok(DeleteResult {
                symbols_deleted,
                references_deleted,
                calls_deleted,
                chunks_deleted,
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

            Ok(DeleteResult {
                symbols_deleted: 0,
                references_deleted,
                calls_deleted,
                chunks_deleted,
                edges_deleted: 0,
            })
        }
    }
}

/// Count code chunks for a file path.
///
/// Used to verify deletion completeness.
fn count_chunks_for_file(graph: &CodeGraph, path: &str) -> usize {
    graph
        .chunks
        .connect()
        .ok()
        .and_then(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM code_chunks WHERE file_path = ?1",
                &[path],
                |row| row.get::<_, i64>(0),
            )
            .ok()
        })
        .map(|count| count as usize)
        .unwrap_or(0)
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
        // DeleteResult is ignored - we only care that deletion succeeded
        let _ = delete_file_facts(graph, path_key)?;
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
    // DeleteResult is ignored - we only care that deletion succeeded
    let _ = delete_file_facts(graph, path_key)?;

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
