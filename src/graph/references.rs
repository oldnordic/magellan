//! Reference node operations for CodeGraph
//!
//! Handles reference node CRUD operations and REFERENCES edge management.

use anyhow::Result;
use sqlitegraph::{
    BackendDirection, EdgeSpec, GraphBackend, NeighborQuery, NodeId, NodeSpec, SqliteGraphBackend,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use crate::graph::schema::ReferenceNode;
use crate::ingest::c::CParser;
use crate::ingest::cpp::CppParser;
use crate::ingest::java::JavaParser;
use crate::ingest::javascript::JavaScriptParser;
use crate::ingest::python::PythonParser;
use crate::ingest::typescript::TypeScriptParser;
use crate::ingest::{detect::Language, detect_language, Parser};
use crate::references::ReferenceFact;

/// Reference operations for CodeGraph
pub struct ReferenceOps {
    pub backend: Rc<SqliteGraphBackend>,
}

impl ReferenceOps {
    /// Delete all Reference nodes that belong to a specific file path.
    ///
    /// Determinism: collects candidate entity IDs, sorts ascending, deletes in that order.
    pub fn delete_references_in_file(&self, path: &str) -> Result<usize> {
        let entity_ids = self.backend.entity_ids()?;

        let mut to_delete: Vec<i64> = Vec::new();
        for entity_id in entity_ids {
            let node = match self.backend.get_node(entity_id) {
                Ok(n) => n,
                Err(_) => continue,
            };

            if node.kind != "Reference" {
                continue;
            }

            let reference_node: ReferenceNode = match serde_json::from_value(node.data) {
                Ok(value) => value,
                Err(_) => continue,
            };

            if reference_node.file == path {
                to_delete.push(entity_id);
            }
        }

        to_delete.sort_unstable();

        for id in &to_delete {
            self.backend.graph().delete_entity(*id)?;
        }

        Ok(to_delete.len())
    }
    /// Index references for a file into the graph
    ///
    /// # Behavior
    /// 1. Parse symbols from source (to get correct spans for filtering)
    /// 2. Extract references to those symbols
    /// 3. Insert Reference nodes
    /// 4. Create REFERENCES edges from Reference to Symbol
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `source` - File contents as bytes
    /// * `symbol_fqn_to_id` - Map of FQNs to their node IDs (ALL symbols in database)
    pub fn index_references(
        &self,
        path: &str,
        source: &[u8],
        symbol_fqn_to_id: &HashMap<String, i64>,
    ) -> Result<usize> {
        let path_buf = PathBuf::from(path);
        let language = detect_language(&path_buf);

        // Build symbol_facts from ALL symbols in the database
        // This enables cross-file reference matching
        let mut all_symbol_facts: Vec<crate::ingest::SymbolFact> = Vec::new();

        // Get all entity IDs from the graph
        let entity_ids = match self.backend.entity_ids() {
            Ok(ids) => ids,
            Err(_) => return Ok(0), // If we can't get entities, no references to index
        };

        // Iterate through all entities and find Symbol nodes
        for entity_id in entity_ids {
            if let Ok(node) = self.backend.get_node(entity_id) {
                // Check if this is a Symbol node by looking at the kind field
                if node.kind == "Symbol" {
                    if let Ok(symbol_node) = serde_json::from_value::<
                        crate::graph::schema::SymbolNode,
                    >(node.data.clone())
                    {
                        // Convert SymbolNode to SymbolFact
                        if let Some(name) = &symbol_node.name {
                            // Validate file_path is valid UTF-8 before creating SymbolFact
                            let file_path_str = node.file_path.as_deref().unwrap_or("");
                            if std::str::from_utf8(file_path_str.as_bytes()).is_ok() {
                                // Extract FQN, fall back to name for backward compatibility
                                let fqn = symbol_node
                                    .fqn
                                    .clone()
                                    .or(symbol_node.name.clone())
                                    .unwrap_or_default();

                                all_symbol_facts.push(crate::ingest::SymbolFact {
                                    file_path: PathBuf::from(file_path_str),
                                    kind: match symbol_node.kind_normalized.as_deref() {
                                        Some("fn") => crate::ingest::SymbolKind::Function,
                                        Some("method") => crate::ingest::SymbolKind::Method,
                                        Some("struct") => crate::ingest::SymbolKind::Class,
                                        Some("enum") => crate::ingest::SymbolKind::Enum,
                                        Some("trait") => crate::ingest::SymbolKind::Interface,
                                        Some("mod") => crate::ingest::SymbolKind::Module,
                                        _ => crate::ingest::SymbolKind::Unknown,
                                    },
                                    kind_normalized: symbol_node
                                        .kind_normalized
                                        .clone()
                                        .unwrap_or(symbol_node.kind.clone()),
                                    name: Some(name.clone()),
                                    fqn: if fqn.is_empty() { None } else { Some(fqn) },
                                    byte_start: symbol_node.byte_start as usize,
                                    byte_end: symbol_node.byte_end as usize,
                                    start_line: symbol_node.start_line as usize,
                                    start_col: symbol_node.start_col as usize,
                                    end_line: symbol_node.end_line as usize,
                                    end_col: symbol_node.end_col as usize,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Extract references using language-specific parser
        // Pass all_symbol_facts to enable cross-file reference matching
        let references = match language {
            Some(Language::Rust) => {
                let mut parser = Parser::new()?;
                parser.extract_references(path_buf.clone(), source, &all_symbol_facts)
            }
            Some(Language::Python) => {
                let mut parser = PythonParser::new()?;
                parser.extract_references(path_buf.clone(), source, &all_symbol_facts)
            }
            Some(Language::C) => {
                let mut parser = CParser::new()?;
                parser.extract_references(path_buf.clone(), source, &all_symbol_facts)
            }
            Some(Language::Cpp) => {
                let mut parser = CppParser::new()?;
                parser.extract_references(path_buf.clone(), source, &all_symbol_facts)
            }
            Some(Language::Java) => {
                let mut parser = JavaParser::new()?;
                parser.extract_references(path_buf.clone(), source, &all_symbol_facts)
            }
            Some(Language::JavaScript) => {
                let mut parser = JavaScriptParser::new()?;
                parser.extract_references(path_buf.clone(), source, &all_symbol_facts)
            }
            Some(Language::TypeScript) => {
                let mut parser = TypeScriptParser::new()?;
                parser.extract_references(path_buf.clone(), source, &all_symbol_facts)
            }
            None => Vec::new(),
        };

        // Insert reference nodes and REFERENCES edges
        for reference in &references {
            if let Some(&target_symbol_id) = symbol_fqn_to_id.get(&reference.referenced_symbol) {
                let reference_id = self.insert_reference_node(reference)?;
                self.insert_references_edge(
                    reference_id,
                    NodeId::from(target_symbol_id),
                    reference,
                )?;
            }
        }

        Ok(references.len())
    }

    /// Query all references to a specific symbol
    ///
    /// # Arguments
    /// * `symbol_id` - Node ID of the target symbol
    ///
    /// # Returns
    /// Vector of ReferenceFact for all references to the symbol
    pub fn references_to_symbol(&mut self, symbol_id: i64) -> Result<Vec<ReferenceFact>> {
        // Query incoming REFERENCES edges
        let neighbor_ids = self.backend.neighbors(
            symbol_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: Some("REFERENCES".to_string()),
            },
        )?;

        let mut references = Vec::new();
        for reference_node_id in neighbor_ids {
            if let Ok(Some(reference)) = self.reference_fact_from_node(reference_node_id) {
                references.push(reference);
            }
        }

        Ok(references)
    }

    /// Insert a reference node from ReferenceFact
    fn insert_reference_node(&self, reference: &ReferenceFact) -> Result<NodeId> {
        let reference_node = ReferenceNode {
            file: reference.file_path.to_string_lossy().to_string(),
            byte_start: reference.byte_start as u64,
            byte_end: reference.byte_end as u64,
            start_line: reference.start_line as u64,
            start_col: reference.start_col as u64,
            end_line: reference.end_line as u64,
            end_col: reference.end_col as u64,
        };

        let node_spec = NodeSpec {
            kind: "Reference".to_string(),
            name: format!("ref to {}", reference.referenced_symbol),
            file_path: Some(reference.file_path.to_string_lossy().to_string()),
            data: serde_json::to_value(reference_node)?,
        };

        let id = self.backend.insert_node(node_spec)?;
        Ok(NodeId::from(id))
    }

    /// Insert REFERENCES edge from reference to symbol
    fn insert_references_edge(
        &self,
        reference_id: NodeId,
        symbol_id: NodeId,
        reference: &ReferenceFact,
    ) -> Result<()> {
        let edge_spec = EdgeSpec {
            from: reference_id.as_i64(),
            to: symbol_id.as_i64(),
            edge_type: "REFERENCES".to_string(),
            data: serde_json::json!({
                "byte_start": reference.byte_start,
                "byte_end": reference.byte_end,
                "start_line": reference.start_line,
                "start_col": reference.start_col,
                "end_line": reference.end_line,
                "end_col": reference.end_col,
            }),
        };

        self.backend.insert_edge(edge_spec)?;
        Ok(())
    }

    /// Convert a reference node to ReferenceFact
    fn reference_fact_from_node(&self, node_id: i64) -> Result<Option<ReferenceFact>> {
        let node = self.backend.get_node(node_id)?;

        let reference_node: Option<ReferenceNode> = serde_json::from_value(node.data).ok();

        let reference_node = match reference_node {
            Some(n) => n,
            None => return Ok(None),
        };

        // Extract symbol name from node.name (format: "ref to {symbol_name}")
        let referenced_symbol = node.name.strip_prefix("ref to ").unwrap_or("").to_string();

        Ok(Some(ReferenceFact {
            file_path: PathBuf::from(&reference_node.file),
            referenced_symbol,
            byte_start: reference_node.byte_start as usize,
            byte_end: reference_node.byte_end as usize,
            start_line: reference_node.start_line as usize,
            start_col: reference_node.start_col as usize,
            end_line: reference_node.end_line as usize,
            end_col: reference_node.end_col as usize,
        }))
    }
}
