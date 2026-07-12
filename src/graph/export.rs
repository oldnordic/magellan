//! Export functionality for CodeGraph
//!
//! Exports graph data to JSON/JSONL/CSV/SCIP format for LLM and pipeline consumption.
//!
//! # Export Schema Versioning
//!
//! All export formats include a version field for parsing stability:
//!
//! | Version | Changes |
//! |---------|---------|
//! | 2.0.0 | Added `symbol_id`, `canonical_fqn`, `display_fqn` fields |
//!
//! - **JSON**: Top-level `version` field
//! - **JSONL**: First line is `{"type":"Version","version":"2.0.0"}`
//! - **CSV**: Header comment `# Magellan Export Version: 2.0.0`
//!
//! See MANUAL.md section 3.8 for detailed export documentation.

mod csv;
mod dot;
mod ndjson;
pub mod scip;

use anyhow::Result;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlitegraph::{BackendDirection, NeighborQuery, SnapshotId};

use super::{CallNode, CodeGraph, FileNode, ReferenceNode, SymbolNode};
use crate::graph::query::{collision_groups, CollisionField};

pub use csv::export_csv;
pub use dot::export_dot;
#[cfg(test)]
use dot::{escape_dot_id, escape_dot_label};
pub use ndjson::{export_jsonl, stream_ndjson};

/// Export format options
///
/// Dot, Csv, Scip, Lsif, and Impact are available export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Standard JSON array format
    Json,
    /// JSON Lines format (one JSON record per line)
    JsonL,
    /// Graphviz DOT format
    Dot,
    /// CSV format
    Csv,
    /// SCIP (Source Code Intelligence Protocol) binary format
    Scip,
    /// LSIF (Language Server Index Format) for cross-repo navigation
    Lsif,
    /// Impact analysis format (blast radius for a symbol)
    Impact,
}

