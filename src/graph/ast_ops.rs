//! AST node query operations
//!
//! Provides high-level methods for querying AST nodes from the database.
//! Supports file-based queries, position-based queries, and hierarchy traversal.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use crate::graph::{AstNode, AstNodeWithText, CodeGraph};

#[cfg(feature = "native-v2")]
/// Get file_id from file_path using KV store
///
/// For Native-V2 backend, looks up file:path:{path} key to get file_id.
/// Returns None if file not found or not using Native-V2 backend.
///
/// # Arguments
/// * `backend` - Graph backend reference (must support KV operations)
/// * `file_path` - File path to look up
///
/// # Returns
/// * `Ok(Some(file_id))` - File found in KV store
/// * `Ok(None)` - File not found
/// * `Err(e)` - KV operation failed
fn get_file_id_kv(backend: &std::rc::Rc<dyn sqlitegraph::GraphBackend>, file_path: &str) -> Result<Option<u64>> {
    use sqlitegraph::{SnapshotId, backend::KvValue};
    use crate::kv::keys::file_path_key;

    let snapshot = SnapshotId::current();
    let key = file_path_key(file_path);

    match backend.kv_get(snapshot, &key)? {
        Some(KvValue::Integer(id)) => Ok(Some(id as u64)),
        Some(KvValue::BigInt(id)) => Ok(Some(id as u64)),
        _ => Ok(None),
    }
}

