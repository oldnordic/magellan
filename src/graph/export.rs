//! JSON export functionality for CodeGraph
//!
//! Exports graph data to JSON format for LLM consumption.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlitegraph::{GraphBackend, NeighborQuery, BackendDirection};

use super::{CodeGraph, FileNode, SymbolNode, ReferenceNode, CallNode};

/// JSON export structure containing all graph data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphExport {
    pub files: Vec<FileExport>,
    pub symbols: Vec<SymbolExport>,
    pub references: Vec<ReferenceExport>,
    pub calls: Vec<CallExport>,
}

/// File entry for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileExport {
    pub path: String,
    pub hash: String,
}

/// Symbol entry for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolExport {
    pub name: Option<String>,
    pub kind: String,
    pub file: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// Reference entry for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceExport {
    pub file: String,
    pub referenced_symbol: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// Call entry for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallExport {
    pub file: String,
    pub caller: String,
    pub callee: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// Export all graph data to JSON format
///
/// # Returns
/// JSON string containing all files, symbols, references, and calls
pub fn export_json(graph: &mut CodeGraph) -> Result<String> {
    let mut files = Vec::new();
    let mut symbols = Vec::new();
    let mut references = Vec::new();
    let mut calls = Vec::new();

    // Get all entity IDs from the graph
    let entity_ids = graph.files.backend.entity_ids()?;

    // Process each entity
    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(entity_id)?;

        match entity.kind.as_str() {
            "File" => {
                if let Ok(file_node) = serde_json::from_value::<FileNode>(entity.data.clone()) {
                    files.push(FileExport {
                        path: file_node.path,
                        hash: file_node.hash,
                    });
                }
            }
            "Symbol" => {
                if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(entity.data.clone()) {
                    // Get file path from DEFINES edge (incoming from File)
                    let file = get_file_path_from_symbol(graph, entity_id)?;

                    symbols.push(SymbolExport {
                        name: symbol_node.name,
                        kind: symbol_node.kind,
                        file,
                        byte_start: symbol_node.byte_start,
                        byte_end: symbol_node.byte_end,
                        start_line: symbol_node.start_line,
                        start_col: symbol_node.start_col,
                        end_line: symbol_node.end_line,
                        end_col: symbol_node.end_col,
                    });
                }
            }
            "Reference" => {
                if let Ok(ref_node) = serde_json::from_value::<ReferenceNode>(entity.data.clone()) {
                    // Extract symbol name from entity name (format: "ref to {symbol_name}")
                    let referenced_symbol = entity
                        .name
                        .strip_prefix("ref to ")
                        .unwrap_or("")
                        .to_string();

                    references.push(ReferenceExport {
                        file: ref_node.file,
                        referenced_symbol,
                        byte_start: ref_node.byte_start as usize,
                        byte_end: ref_node.byte_end as usize,
                        start_line: ref_node.start_line as usize,
                        start_col: ref_node.start_col as usize,
                        end_line: ref_node.end_line as usize,
                        end_col: ref_node.end_col as usize,
                    });
                }
            }
            "Call" => {
                if let Ok(call_node) = serde_json::from_value::<CallNode>(entity.data.clone()) {
                    calls.push(CallExport {
                        file: call_node.file,
                        caller: call_node.caller,
                        callee: call_node.callee,
                        byte_start: call_node.byte_start as usize,
                        byte_end: call_node.byte_end as usize,
                        start_line: call_node.start_line as usize,
                        start_col: call_node.start_col as usize,
                        end_line: call_node.end_line as usize,
                        end_col: call_node.end_col as usize,
                    });
                }
            }
            _ => {
                // Ignore unknown node types
            }
        }
    }

    // Sort for deterministic output
    files.sort_by(|a, b| a.path.cmp(&b.path));
    symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));
    references.sort_by(|a, b| (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol)));
    calls.sort_by(|a, b| (&a.file, &a.caller, &a.callee).cmp(&(&b.file, &b.caller, &b.callee)));

    let export = GraphExport {
        files,
        symbols,
        references,
        calls,
    };

    Ok(serde_json::to_string_pretty(&export)?)
}

/// Get the file path for a symbol by following DEFINES edge
fn get_file_path_from_symbol(graph: &mut CodeGraph, symbol_id: i64) -> Result<String> {
    // Query incoming DEFINES edges to find the File node
    let file_ids = graph.files.backend.neighbors(
        symbol_id,
        NeighborQuery {
            direction: BackendDirection::Incoming,
            edge_type: Some("DEFINES".to_string()),
        },
    )?;

    if let Some(file_id) = file_ids.first() {
        let entity = graph.files.backend.get_node(*file_id)?;
        if entity.kind == "File" {
            if let Ok(file_node) = serde_json::from_value::<FileNode>(entity.data) {
                return Ok(file_node.path);
            }
        }
    }

    // Fallback: return empty string if no file found
    Ok(String::new())
}
