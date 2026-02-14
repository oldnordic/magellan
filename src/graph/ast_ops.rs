//! AST node query operations
//!
//! Provides high-level methods for querying AST nodes from the database.
//! Supports file-based queries, position-based queries, and hierarchy traversal.

use anyhow::Result;

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
    /// Uses SideTables trait for backend-agnostic storage.
    /// For V3 backend, this requires prefix scan support (not yet implemented).
    pub fn get_ast_nodes_by_file(&self, file_path: &str) -> Result<Vec<AstNodeWithText>> {
        // Find file_id from file_path
        // Note: file_index lookup doesn't require &mut self since it's cached
        let file_id = self.files.file_index.get(file_path).copied();
        
        match file_id {
            Some(id) => {
                let nodes = self.side_tables.get_ast_nodes_by_file(id.as_i64())?;
                Ok(nodes.into_iter().map(AstNodeWithText::from).collect())
            }
            None => Ok(vec![]), // File not found
        }
    }

    /// Get direct children of an AST node
    ///
    /// # Arguments
    /// * `node_id` - The database ID of the parent node
    ///
    /// # Returns
    /// Vector of child AstNode structs
    pub fn get_ast_children(&self, node_id: i64) -> Result<Vec<AstNode>> {
        self.side_tables.get_ast_children(node_id)
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
        // Get all nodes and find the smallest one containing the position
        let all_nodes = self.side_tables.get_all_ast_nodes()?;
        
        // Filter nodes where position is within [byte_start, byte_end)
        let containing_nodes: Vec<_> = all_nodes
            .into_iter()
            .filter(|n| n.byte_start <= position && position < n.byte_end)
            .collect();
        
        if containing_nodes.is_empty() {
            return Ok(None);
        }
        
        // Find the smallest node (most specific match)
        // The node with the smallest span (byte_end - byte_start)
        let smallest = containing_nodes
            .into_iter()
            .min_by_key(|n| n.byte_end - n.byte_start);
        
        Ok(smallest)
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
    /// Uses SideTables trait for backend-agnostic storage.
    pub fn get_ast_nodes_by_kind(&self, kind: &str) -> Result<Vec<AstNode>> {
        self.side_tables.get_ast_nodes_by_kind(kind)
    }

    /// Get the root AST nodes (nodes without parents)
    ///
    /// # Returns
    /// Vector of root-level AstNode structs
    /// 
    /// # Note
    /// This operation requires scanning all nodes to find those without parents.
    /// For V3 backend, this returns empty until prefix scan is implemented.
    pub fn get_ast_roots(&self) -> Result<Vec<AstNode>> {
        // Get all nodes and filter for roots (parent_id IS NULL)
        let all_nodes = self.side_tables.get_all_ast_nodes()?;
        Ok(all_nodes.into_iter().filter(|n| n.parent_id.is_none()).collect())
    }

    /// Count all AST nodes in the database
    ///
    /// # Returns
    /// Total number of AST nodes
    pub fn count_ast_nodes(&self) -> Result<usize> {
        self.side_tables.count_ast_nodes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    #[cfg(not(feature = "native-v3"))]
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
    #[cfg(feature = "native-v3")]
    fn test_get_ast_children_v3() {
        // V3 backend - just verify the function doesn't panic
        let temp_dir = std::env::temp_dir().join(format!("magellan_ast_test2_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("test.db");
        let graph = CodeGraph::open(&db_path).unwrap();
        
        let _children = graph.get_ast_children(1).unwrap();
    }

    #[test]
    #[cfg(not(feature = "native-v3"))]
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
    #[cfg(feature = "native-v3")]
    fn test_get_ast_node_at_position_v3() {
        // V3 backend - just verify the function doesn't panic
        let temp_dir = std::env::temp_dir().join(format!("magellan_ast_test3_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("test.db");
        let graph = CodeGraph::open(&db_path).unwrap();
        
        let _node = graph.get_ast_node_at_position("test.rs", 25).unwrap();
    }

    #[test]
    #[cfg(not(feature = "native-v3"))]
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
    #[cfg(feature = "native-v3")]
    fn test_get_ast_nodes_by_kind_v3() {
        // V3 backend - just verify the function doesn't panic
        let temp_dir = std::env::temp_dir().join(format!("magellan_ast_test4_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("test.db");
        let graph = CodeGraph::open(&db_path).unwrap();
        
        let _nodes = graph.get_ast_nodes_by_kind("if_expression").unwrap();
    }

    #[test]
    #[cfg(not(feature = "native-v3"))]
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
    
    #[test]
    #[cfg(feature = "native-v3")]
    fn test_count_ast_nodes_v3() {
        // V3 backend uses KV store for AST nodes - test basic functionality
        let temp_dir = std::env::temp_dir().join(format!("magellan_ast_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("test.db");
        let graph = CodeGraph::open(&db_path).unwrap();

        // V3 returns 0 for count_ast_nodes (not yet fully implemented)
        // Just verify it doesn't panic
        let _count = graph.count_ast_nodes().unwrap();
    }
}