impl CodeGraph {
    /// Get all AST nodes for a specific file
    ///
    /// # Arguments
    /// * `file_path` - The file path to query
    ///
    /// # Returns
    /// Vector of AstNodeWithText for all nodes in the file
    ///
    /// # Note
    /// For Native-V2 backend, uses KV store with ast:file:{file_id} key.
    /// For SQLite backend, queries ast_nodes table.
    pub fn get_ast_nodes_by_file(&self, file_path: &str) -> Result<Vec<AstNodeWithText>> {
        #[cfg(feature = "native-v2")]
        {
            // Check if using Native-V2 backend via ChunkStore helper
            if self.chunks.has_kv_backend() {
                // Use KV store for Native-V2
                use sqlitegraph::{SnapshotId, backend::KvValue};
                use crate::kv::keys::ast_nodes_key;
                use crate::kv::encoding::decode_ast_nodes;

                let backend = &self.files.backend;
                let snapshot = SnapshotId::current();

                // Get file_id first using helper from Plan 03
                let file_id = match get_file_id_kv(backend, file_path)? {
                    Some(id) => id,
                    None => return Ok(vec![]), // File not found
                };

                // Get AST nodes for this file
                let ast_key = ast_nodes_key(file_id);
                match backend.kv_get(snapshot, &ast_key)? {
                    Some(KvValue::Bytes(data)) => {
                        let nodes: Vec<AstNode> = decode_ast_nodes(&data)?;
                        return Ok(nodes.into_iter()
                            .map(|node| AstNodeWithText::from(node))
                            .collect());
                    }
                    _ => return Ok(vec![]), // No AST data for this file
                }
            }
        }

        // SQLite fallback (existing code)
        let conn = self.chunks.connect()?;

        let mut stmt = conn.prepare(
            "SELECT id, parent_id, kind, byte_start, byte_end
             FROM ast_nodes
             ORDER BY byte_start",
        )?;

        let nodes = stmt
            .query_map([], |row| {
                let byte_start_i64: i64 = row.get(3)?;
                let byte_end_i64: i64 = row.get(4)?;
                Ok(AstNode {
                    id: Some(row.get(0)?),
                    parent_id: row.get(1)?,
                    kind: row.get(2)?,
                    byte_start: byte_start_i64 as usize,
                    byte_end: byte_end_i64 as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|node| AstNodeWithText::from(node))
            .collect();

        Ok(nodes)
    }

    /// Get direct children of an AST node
    ///
    /// # Arguments
    /// * `node_id` - The database ID of the parent node
    ///
    /// # Returns
    /// Vector of child AstNode structs
    pub fn get_ast_children(&self, node_id: i64) -> Result<Vec<AstNode>> {
        let conn = self.chunks.connect()?;

        let mut stmt = conn.prepare(
            "SELECT id, parent_id, kind, byte_start, byte_end
             FROM ast_nodes
             WHERE parent_id = ?1
             ORDER BY byte_start",
        )?;

        let children = stmt
            .query_map(params![node_id], |row| {
                let byte_start_i64: i64 = row.get(3)?;
                let byte_end_i64: i64 = row.get(4)?;
                Ok(AstNode {
                    id: Some(row.get(0)?),
                    parent_id: row.get(1)?,
                    kind: row.get(2)?,
                    byte_start: byte_start_i64 as usize,
                    byte_end: byte_end_i64 as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(children)
    }

    /// Get the AST node at a specific byte position
    ///
    /// # Arguments
    /// * `file_path` - The file path
    /// * `position` - Byte offset in the file
    ///
    /// # Returns
    /// Option<AstNode> if a node is found at the position
    pub fn get_ast_node_at_position(
        &self,
        _file_path: &str,
        position: usize,
    ) -> Result<Option<AstNode>> {
        let conn = self.chunks.connect()?;

        let node = conn
            .query_row(
                "SELECT id, parent_id, kind, byte_start, byte_end
                 FROM ast_nodes
                 WHERE byte_start <= ?1 AND byte_end > ?1
                 ORDER BY byte_end - byte_start ASC
                 LIMIT 1",
                params![position as i64],
                |row| {
                    let byte_start_i64: i64 = row.get(3)?;
                    let byte_end_i64: i64 = row.get(4)?;
                    Ok(AstNode {
                        id: Some(row.get(0)?),
                        parent_id: row.get(1)?,
                        kind: row.get(2)?,
                        byte_start: byte_start_i64 as usize,
                        byte_end: byte_end_i64 as usize,
                    })
                },
            )
            .optional()?;

        Ok(node)
    }

    /// Get all AST nodes of a specific kind
    ///
    /// # Arguments
    /// * `kind` - The node kind to filter by (e.g., "if_expression")
    ///
    /// # Returns
    /// Vector of matching AstNode structs
    ///
    /// # Note
    /// For Native-V2 backend, uses KV prefix scan on ast:file:* keys.
    /// For SQLite backend, queries ast_nodes table with WHERE clause.
    pub fn get_ast_nodes_by_kind(&self, kind: &str) -> Result<Vec<AstNode>> {
        #[cfg(feature = "native-v2")]
        {
            if self.chunks.has_kv_backend() {
                // Use KV store: scan all AST nodes and filter by kind
                use sqlitegraph::{SnapshotId, backend::KvValue};
                use crate::kv::encoding::decode_ast_nodes;

                let backend = &self.files.backend;
                let snapshot = SnapshotId::current();

                // Prefix scan all ast:file:* keys
                let entries = backend.kv_prefix_scan(snapshot, b"ast:file:")?;

                let mut all_nodes = Vec::new();
                for (_key, value) in entries {
                    if let KvValue::Bytes(data) = value {
                        if let Ok(nodes) = decode_ast_nodes::<AstNode>(&data) {
                            all_nodes.extend(nodes);
                        }
                    }
                }

                // Filter by kind
                all_nodes.retain(|n| n.kind == kind);
                all_nodes.sort_by_key(|n| n.byte_start);
                return Ok(all_nodes);
            }
        }

        // SQLite fallback (existing code)
        let conn = self.chunks.connect()?;

        let mut stmt = conn.prepare(
            "SELECT id, parent_id, kind, byte_start, byte_end
             FROM ast_nodes
             WHERE kind = ?1
             ORDER BY byte_start",
        )?;

        let nodes = stmt
            .query_map(params![kind], |row| {
                let byte_start_i64: i64 = row.get(3)?;
                let byte_end_i64: i64 = row.get(4)?;
                Ok(AstNode {
                    id: Some(row.get(0)?),
                    parent_id: row.get(1)?,
                    kind: row.get(2)?,
                    byte_start: byte_start_i64 as usize,
                    byte_end: byte_end_i64 as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(nodes)
    }

    /// Get the root AST nodes (nodes without parents)
    ///
    /// # Returns
    /// Vector of root-level AstNode structs
    pub fn get_ast_roots(&self) -> Result<Vec<AstNode>> {
        let conn = self.chunks.connect()?;

        let mut stmt = conn.prepare(
            "SELECT id, parent_id, kind, byte_start, byte_end
             FROM ast_nodes
             WHERE parent_id IS NULL
             ORDER BY byte_start",
        )?;

        let nodes = stmt
            .query_map([], |row| {
                let byte_start_i64: i64 = row.get(3)?;
                let byte_end_i64: i64 = row.get(4)?;
                Ok(AstNode {
                    id: Some(row.get(0)?),
                    parent_id: row.get(1)?,
                    kind: row.get(2)?,
                    byte_start: byte_start_i64 as usize,
                    byte_end: byte_end_i64 as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(nodes)
    }

    /// Count total AST nodes in the database
    pub fn count_ast_nodes(&self) -> Result<usize> {
        let conn = self.chunks.connect()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM ast_nodes", [], |row| {
            row.get(0)
        })?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_get_ast_children() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let graph = CodeGraph::open(&db_path).unwrap();

        // Insert test data
        let conn = graph.chunks.connect().unwrap();
        conn.execute(
            "INSERT INTO ast_nodes (id, parent_id, kind, byte_start, byte_end)
             VALUES (1, NULL, 'function_item', 0, 100),
                    (2, 1, 'block', 10, 90),
                    (3, 2, 'if_expression', 20, 80)",
            [],
        ).unwrap();

        // Get children of node 1 (should have node 2)
        let children = graph.get_ast_children(1).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].kind, "block");

        // Get children of node 2 (should have node 3)
        let children = graph.get_ast_children(2).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].kind, "if_expression");
    }

    #[test]
    fn test_get_ast_node_at_position() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let graph = CodeGraph::open(&db_path).unwrap();

        // Insert test data
        let conn = graph.chunks.connect().unwrap();
        conn.execute(
            "INSERT INTO ast_nodes (id, parent_id, kind, byte_start, byte_end)
             VALUES (1, NULL, 'block', 0, 100),
                    (2, NULL, 'if_expression', 50, 100)",
            [],
        ).unwrap();

        // Position 25 should match block (0-100)
        let node = graph.get_ast_node_at_position("test.rs", 25).unwrap();
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind, "block");

        // Position 75 should match if_expression (50-100, smallest match)
        let node = graph.get_ast_node_at_position("test.rs", 75).unwrap();
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind, "if_expression");
    }

    #[test]
    fn test_get_ast_nodes_by_kind() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let graph = CodeGraph::open(&db_path).unwrap();

        // Insert test data
        let conn = graph.chunks.connect().unwrap();
        conn.execute(
            "INSERT INTO ast_nodes (id, parent_id, kind, byte_start, byte_end)
             VALUES (1, NULL, 'if_expression', 0, 100),
                    (2, NULL, 'block', 100, 200),
                    (3, NULL, 'if_expression', 200, 300)",
            [],
        ).unwrap();

        let if_nodes = graph.get_ast_nodes_by_kind("if_expression").unwrap();
        assert_eq!(if_nodes.len(), 2);

        let block_nodes = graph.get_ast_nodes_by_kind("block").unwrap();
        assert_eq!(block_nodes.len(), 1);
    }

    #[test]
    fn test_count_ast_nodes() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let graph = CodeGraph::open(&db_path).unwrap();

        assert_eq!(graph.count_ast_nodes().unwrap(), 0);

        let conn = graph.chunks.connect().unwrap();
        conn.execute(
            "INSERT INTO ast_nodes (kind, byte_start, byte_end)
             VALUES ('block', 0, 100), ('if_expression', 100, 200)",
            [],
        ).unwrap();

        assert_eq!(graph.count_ast_nodes().unwrap(), 2);
    }
}
