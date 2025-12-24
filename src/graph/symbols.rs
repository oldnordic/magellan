//! Symbol node operations for CodeGraph
//!
//! Handles symbol node CRUD operations and DEFINES edge management.

use anyhow::Result;
use sqlitegraph::{NodeId, NodeSpec, EdgeSpec, SqliteGraphBackend, BackendDirection, NeighborQuery, GraphBackend};
use std::path::PathBuf;
use std::rc::Rc;

use crate::graph::schema::SymbolNode;
use crate::ingest::{SymbolFact, SymbolKind};

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
            byte_start: fact.byte_start,
            byte_end: fact.byte_end,
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
        Ok(NodeId::from(id))
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

    /// Convert a symbol node to SymbolFact
    pub fn symbol_fact_from_node(&self, node_id: i64, file_path: PathBuf) -> Result<Option<SymbolFact>> {
        let node = self.backend.get_node(node_id)?;

        let symbol_node: Option<SymbolNode> = serde_json::from_value(node.data)
            .ok();

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