impl ExportFormat {
    /// Parse from string
    pub fn parse_format(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(ExportFormat::Json),
            "jsonl" => Some(ExportFormat::JsonL),
            "dot" => Some(ExportFormat::Dot),
            "csv" => Some(ExportFormat::Csv),
            "scip" => Some(ExportFormat::Scip),
            "lsif" => Some(ExportFormat::Lsif),
            "impact" => Some(ExportFormat::Impact),
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
    /// Include collision groups in JSON export
    pub include_collisions: bool,
    /// Field used to group collisions
    pub collisions_field: CollisionField,
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

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
impl Default for ExportConfig {
    fn default() -> Self {
        ExportConfig {
            format: ExportFormat::Json,
            include_symbols: true,
            include_references: true,
            include_calls: true,
            minify: false,
            filters: ExportFilters::default(),
            include_collisions: false,
            collisions_field: CollisionField::Fqn,
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
    /// Export schema version for parsing stability
    pub version: String,
    pub files: Vec<FileExport>,
    pub symbols: Vec<SymbolExport>,
    pub references: Vec<ReferenceExport>,
    pub calls: Vec<CallExport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collisions: Vec<CollisionExport>,
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

    /// Canonical fully-qualified name for unambiguous identity
    #[serde(default)]
    pub canonical_fqn: Option<String>,

    /// Display fully-qualified name for human-readable output
    #[serde(default)]
    pub display_fqn: Option<String>,

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

/// Collision candidate entry for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionCandidateExport {
    pub entity_id: i64,
    pub symbol_id: Option<String>,
    pub canonical_fqn: Option<String>,
    pub display_fqn: Option<String>,
    pub name: Option<String>,
    pub file_path: Option<String>,
}

/// Collision group entry for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionExport {
    pub field: String,
    pub value: String,
    pub count: usize,
    pub candidates: Vec<CollisionCandidateExport>,
}

fn build_collision_exports(
    graph: &mut CodeGraph,
    field: CollisionField,
    limit: usize,
) -> Result<Vec<CollisionExport>> {
    let groups = collision_groups(graph, field, limit)?;
    let mut exports = Vec::new();

    for group in groups {
        let candidates = group
            .candidates
            .into_iter()
            .map(|candidate| CollisionCandidateExport {
                entity_id: candidate.entity_id,
                symbol_id: candidate.symbol_id,
                canonical_fqn: candidate.canonical_fqn,
                display_fqn: candidate.display_fqn,
                name: candidate.name,
                file_path: candidate.file_path,
            })
            .collect();

        exports.push(CollisionExport {
            field: group.field,
            value: group.value,
            count: group.count,
            candidates,
        });
    }

    Ok(exports)
}

/// Export all graph data to JSON format
///
/// Note: This function loads all data into memory before serialization.
/// For large graphs, use stream_json() instead to reduce peak memory.
///
/// # Returns
/// JSON string containing all files, symbols, references, and calls
pub fn export_json(graph: &mut CodeGraph) -> Result<String> {
    let mut files = Vec::new();
    let mut symbols = Vec::new();
    let mut references = Vec::new();
    let mut calls = Vec::new();
    let collisions: Vec<CollisionExport> = Vec::new();

    // Get all entity IDs from the graph
    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    // Process each entity
    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;

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
                        canonical_fqn: symbol_node.canonical_fqn,
                        display_fqn: symbol_node.display_fqn,
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
        version: "2.0.0".to_string(),
        files,
        symbols,
        references,
        calls,
        collisions,
    };

    Ok(serde_json::to_string_pretty(&export)?)
}

/// Stream all graph data to JSON format with reduced memory footprint
///
/// This function writes JSON incrementally to avoid loading all data into memory.
/// It collects entities into vectors for sorting (deterministic output), but uses
/// serde_json::to_writer for streaming serialization instead of to_string.
///
/// # Arguments
/// * `graph` - The code graph to export
/// * `config` - Export configuration (include_symbols, include_references, include_calls)
/// * `writer` - Writer to receive JSON output
///
/// # Returns
/// Result indicating success or failure
pub fn stream_json<W: std::io::Write>(
    graph: &mut CodeGraph,
    config: &ExportConfig,
    writer: &mut W,
) -> Result<()> {
    let mut files = Vec::new();
    let mut symbols = Vec::new();
    let mut references = Vec::new();
    let mut calls = Vec::new();
    let mut collisions = Vec::new();

    // Get all entity IDs from the graph
    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    // Process each entity
    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;

        match entity.kind.as_str() {
            "File" => {
                if let Ok(file_node) = serde_json::from_value::<FileNode>(entity.data.clone()) {
                    files.push(FileExport {
                        path: file_node.path,
                        hash: file_node.hash,
                    });
                }
            }
            "Symbol" if config.include_symbols => {
                if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(entity.data.clone()) {
                    let file = get_file_path_from_symbol(graph, entity_id)?;
                    symbols.push(SymbolExport {
                        symbol_id: symbol_node.symbol_id,
                        canonical_fqn: symbol_node.canonical_fqn,
                        display_fqn: symbol_node.display_fqn,
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
            "Reference" if config.include_references => {
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
            "Call" if config.include_calls => {
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

    if config.include_collisions {
        collisions = build_collision_exports(graph, config.collisions_field, usize::MAX)?;
    }

    // Sort for deterministic output
    files.sort_by(|a, b| a.path.cmp(&b.path));
    symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));
    references
        .sort_by(|a, b| (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol)));
    calls.sort_by(|a, b| (&a.file, &a.caller, &a.callee).cmp(&(&b.file, &b.caller, &b.callee)));

    let export = GraphExport {
        version: "2.0.0".to_string(), // v1.5 adds symbol_id, canonical_fqn, display_fqn
        files,
        symbols,
        references,
        calls,
        collisions,
    };

    // Stream to writer instead of returning String
    serde_json::to_writer_pretty(writer, &export).map_err(Into::into)
}

/// Stream all graph data to JSON format with minified output
///
/// This function writes JSON incrementally to avoid loading all data into memory.
/// Uses compact serialization (no pretty-printing) for smaller output size.
///
/// # Arguments
/// * `graph` - The code graph to export
/// * `config` - Export configuration (include_symbols, include_references, include_calls)
/// * `writer` - Writer to receive JSON output
///
/// # Returns
/// Result indicating success or failure
pub fn stream_json_minified<W: std::io::Write>(
    graph: &mut CodeGraph,
    config: &ExportConfig,
    writer: &mut W,
) -> Result<()> {
    let mut files = Vec::new();
    let mut symbols = Vec::new();
    let mut references = Vec::new();
    let mut calls = Vec::new();
    let mut collisions = Vec::new();

    // Get all entity IDs from the graph
    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    // Process each entity
    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;

        match entity.kind.as_str() {
            "File" => {
                if let Ok(file_node) = serde_json::from_value::<FileNode>(entity.data.clone()) {
                    files.push(FileExport {
                        path: file_node.path,
                        hash: file_node.hash,
                    });
                }
            }
            "Symbol" if config.include_symbols => {
                if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(entity.data.clone()) {
                    let file = get_file_path_from_symbol(graph, entity_id)?;
                    symbols.push(SymbolExport {
                        symbol_id: symbol_node.symbol_id,
                        canonical_fqn: symbol_node.canonical_fqn,
                        display_fqn: symbol_node.display_fqn,
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
            "Reference" if config.include_references => {
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
            "Call" if config.include_calls => {
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

    if config.include_collisions {
        collisions = build_collision_exports(graph, config.collisions_field, usize::MAX)?;
    }

    // Sort for deterministic output
    files.sort_by(|a, b| a.path.cmp(&b.path));
    symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));
    references
        .sort_by(|a, b| (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol)));
    calls.sort_by(|a, b| (&a.file, &a.caller, &a.callee).cmp(&(&b.file, &b.caller, &b.callee)));

    let export = GraphExport {
        version: "2.0.0".to_string(), // v1.5 adds symbol_id, canonical_fqn, display_fqn
        files,
        symbols,
        references,
        calls,
        collisions,
    };

    // Stream to writer using compact serialization (minified)
    serde_json::to_writer(writer, &export).map_err(Into::into)
}

/// Get the file path for a symbol by following DEFINES edge
fn get_file_path_from_symbol(graph: &mut CodeGraph, symbol_id: i64) -> Result<String> {
    // Query incoming DEFINES edges to find the File node
    let snapshot = SnapshotId::current();
    let file_ids = graph.files.backend.neighbors(
        snapshot,
        symbol_id,
        NeighborQuery {
            direction: BackendDirection::Incoming,
            edge_type: Some("DEFINES".to_string()),
        },
    )?;

    if let Some(file_id) = file_ids.first() {
        let entity = graph.files.backend.get_node(snapshot, *file_id)?;
        if entity.kind == "File" {
            if let Ok(file_node) = serde_json::from_value::<FileNode>(entity.data) {
                return Ok(file_node.path);
            }
        }
    }

    // Fallback: return empty string if no file found
    Ok(String::new())
}

/// Export graph data with configurable format and options
///
/// Dispatches to export_json(), export_jsonl(), or export_dot() based on config.format.
/// Respects minify flag for JSON output.
///
/// # Arguments
/// * `graph` - The code graph to export
/// * `config` - Export configuration (format, minify, filters)
///
/// # Returns
/// JSON, JSONL, or DOT string based on config.format
pub fn export_graph(graph: &mut CodeGraph, config: &ExportConfig) -> Result<String> {
    // Check if export should be empty based on filters
    let has_content = config.include_symbols || config.include_references || config.include_calls;

    if !has_content {
        // Return empty result of appropriate format
        return match config.format {
            ExportFormat::Json => {
                let empty = GraphExport {
                    version: "2.0.0".to_string(),
                    files: Vec::new(),
                    symbols: Vec::new(),
                    references: Vec::new(),
                    calls: Vec::new(),
                    collisions: Vec::new(),
                };
                if config.minify {
                    serde_json::to_string(&empty).map_err(Into::into)
                } else {
                    serde_json::to_string_pretty(&empty).map_err(Into::into)
                }
            }
            ExportFormat::JsonL => Ok(String::new()),
            ExportFormat::Dot => {
                // Empty DOT graph
                Ok("strict digraph call_graph {\n}\n".to_string())
            }
            _ => Err(anyhow::anyhow!(
                "Export format {:?} not yet implemented",
                config.format
            )),
        };
    }

    match config.format {
        ExportFormat::Json => {
            let mut files = Vec::new();
            let mut symbols = Vec::new();
            let mut references = Vec::new();
            let mut calls = Vec::new();
            let mut collisions = Vec::new();

            // Get all entity IDs from the graph
            let entity_ids = graph.files.backend.entity_ids()?;
            let snapshot = SnapshotId::current();

            // Process each entity
            for entity_id in entity_ids {
                let entity = graph.files.backend.get_node(snapshot, entity_id)?;

                match entity.kind.as_str() {
                    "File" => {
                        if let Ok(file_node) =
                            serde_json::from_value::<FileNode>(entity.data.clone())
                        {
                            files.push(FileExport {
                                path: file_node.path,
                                hash: file_node.hash,
                            });
                        }
                    }
                    "Symbol" if config.include_symbols => {
                        if let Ok(symbol_node) =
                            serde_json::from_value::<SymbolNode>(entity.data.clone())
                        {
                            let file = get_file_path_from_symbol(graph, entity_id)?;
                            symbols.push(SymbolExport {
                                symbol_id: symbol_node.symbol_id,
                                canonical_fqn: symbol_node.canonical_fqn,
                                display_fqn: symbol_node.display_fqn,
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
                    "Reference" if config.include_references => {
                        if let Ok(ref_node) =
                            serde_json::from_value::<ReferenceNode>(entity.data.clone())
                        {
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
                    "Call" if config.include_calls => {
                        if let Ok(call_node) =
                            serde_json::from_value::<CallNode>(entity.data.clone())
                        {
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

            if config.include_collisions {
                collisions = build_collision_exports(graph, config.collisions_field, usize::MAX)?;
            }

            // Sort for deterministic output
            files.sort_by(|a, b| a.path.cmp(&b.path));
            symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));
            references.sort_by(|a, b| {
                (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol))
            });
            calls.sort_by(|a, b| {
                (&a.file, &a.caller, &a.callee).cmp(&(&b.file, &b.caller, &b.callee))
            });

            let export = GraphExport {
                version: "2.0.0".to_string(), // v1.5 adds symbol_id, canonical_fqn, display_fqn
                files,
                symbols,
                references,
                calls,
                collisions,
            };

            if config.minify {
                serde_json::to_string(&export).map_err(Into::into)
            } else {
                serde_json::to_string_pretty(&export).map_err(Into::into)
            }
        }
        ExportFormat::JsonL => export_jsonl(graph),
        ExportFormat::Dot => export_dot(graph, config),
        ExportFormat::Csv => export_csv(graph, config),
        ExportFormat::Scip => {
            // SCIP export is binary, not text - use separate function
            let scip_config = self::scip::ScipExportConfig {
                project_root: ".".to_string(),
                project_name: None,
                version: None,
            };
            let scip_bytes = self::scip::export_scip(graph, &scip_config)?;

            // Return base64-encoded SCIP data as a workaround for text-based export_graph
            // For direct binary output, use export_cmd.rs which handles SCIP specially
            Ok(base64::engine::general_purpose::STANDARD.encode(&scip_bytes))
        }
        ExportFormat::Lsif => {
            // LSIF export returns JSONL format
            // For file output, use export_cmd.rs which handles LSIF specially
            Ok(String::new())
        }
        ExportFormat::Impact => {
            // Impact export requires --symbol parameter and is handled separately in export_cmd.rs
            Err(anyhow::anyhow!(
                "Impact export requires --symbol parameter. Use: magellan export --db code.db --format impact --symbol <name>"
            ))
        }
    }
}
