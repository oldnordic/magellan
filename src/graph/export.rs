//! JSON export functionality for CodeGraph
//!
//! Exports graph data to JSON/JSONL format for LLM consumption.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlitegraph::{BackendDirection, GraphBackend, NeighborQuery};

use super::{CallNode, CodeGraph, FileNode, ReferenceNode, SymbolNode};

/// Export format options
///
/// Dot and Csv are placeholders for future plans (02-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Standard JSON array format
    Json,
    /// JSON Lines format (one JSON record per line)
    JsonL,
    /// Graphviz DOT format (placeholder for Plan 02)
    Dot,
    /// CSV format (placeholder for Plan 03)
    Csv,
}

impl ExportFormat {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(ExportFormat::Json),
            "jsonl" => Some(ExportFormat::JsonL),
            "dot" => Some(ExportFormat::Dot),
            "csv" => Some(ExportFormat::Csv),
            _ => None,
        }
    }
}

/// Configuration for graph export
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Output format
    pub format: ExportFormat,
    /// Include symbols in export
    pub include_symbols: bool,
    /// Include references in export
    pub include_references: bool,
    /// Include calls in export
    pub include_calls: bool,
    /// Use minified JSON (no pretty-printing)
    pub minify: bool,
    /// Filters for export (file, symbol, kind, max_depth, cluster)
    pub filters: ExportFilters,
}

/// Export filters for DOT export
///
/// Filters allow restricting the exported graph to specific files,
/// symbols, or limiting traversal depth.
#[derive(Debug, Clone, Default)]
pub struct ExportFilters {
    /// Only include calls from/to symbols in this file path
    pub file: Option<String>,
    /// Only include calls from/to this specific symbol name
    pub symbol: Option<String>,
    /// Only include symbols of this kind (e.g., "Function", "Method")
    pub kind: Option<String>,
    /// Maximum depth for call graph traversal (None = unlimited)
    pub max_depth: Option<usize>,
    /// Group nodes by file in subgraphs (DOT cluster feature)
    pub cluster: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        ExportConfig {
            format: ExportFormat::Json,
            include_symbols: true,
            include_references: true,
            include_calls: true,
            minify: false,
            filters: ExportFilters::default(),
        }
    }
}

impl ExportConfig {
    /// Create a new export config with the specified format
    pub fn new(format: ExportFormat) -> Self {
        ExportConfig {
            format,
            ..Default::default()
        }
    }

    /// Set whether to include symbols
    pub fn with_symbols(mut self, include: bool) -> Self {
        self.include_symbols = include;
        self
    }

    /// Set whether to include references
    pub fn with_references(mut self, include: bool) -> Self {
        self.include_references = include;
        self
    }

    /// Set whether to include calls
    pub fn with_calls(mut self, include: bool) -> Self {
        self.include_calls = include;
        self
    }

    /// Set whether to minify JSON output
    pub fn with_minify(mut self, minify: bool) -> Self {
        self.minify = minify;
        self
    }
}

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
    /// Stable symbol ID for cross-run correlation
    #[serde(default)]
    pub symbol_id: Option<String>,
    pub name: Option<String>,
    pub kind: String,
    pub kind_normalized: Option<String>,
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
    /// Stable ID of referenced symbol
    #[serde(default)]
    pub target_symbol_id: Option<String>,
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
    /// Stable ID of caller symbol
    #[serde(default)]
    pub caller_symbol_id: Option<String>,
    /// Stable ID of callee symbol
    #[serde(default)]
    pub callee_symbol_id: Option<String>,
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
                        symbol_id: symbol_node.symbol_id,
                        name: symbol_node.name,
                        kind: symbol_node.kind,
                        kind_normalized: symbol_node.kind_normalized,
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
                        target_symbol_id: None, // Would need symbol lookup; defer to Task 3
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
                        caller_symbol_id: call_node.caller_symbol_id,
                        callee_symbol_id: call_node.callee_symbol_id,
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
    references
        .sort_by(|a, b| (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol)));
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

/// JSONL record type discriminator
///
/// Each JSONL line includes a "type" field to identify the record type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum JsonlRecord {
    File(FileExport),
    Symbol(SymbolExport),
    Reference(ReferenceExport),
    Call(CallExport),
}

