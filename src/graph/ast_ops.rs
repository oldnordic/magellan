//! AST node query operations
//!
//! Provides high-level methods for querying AST nodes from the database.
//! Supports file-based queries, position-based queries, and hierarchy traversal.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use crate::graph::{AstNode, AstNodeWithText, CodeGraph};

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
    /// MVP version returns all nodes. Phase 2 will add file_id filtering.
    pub fn get_ast_nodes_by_file(&self, _file_path: &str) -> Result<Vec<AstNodeWithText>> {
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
    pub fn get_ast_nodes_by_kind(&self, kind: &str) -> Result<Vec<AstNode>> {
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
