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
use crate::ingest::{detect::Language, detect_language, Parser};
use crate::references::CallFact;

/// Call operations for CodeGraph
pub struct CallOps {
    pub backend: Rc<SqliteGraphBackend>,
}

impl CallOps {
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
    /// * `symbol_ids` - Map of symbol names to their node IDs
    pub fn index_calls(
        &self,
        path: &str,
        source: &[u8],
        symbol_ids: &HashMap<String, i64>,
    ) -> Result<usize> {
        let path_buf = PathBuf::from(path);
        let language = detect_language(&path_buf);

        // Extract symbols first to get proper information
        let symbol_facts = match language {
            Some(Language::Rust) => {
                let mut parser = Parser::new()?;
                parser.extract_symbols(path_buf.clone(), source)
            }
            Some(Language::Python) => {
                let mut parser = PythonParser::new()?;
                parser.extract_symbols(path_buf.clone(), source)
            }
            Some(Language::C) => {
                let mut parser = CParser::new()?;
                parser.extract_symbols(path_buf.clone(), source)
            }
            Some(Language::Cpp) => {
                let mut parser = CppParser::new()?;
                parser.extract_symbols(path_buf.clone(), source)
            }
            Some(Language::Java) => {
                let mut parser = JavaParser::new()?;
                parser.extract_symbols(path_buf.clone(), source)
            }
            Some(Language::JavaScript) => {
                let mut parser = JavaScriptParser::new()?;
                parser.extract_symbols(path_buf.clone(), source)
            }
            Some(Language::TypeScript) => {
                let mut parser = TypeScriptParser::new()?;
                parser.extract_symbols(path_buf.clone(), source)
            }
            None => Vec::new(),
        };

        // Filter to only symbols that are in symbol_ids
        let symbol_facts: Vec<crate::ingest::SymbolFact> = symbol_facts
            .into_iter()
            .filter(|fact| {
                fact.name
                    .as_ref()
                    .map(|name| symbol_ids.contains_key(name))
                    .unwrap_or(false)
            })
            .collect();

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
            byte_start: call_node.byte_start as usize,
            byte_end: call_node.byte_end as usize,
            start_line: call_node.start_line as usize,
            start_col: call_node.start_col as usize,
            end_line: call_node.end_line as usize,
            end_col: call_node.end_col as usize,
        }))
    }
}
