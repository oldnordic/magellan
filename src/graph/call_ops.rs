//! Call node operations for CodeGraph
//!
//! Handles call node CRUD operations and CALLS/CALLER edge management.

use anyhow::Result;
use sqlitegraph::{
    BackendDirection, EdgeSpec, GraphBackend, NeighborQuery, NodeId, NodeSpec, SqliteGraphBackend,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use crate::graph::schema::CallNode;
use crate::ingest::c::CParser;
use crate::ingest::cpp::CppParser;
use crate::ingest::java::JavaParser;
use crate::ingest::javascript::JavaScriptParser;
use crate::ingest::python::PythonParser;
use crate::ingest::typescript::TypeScriptParser;
use crate::ingest::{detect::Language, detect_language, Parser, SymbolFact, SymbolKind};
use crate::references::CallFact;

/// Call operations for CodeGraph
pub struct CallOps {
    pub backend: Rc<SqliteGraphBackend>,
}

impl CallOps {
    /// Delete all Call nodes that belong to a specific file path.
    ///
    /// Determinism: collects candidate entity IDs, sorts ascending, deletes in that order.
    pub fn delete_calls_in_file(&self, path: &str) -> Result<usize> {
        let entity_ids = self.backend.entity_ids()?;

        let mut to_delete: Vec<i64> = Vec::new();
        for entity_id in entity_ids {
            let node = match self.backend.get_node(entity_id) {
                Ok(n) => n,
                Err(_) => continue,
            };

            if node.kind != "Call" {
                continue;
            }

            let call_node: CallNode = match serde_json::from_value(node.data) {
                Ok(value) => value,
                Err(_) => continue,
            };

            if call_node.file == path {
                to_delete.push(entity_id);
            }
        }

        to_delete.sort_unstable();

        for id in &to_delete {
            self.backend.graph().delete_entity(*id)?;
        }

        Ok(to_delete.len())
    }
    /// Index calls for a file into the graph
    ///
    /// # Behavior
    /// 1. Parse symbols from source
    /// 2. Extract function calls (caller → callee)
    /// 3. Insert Call nodes
    /// 4. Create CALLER edges from caller Symbol to Call node
    /// 5. Create CALLS edges from Call node to callee Symbol
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `source` - File contents as bytes
    /// * `symbol_ids` - Map of symbol names to their node IDs (ALL symbols in database)
    pub fn index_calls(
        &self,
        path: &str,
        source: &[u8],
        symbol_ids: &HashMap<String, i64>,
    ) -> Result<usize> {
        let path_buf = PathBuf::from(path);
        let language = detect_language(&path_buf);

        // Build symbol facts from persisted symbols to enable cross-file call matching.
        let mut symbol_facts = Vec::new();
        let mut current_file_facts = Vec::new();
        for symbol_id in symbol_ids.values() {
            let node = match self.backend.get_node(*symbol_id) {
                Ok(value) => value,
                Err(_) => continue,
            };

            if node.kind != "Symbol" {
                continue;
            }

            let symbol_fact = match self.symbol_fact_from_node(&node) {
                Some(value) => value,
                None => continue,
            };

            if node.file_path.as_deref() == Some(path) {
                current_file_facts.push(symbol_fact);
            } else {
                symbol_facts.push(symbol_fact);
            }
        }

        symbol_facts.extend(current_file_facts);

        // Extract calls using language-specific parser
        let calls = match language {
            Some(Language::Rust) => {
                let mut parser = Parser::new()?;
                parser.extract_calls(path_buf.clone(), source, &symbol_facts)
            }
            Some(Language::Python) => {
                let mut parser = PythonParser::new()?;
                parser.extract_calls(path_buf.clone(), source, &symbol_facts)
            }
            Some(Language::C) => {
                let mut parser = CParser::new()?;
                parser.extract_calls(path_buf.clone(), source, &symbol_facts)
            }
            Some(Language::Cpp) => {
                let mut parser = CppParser::new()?;
                parser.extract_calls(path_buf.clone(), source, &symbol_facts)
            }
            Some(Language::Java) => {
                let mut parser = JavaParser::new()?;
                parser.extract_calls(path_buf.clone(), source, &symbol_facts)
            }
            Some(Language::JavaScript) => {
                let mut parser = JavaScriptParser::new()?;
                parser.extract_calls(path_buf.clone(), source, &symbol_facts)
            }
            Some(Language::TypeScript) => {
                let mut parser = TypeScriptParser::new()?;
                parser.extract_calls(path_buf.clone(), source, &symbol_facts)
            }
            None => Vec::new(),
        };

        // Insert call nodes and edges
        for call in &calls {
            if let Some(&callee_symbol_id) = symbol_ids.get(&call.callee) {
                if let Some(&caller_symbol_id) = symbol_ids.get(&call.caller) {
                    let call_id = self.insert_call_node(call)?;
                    // CALLER edge: caller Symbol -> Call node
                    self.insert_caller_edge(NodeId::from(caller_symbol_id), call_id)?;
                    // CALLS edge: Call node -> callee Symbol
                    self.insert_calls_edge(call_id, NodeId::from(callee_symbol_id))?;
                }
            }
        }

        Ok(calls.len())
    }

    /// Query all calls FROM a specific symbol (forward call graph)
    ///
    /// # Arguments
    /// * `symbol_id` - Node ID of the caller symbol
    ///
    /// # Returns
    /// Vector of CallFact for all calls from this symbol
    pub fn calls_from_symbol(&mut self, symbol_id: i64) -> Result<Vec<CallFact>> {
        // Query outgoing CALLER edges from caller to Call nodes
        let neighbor_ids = self.backend.neighbors(
            symbol_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLER".to_string()),
            },
        )?;

        let mut calls = Vec::new();
        for call_node_id in neighbor_ids {
            if let Ok(Some(call)) = self.call_fact_from_node(call_node_id) {
                calls.push(call);
            }
        }

        Ok(calls)
    }

    /// Query all calls TO a specific symbol (reverse call graph)
    ///
    /// # Arguments
    /// * `symbol_id` - Node ID of the callee symbol
    ///
    /// # Returns
    /// Vector of CallFact for all calls to this symbol
    pub fn callers_of_symbol(&mut self, symbol_id: i64) -> Result<Vec<CallFact>> {
        // Query incoming CALLS edges to callee
        let neighbor_ids = self.backend.neighbors(
            symbol_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: Some("CALLS".to_string()),
            },
        )?;

        let mut calls = Vec::new();
        for call_node_id in neighbor_ids {
            if let Ok(Some(call)) = self.call_fact_from_node(call_node_id) {
                calls.push(call);
            }
        }

        Ok(calls)
    }

    /// Insert a call node from CallFact
    fn insert_call_node(&self, call: &CallFact) -> Result<NodeId> {
        let call_node = CallNode {
            file: call.file_path.to_string_lossy().to_string(),
            caller: call.caller.clone(),
            callee: call.callee.clone(),
            caller_symbol_id: call.caller_symbol_id.clone(),
            callee_symbol_id: call.callee_symbol_id.clone(),
            byte_start: call.byte_start as u64,
            byte_end: call.byte_end as u64,
            start_line: call.start_line as u64,
            start_col: call.start_col as u64,
            end_line: call.end_line as u64,
            end_col: call.end_col as u64,
        };

        let node_spec = NodeSpec {
            kind: "Call".to_string(),
            name: format!("{} calls {}", call.caller, call.callee),
            file_path: Some(call.file_path.to_string_lossy().to_string()),
            data: serde_json::to_value(call_node)?,
        };

        let id = self.backend.insert_node(node_spec)?;
        Ok(NodeId::from(id))
    }

    /// Insert CALLS edge from call node to callee symbol
    fn insert_calls_edge(&self, call_id: NodeId, callee_id: NodeId) -> Result<()> {
        let edge_spec = EdgeSpec {
            from: call_id.as_i64(),
            to: callee_id.as_i64(),
            edge_type: "CALLS".to_string(),
            data: serde_json::json!({}),
        };

        self.backend.insert_edge(edge_spec)?;
        Ok(())
    }

    /// Insert CALLER edge from caller symbol to call node
    fn insert_caller_edge(&self, caller_id: NodeId, call_id: NodeId) -> Result<()> {
        let edge_spec = EdgeSpec {
            from: caller_id.as_i64(),
            to: call_id.as_i64(),
            edge_type: "CALLER".to_string(),
            data: serde_json::json!({}),
        };

        self.backend.insert_edge(edge_spec)?;
        Ok(())
    }

    /// Convert a call node to CallFact
    fn call_fact_from_node(&self, node_id: i64) -> Result<Option<CallFact>> {
        let node = self.backend.get_node(node_id)?;

        let call_node: Option<CallNode> = serde_json::from_value(node.data).ok();

        let call_node = match call_node {
            Some(n) => n,
            None => return Ok(None),
        };

        Ok(Some(CallFact {
            file_path: PathBuf::from(&call_node.file),
            caller: call_node.caller,
            callee: call_node.callee,
            caller_symbol_id: call_node.caller_symbol_id,
            callee_symbol_id: call_node.callee_symbol_id,
            byte_start: call_node.byte_start as usize,
            byte_end: call_node.byte_end as usize,
            start_line: call_node.start_line as usize,
            start_col: call_node.start_col as usize,
            end_line: call_node.end_line as usize,
            end_col: call_node.end_col as usize,
        }))
    }

    fn symbol_fact_from_node(&self, node: &sqlitegraph::GraphEntity) -> Option<SymbolFact> {
        let symbol_node: crate::graph::schema::SymbolNode =
            serde_json::from_value(node.data.clone()).ok()?;

        let file_path = node.file_path.as_deref()?;

        let kind = match symbol_node.kind.as_str() {
            "Function" => SymbolKind::Function,
            "Method" => SymbolKind::Method,
            "Class" => SymbolKind::Class,
            "Interface" => SymbolKind::Interface,
            "Enum" => SymbolKind::Enum,
            "Module" => SymbolKind::Module,
            "Union" => SymbolKind::Union,
            "Namespace" => SymbolKind::Namespace,
            "TypeAlias" => SymbolKind::TypeAlias,
            "Unknown" => SymbolKind::Unknown,
            _ => SymbolKind::Unknown,
        };

        let normalized_kind = symbol_node
            .kind_normalized
            .clone()
            .unwrap_or_else(|| kind.normalized_key().to_string());

        Some(SymbolFact {
            file_path: PathBuf::from(file_path),
            kind,
            kind_normalized: normalized_kind,
            name: symbol_node.name.clone(),
            fqn: symbol_node.name,
            byte_start: symbol_node.byte_start,
            byte_end: symbol_node.byte_end,
            start_line: symbol_node.start_line,
            start_col: symbol_node.start_col,
            end_line: symbol_node.end_line,
            end_col: symbol_node.end_col,
        })
    }
}
