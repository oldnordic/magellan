//! File node operations for CodeGraph
//!
//! Handles file node CRUD operations and in-memory file indexing.

use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlitegraph::{
    GraphBackend, NodeId, NodeSpec,
    SqliteGraphBackend,
};
use std::collections::HashMap;
use std::rc::Rc;

use crate::graph::schema::FileNode;
use crate::ingest::{SymbolFact, SymbolKind};

/// File operations for CodeGraph
pub struct FileOps {
    pub backend: Rc<SqliteGraphBackend>,
    pub file_index: HashMap<String, NodeId>,
}

impl FileOps {
    /// Find file node by path, checking in-memory index first
    pub fn find_file_node(&mut self, path: &str) -> Result<Option<NodeId>> {
        // Check in-memory index first
        if let Some(&node_id) = self.file_index.get(path) {
            return Ok(Some(node_id));
        }

        // Not in index, need to scan (fallback)
        // This is expensive, so we rebuild the index
        self.rebuild_file_index()?;
        Ok(self.file_index.get(path).copied())
    }

    /// Find existing file node or create new one
    pub fn find_or_create_file_node(&mut self, path: &str, hash: &str) -> Result<NodeId> {
        if let Some(id) = self.find_file_node(path)? {
            // File exists, update hash
            let node = self.backend.get_node(id.as_i64())?;

            // Parse existing FileNode, update hash, serialize back
            let mut file_node: FileNode = serde_json::from_value(node.data.clone())
                .unwrap_or_else(|_| FileNode {
                    path: path.to_string(),
                    hash: hash.to_string(),
                });
            file_node.hash = hash.to_string();

            let updated_data = serde_json::to_value(file_node)?;

            // Create new NodeSpec with updated data
            let node_spec = NodeSpec {
                kind: "File".to_string(),
                name: path.to_string(),
                file_path: Some(path.to_string()),
                data: updated_data,
            };

            // Delete old node and insert new one (sqlitegraph doesn't support update)
            self.backend.graph().delete_entity(id.as_i64())?;
            let new_id = self.backend.insert_node(node_spec)?;
            let new_node_id = NodeId::from(new_id);

            // Update index
            self.file_index.insert(path.to_string(), new_node_id);

            Ok(new_node_id)
        } else {
            // Create new file node
            let file_node = FileNode {
                path: path.to_string(),
                hash: hash.to_string(),
            };

            let node_spec = NodeSpec {
                kind: "File".to_string(),
                name: path.to_string(),
                file_path: Some(path.to_string()),
                data: serde_json::to_value(file_node)?,
            };

            let id = self.backend.insert_node(node_spec)?;
            let node_id = NodeId::from(id);

            // Update index
            self.file_index.insert(path.to_string(), node_id);

            Ok(node_id)
        }
    }

    /// Rebuild in-memory file index by scanning all nodes
    pub fn rebuild_file_index(&mut self) -> Result<()> {
        self.file_index.clear();

        // Use SqliteGraphBackend's entity_ids method
        let ids = self.backend.entity_ids()?;

        for id in ids {
            let node = match self.backend.get_node(id) {
                Ok(n) => n,
                Err(_) => continue,
            };

            if node.kind == "File" {
                if let Ok(file_node) = serde_json::from_value::<FileNode>(node.data) {
                    self.file_index.insert(file_node.path.clone(), NodeId::from(id));
                }
            }
        }

        Ok(())
    }

    /// Compute SHA-256 hash of file contents
    pub fn compute_hash(&self, source: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(source);
        let hash = hasher.finalize();
        hex::encode(hash)
    }

    /// Convert a symbol node to SymbolFact
    pub fn symbol_fact_from_node(
        &self,
        node_id: i64,
        file_path: std::path::PathBuf,
    ) -> Result<Option<SymbolFact>> {
        let node = self.backend.get_node(node_id)?;

        let symbol_node: Option<crate::graph::schema::SymbolNode> =
            serde_json::from_value(node.data).ok();

        let symbol_node = match symbol_node {
            Some(n) => n,
            None => return Ok(None),
        };

        let kind = match symbol_node.kind.as_str() {
            "Function" => SymbolKind::Function,
            "Struct" => SymbolKind::Struct,
            "Enum" => SymbolKind::Enum,
            "Trait" => SymbolKind::Trait,
            "Method" => SymbolKind::Method,
            "Module" => SymbolKind::Module,
            "Unknown" => SymbolKind::Unknown,
            _ => SymbolKind::Unknown,
        };

        Ok(Some(SymbolFact {
            file_path,
            kind,
            name: symbol_node.name,
            byte_start: symbol_node.byte_start,
            byte_end: symbol_node.byte_end,
        }))
    }
}
