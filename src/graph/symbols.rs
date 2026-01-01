//! Symbol node operations for CodeGraph
//!
//! Handles symbol node CRUD operations and DEFINES edge management.

use anyhow::Result;
use sqlitegraph::{
    add_label, BackendDirection, EdgeSpec, GraphBackend, NeighborQuery, NodeId, NodeSpec,
    SqliteGraphBackend,
};
use std::rc::Rc;

use crate::detect_language;
use crate::graph::schema::SymbolNode;
use crate::ingest::SymbolFact;

/// Symbol operations for CodeGraph
pub struct SymbolOps {
    pub backend: Rc<SqliteGraphBackend>,
}

impl SymbolOps {
    /// Insert a symbol node from SymbolFact
    pub fn insert_symbol_node(&self, fact: &SymbolFact) -> Result<NodeId> {
        let symbol_node = SymbolNode {
            name: fact.name.clone(),
            kind: format!("{:?}", fact.kind),
            kind_normalized: Some(fact.kind_normalized.clone()),
            byte_start: fact.byte_start,
            byte_end: fact.byte_end,
            start_line: fact.start_line,
            start_col: fact.start_col,
            end_line: fact.end_line,
            end_col: fact.end_col,
        };

        let name = fact.name.clone().unwrap_or_else(|| {
            // Generate a name for unnamed symbols (like impl blocks)
            format!("<{:?} at {}>", fact.kind, fact.byte_start)
        });

        let node_spec = NodeSpec {
            kind: "Symbol".to_string(),
            name,
            file_path: Some(fact.file_path.to_string_lossy().to_string()),
            data: serde_json::to_value(symbol_node)?,
        };

        let id = self.backend.insert_node(node_spec)?;
        let node_id = NodeId::from(id);

        // Add labels for efficient querying
        let graph = self.backend.graph();

        // Language label (e.g., "rust", "python", "javascript")
        if let Some(language) = detect_language(&fact.file_path) {
            add_label(graph, node_id.as_i64(), language.as_str())?;
        }

        // Symbol kind label (e.g., "fn", "struct", "enum", "method")
        add_label(graph, node_id.as_i64(), &fact.kind_normalized)?;

        Ok(node_id)
    }

    /// Insert DEFINES edge from file to symbol
    pub fn insert_defines_edge(&self, file_id: NodeId, symbol_id: NodeId) -> Result<()> {
        let edge_spec = EdgeSpec {
            from: file_id.as_i64(),
            to: symbol_id.as_i64(),
            edge_type: "DEFINES".to_string(),
            data: serde_json::json!({}),
        };

        self.backend.insert_edge(edge_spec)?;
        Ok(())
    }

    /// Delete all symbols and DEFINES edges for a file
    pub fn delete_file_symbols(&self, file_id: NodeId) -> Result<()> {
        // Find all outgoing DEFINES edges
        let neighbor_ids = self.backend.neighbors(
            file_id.as_i64(),
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("DEFINES".to_string()),
            },
        )?;

        // Delete each symbol node (edges are cascade deleted)
        for symbol_node_id in neighbor_ids {
            self.backend.graph().delete_entity(symbol_node_id)?;
        }

        Ok(())
    }
}