/// Export all graph data to JSONL format
///
/// JSONL (JSON Lines) format: one compact JSON object per line.
/// Each line includes a "type" field for record identification.
///
/// # Returns
/// JSONL string with one record per line, deterministically sorted
pub fn export_jsonl(graph: &mut CodeGraph) -> Result<String> {
    let mut records = Vec::new();

    // Get all entity IDs from the graph
    let entity_ids = graph.files.backend.entity_ids()?;

    // Process each entity and create typed records
    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(entity_id)?;

        match entity.kind.as_str() {
            "File" => {
                if let Ok(file_node) = serde_json::from_value::<FileNode>(entity.data.clone()) {
                    records.push(JsonlRecord::File(FileExport {
                        path: file_node.path,
                        hash: file_node.hash,
                    }));
                }
            }
            "Symbol" => {
                if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(entity.data.clone()) {
                    let file = get_file_path_from_symbol(graph, entity_id)?;
                    records.push(JsonlRecord::Symbol(SymbolExport {
                        symbol_id: symbol_node.symbol_id,
                        name: symbol_node.name,
                        kind: symbol_node.kind,
                        kind_normalized: symbol_node.kind_normalized,
                        file,
                        byte_start: symbol_node.byte_start,
                        byte_end: symbol_node.byte_end,
                        start_line: symbol_node.start_line,
                        start_col: symbol_node.start_col,
                        end_line: symbol_node.end_line,
                        end_col: symbol_node.end_col,
                    }));
                }
            }
            "Reference" => {
                if let Ok(ref_node) = serde_json::from_value::<ReferenceNode>(entity.data.clone()) {
                    let referenced_symbol = entity
                        .name
                        .strip_prefix("ref to ")
                        .unwrap_or("")
                        .to_string();

                    records.push(JsonlRecord::Reference(ReferenceExport {
                        file: ref_node.file,
                        referenced_symbol,
                        target_symbol_id: None,
                        byte_start: ref_node.byte_start as usize,
                        byte_end: ref_node.byte_end as usize,
                        start_line: ref_node.start_line as usize,
                        start_col: ref_node.start_col as usize,
                        end_line: ref_node.end_line as usize,
                        end_col: ref_node.end_col as usize,
                    }));
                }
            }
            "Call" => {
                if let Ok(call_node) = serde_json::from_value::<CallNode>(entity.data.clone()) {
                    records.push(JsonlRecord::Call(CallExport {
                        file: call_node.file,
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
                    }));
                }
            }
            _ => {
                // Ignore unknown node types
            }
        }
    }

    // Sort deterministically before output
    records.sort_by(|a, b| match (a, b) {
        (JsonlRecord::File(a), JsonlRecord::File(b)) => a.path.cmp(&b.path),
        (JsonlRecord::Symbol(a), JsonlRecord::Symbol(b)) => (&a.file, &a.name).cmp(&(&b.file, &b.name)),
        (JsonlRecord::Reference(a), JsonlRecord::Reference(b)) => {
            (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol))
        }
        (JsonlRecord::Call(a), JsonlRecord::Call(b)) => {
            (&a.file, &a.caller, &a.callee).cmp(&(&b.file, &b.caller, &b.callee))
        }
        // Type ordering: File < Symbol < Reference < Call
        (JsonlRecord::File(_), _) => std::cmp::Ordering::Less,
        (_, JsonlRecord::File(_)) => std::cmp::Ordering::Greater,
        (JsonlRecord::Symbol(_), _) => std::cmp::Ordering::Less,
        (_, JsonlRecord::Symbol(_)) => std::cmp::Ordering::Greater,
        (JsonlRecord::Reference(_), _) => std::cmp::Ordering::Less,
        (_, JsonlRecord::Reference(_)) => std::cmp::Ordering::Greater,
    });

    // Serialize each record to compact JSON and join with newlines
    let lines: Result<Vec<String>, _> = records
        .iter()
        .map(|r| serde_json::to_string(r))
        .collect();
    let lines = lines?;

    Ok(lines.join("\n"))
}

/// Export graph data with configurable format and options
///
/// Dispatches to export_json() or export_jsonl() based on config.format.
/// Respects minify flag for JSON output.
///
/// # Arguments
/// * `graph` - The code graph to export
/// * `config` - Export configuration (format, minify, filters)
///
/// # Returns
/// JSON or JSONL string based on config.format
pub fn export_graph(graph: &mut CodeGraph, config: &ExportConfig) -> Result<String> {
    // Check if export should be empty based on filters
    let has_content = config.include_symbols || config.include_references || config.include_calls;

    if !has_content {
        // Return empty result of appropriate format
        return match config.format {
            ExportFormat::Json => {
                let empty = GraphExport {
                    files: Vec::new(),
                    symbols: Vec::new(),
                    references: Vec::new(),
                    calls: Vec::new(),
                };
                if config.minify {
                    serde_json::to_string(&empty).map_err(Into::into)
                } else {
                    serde_json::to_string_pretty(&empty).map_err(Into::into)
                }
            }
            ExportFormat::JsonL => Ok(String::new()),
            _ => Err(anyhow::anyhow!("Export format {:?} not yet implemented", config.format)),
        };
    }

    match config.format {
        ExportFormat::Json => {
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
                        if config.include_symbols {
                            if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(entity.data.clone()) {
                                let file = get_file_path_from_symbol(graph, entity_id)?;
                                symbols.push(SymbolExport {
                                    symbol_id: symbol_node.symbol_id,
                                    name: symbol_node.name,
                                    kind: symbol_node.kind,
                                    kind_normalized: symbol_node.kind_normalized,
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
                    }
                    "Reference" => {
                        if config.include_references {
                            if let Ok(ref_node) = serde_json::from_value::<ReferenceNode>(entity.data.clone()) {
                                let referenced_symbol = entity
                                    .name
                                    .strip_prefix("ref to ")
                                    .unwrap_or("")
                                    .to_string();

                                references.push(ReferenceExport {
                                    file: ref_node.file,
                                    referenced_symbol,
                                    target_symbol_id: None,
                                    byte_start: ref_node.byte_start as usize,
                                    byte_end: ref_node.byte_end as usize,
                                    start_line: ref_node.start_line as usize,
                                    start_col: ref_node.start_col as usize,
                                    end_line: ref_node.end_line as usize,
                                    end_col: ref_node.end_col as usize,
                                });
                            }
                        }
                    }
                    "Call" => {
                        if config.include_calls {
                            if let Ok(call_node) = serde_json::from_value::<CallNode>(entity.data.clone()) {
                                calls.push(CallExport {
                                    file: call_node.file,
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
                                });
                            }
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

            if config.minify {
                serde_json::to_string(&export).map_err(Into::into)
            } else {
                serde_json::to_string_pretty(&export).map_err(Into::into)
            }
        }
        ExportFormat::JsonL => export_jsonl(graph),
        _ => Err(anyhow::anyhow!("Export format {:?} not yet implemented", config.format)),
    }
}
