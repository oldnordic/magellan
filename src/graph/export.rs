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

pub mod scip;

use anyhow::Result;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlitegraph::{BackendDirection, NeighborQuery, SnapshotId};

use super::{CallNode, CodeGraph, FileNode, ReferenceNode, SymbolNode};
use crate::graph::query::{collision_groups, CollisionField};

/// Export format options
///
/// Dot, Csv, and Scip are available export formats.
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
}

impl ExportFormat {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(ExportFormat::Json),
            "jsonl" => Some(ExportFormat::JsonL),
            "dot" => Some(ExportFormat::Dot),
            "csv" => Some(ExportFormat::Csv),
            "scip" => Some(ExportFormat::Scip),
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

/// Escape a string for use as a DOT label
///
/// DOT labels must be wrapped in double quotes and escape special characters.
/// According to the DOT specification:
/// - Backslashes must be escaped as \\
/// - Double quotes must be escaped as \"
/// - Newlines can be represented as \n for labels
///
/// # Arguments
/// * `s` - The string to escape
///
/// # Returns
/// A quoted and escaped string suitable for use as a DOT label
fn escape_dot_label(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', r#"\""#)
            .replace('\n', "\\n")
    )
}

/// Create a valid DOT identifier from a string
///
/// DOT identifiers should not contain special characters.
/// If symbol_id is available, it's used as a stable identifier.
/// Otherwise, falls back to a sanitized name.
///
/// # Arguments
/// * `symbol_id` - Optional stable symbol ID
/// * `name` - Symbol name to use as fallback
///
/// # Returns
/// A valid DOT identifier string
fn escape_dot_id(symbol_id: &Option<String>, name: &str) -> String {
    if let Some(ref id) = symbol_id {
        // SHA-256 based IDs are already safe (hex only)
        id.clone()
    } else {
        // Sanitize name: replace non-alphanumeric with underscore
        name.chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::query::CollisionField;

    #[test]
    fn test_escape_dot_label_basic() {
        assert_eq!(escape_dot_label("simple"), "\"simple\"");
        assert_eq!(escape_dot_label("with spaces"), "\"with spaces\"");
    }

    #[test]
    fn test_escape_dot_label_quotes() {
        assert_eq!(escape_dot_label("say \"hello\""), r#""say \"hello\"""#);
    }

    #[test]
    fn test_escape_dot_label_backslash() {
        assert_eq!(escape_dot_label(r"C:\path"), r#""C:\\path""#);
        assert_eq!(escape_dot_label("a\\b"), r#""a\\b""#);
    }

    #[test]
    fn test_escape_dot_label_newlines() {
        assert_eq!(escape_dot_label("line1\nline2"), r#""line1\nline2""#);
    }

    #[test]
    fn test_escape_dot_label_empty() {
        assert_eq!(escape_dot_label(""), "\"\"");
    }

    #[test]
    fn test_escape_dot_label_special_chars() {
        // Tabs and other special characters
        assert_eq!(escape_dot_label("a\tb"), "\"a\tb\"");
        // Unicode characters should pass through
        assert_eq!(escape_dot_label("hello世界"), "\"hello世界\"");
    }

    #[test]
    fn test_escape_dot_id_with_symbol_id() {
        // Symbol ID (hex) is used directly
        let symbol_id = Some("a1b2c3d4e5f6".to_string());
        assert_eq!(escape_dot_id(&symbol_id, "fallback"), "a1b2c3d4e5f6");
    }

    #[test]
    fn test_escape_dot_id_without_symbol_id() {
        // Falls back to sanitized name
        assert_eq!(escape_dot_id(&None, "simple_name"), "simple_name");
        assert_eq!(escape_dot_id(&None, "name-with-dashes"), "name_with_dashes");
        assert_eq!(escape_dot_id(&None, "name.with.dots"), "name_with_dots");
        assert_eq!(escape_dot_id(&None, "name with spaces"), "name_with_spaces");
    }

    #[test]
    fn test_escape_dot_id_empty_name() {
        assert_eq!(escape_dot_id(&None, ""), "");
    }

    #[test]
    #[cfg(not(feature = "native-v2"))]
    fn test_export_collisions_included_when_enabled() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = CodeGraph::open(&db_path).unwrap();

        let file1 = temp_dir.path().join("file1.rs");
        std::fs::write(&file1, "fn collide() {}\n").unwrap();
        let file2 = temp_dir.path().join("file2.rs");
        std::fs::write(&file2, "fn collide() {}\n").unwrap();

        let path1 = file1.to_string_lossy().to_string();
        let path2 = file2.to_string_lossy().to_string();
        let source1 = std::fs::read(&file1).unwrap();
        let source2 = std::fs::read(&file2).unwrap();

        graph.index_file(&path1, &source1).unwrap();
        graph.index_file(&path2, &source2).unwrap();

        let config = ExportConfig {
            format: ExportFormat::Json,
            include_symbols: true,
            include_references: false,
            include_calls: false,
            minify: false,
            filters: ExportFilters::default(),
            include_collisions: true,
            collisions_field: CollisionField::Fqn,
        };

        let json = export_graph(&mut graph, &config).unwrap();
        let export: GraphExport = serde_json::from_str(&json).unwrap();
        assert!(!export.collisions.is_empty());
    }

    #[test]
    fn test_csv_export_mixed_record_types() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Create a file with symbols, references, and calls
        let file1 = temp_dir.path().join("test.rs");
        std::fs::write(
            &file1,
            r#"
fn main() {
    println!("hello");
    helper();
}

fn helper() {}
"#,
        )
        .unwrap();

        let path1 = file1.to_string_lossy().to_string();
        let source1 = std::fs::read(&file1).unwrap();
        graph.index_file(&path1, &source1).unwrap();

        // Export to CSV with all record types
        let config = ExportConfig {
            format: ExportFormat::Csv,
            include_symbols: true,
            include_references: true,
            include_calls: true,
            minify: false,
            filters: ExportFilters::default(),
            include_collisions: false,
            collisions_field: CollisionField::Fqn,
        };

        let csv = export_graph(&mut graph, &config).unwrap();

        // Verify CSV output
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines.len() > 1, "CSV should have header + data rows");

        // Check header contains all expected columns
        // The first line is a comment, so find the actual CSV header
        let header = lines.iter().find(|line| !line.starts_with('#') && !line.is_empty())
            .expect("Should have a CSV header row");
        assert!(header.contains("record_type"));
        assert!(header.contains("file"));
        assert!(header.contains("symbol_id"));
        assert!(header.contains("name"));
        assert!(header.contains("kind"));
        assert!(header.contains("referenced_symbol"));
        assert!(header.contains("target_symbol_id"));
        assert!(header.contains("caller"));
        assert!(header.contains("callee"));
        assert!(header.contains("caller_symbol_id"));
        assert!(header.contains("callee_symbol_id"));

        // Verify all data rows have the same number of columns
        let header_cols: Vec<&str> = header.split(',').collect();
        let expected_col_count = header_cols.len();

        for (i, line) in lines.iter().skip(1).enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let data_cols: Vec<&str> = line.split(',').collect();
            assert_eq!(
                data_cols.len(),
                expected_col_count,
                "Row {} has {} columns, expected {}",
                i + 2,
                data_cols.len(),
                expected_col_count
            );
        }

        // Verify version header is present
        assert!(csv.starts_with("# Magellan Export Version: 2.0.0"));
    }
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
    let collisions = Vec::new();

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
        version: "2.0.0".to_string(), // v1.5 adds symbol_id, canonical_fqn, display_fqn
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
            "Symbol" => {
                if config.include_symbols {
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
            }
            "Reference" => {
                if config.include_references {
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
            "Symbol" => {
                if config.include_symbols {
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
            }
            "Reference" => {
                if config.include_references {
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

/// JSONL record type discriminator
///
/// Each JSONL line includes a "type" field to identify the record type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum JsonlRecord {
    Version { version: String },
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

    // Add version record first
    records.push(JsonlRecord::Version {
        version: "2.0.0".to_string(),
    });

    // Get all entity IDs from the graph
    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    // Process each entity and create typed records
    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;

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
        (JsonlRecord::Version { .. }, _) => std::cmp::Ordering::Less,
        (_, JsonlRecord::Version { .. }) => std::cmp::Ordering::Greater,
        (JsonlRecord::File(a), JsonlRecord::File(b)) => a.path.cmp(&b.path),
        (JsonlRecord::Symbol(a), JsonlRecord::Symbol(b)) => {
            (&a.file, &a.name).cmp(&(&b.file, &b.name))
        }
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
    let lines: Result<Vec<String>, _> = records.iter().map(|r| serde_json::to_string(r)).collect();
    let lines = lines?;

    Ok(lines.join("\n"))
}

/// Stream all graph data to JSONL format with reduced memory footprint
///
/// This function writes JSONL incrementally to avoid loading all data into memory.
/// Each line is written as it's serialized, reducing peak memory for large graphs.
///
/// # Arguments
/// * `graph` - The code graph to export
/// * `config` - Export configuration (include_symbols, include_references, include_calls)
/// * `writer` - Writer to receive JSONL output
///
/// # Returns
/// Result indicating success or failure
pub fn stream_ndjson<W: std::io::Write>(
    graph: &mut CodeGraph,
    config: &ExportConfig,
    writer: &mut W,
) -> Result<()> {
    let mut records = Vec::new();

    // Add version record first
    records.push(JsonlRecord::Version {
        version: "2.0.0".to_string(),
    });

    // Get all entity IDs from the graph
    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    // Process each entity and create typed records
    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;

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
                if config.include_symbols {
                    if let Ok(symbol_node) =
                        serde_json::from_value::<SymbolNode>(entity.data.clone())
                    {
                        let file = get_file_path_from_symbol(graph, entity_id)?;
                        records.push(JsonlRecord::Symbol(SymbolExport {
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
                        }));
                    }
                }
            }
            "Reference" => {
                if config.include_references {
                    if let Ok(ref_node) =
                        serde_json::from_value::<ReferenceNode>(entity.data.clone())
                    {
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
            }
            "Call" => {
                if config.include_calls {
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
            }
            _ => {
                // Ignore unknown node types
            }
        }
    }

    // Sort deterministically before output
    records.sort_by(|a, b| match (a, b) {
        (JsonlRecord::Version { .. }, _) => std::cmp::Ordering::Less,
        (_, JsonlRecord::Version { .. }) => std::cmp::Ordering::Greater,
        (JsonlRecord::File(a), JsonlRecord::File(b)) => a.path.cmp(&b.path),
        (JsonlRecord::Symbol(a), JsonlRecord::Symbol(b)) => {
            (&a.file, &a.name).cmp(&(&b.file, &b.name))
        }
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

    // Write each record line by line (streaming)
    let mut first = true;
    for record in records {
        if !first {
            writeln!(&mut *writer)?;
        }
        serde_json::to_writer(&mut *writer, &record)
            .map_err(|e| anyhow::anyhow!("JSON serialization error: {}", e))?;
        first = false;
    }

    Ok(())
}

/// Export call graph to DOT (Graphviz) format
///
/// Generates a DOT digraph representing the call graph with nodes as symbols
/// and edges as call relationships. Output is deterministic for reproducibility.
///
/// # Arguments
/// * `graph` - The code graph to export
/// * `config` - Export configuration with filters
///
/// # Returns
/// DOT format string suitable for Graphviz tools
///
/// # DOT Format Details
/// - Uses "strict digraph" for deterministic output
/// - Node labels: "{symbol_name}\n{file_path}" (newline for readability)
/// - Uses symbol_id as internal identifier if available, fallback to sanitized name
/// - Clusters nodes by file if config.filters.cluster is true
pub fn export_dot(graph: &mut CodeGraph, config: &ExportConfig) -> Result<String> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut dot_output = String::from("strict digraph call_graph {\n");
    dot_output.push_str("  node [shape=box, style=rounded];\n");

    // Collect all Call nodes from the graph
    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = SnapshotId::current();
    let mut calls = Vec::new();

    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;
        if entity.kind == "Call" {
            if let Ok(call_node) = serde_json::from_value::<CallNode>(entity.data) {
                calls.push(call_node);
            }
        }
    }

    // Apply filters
    if let Some(ref file_filter) = config.filters.file {
        calls.retain(|c| c.file.contains(file_filter));
    }
    if let Some(ref symbol_filter) = config.filters.symbol {
        calls.retain(|c| c.caller.contains(symbol_filter) || c.callee.contains(symbol_filter));
    }

    // Sort deterministically: file, then caller, then callee
    calls.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.caller.cmp(&b.caller))
            .then_with(|| a.callee.cmp(&b.callee))
    });

    // Collect unique nodes and organize by file if clustering
    let mut nodes: BTreeSet<(String, String)> = BTreeSet::new(); // (symbol_id_or_name, label)
    let mut file_to_nodes: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

    for call in &calls {
        for (name, symbol_id) in [
            (call.caller.as_str(), call.caller_symbol_id.as_ref()),
            (call.callee.as_str(), call.callee_symbol_id.as_ref()),
        ] {
            let node_id = escape_dot_id(&symbol_id.cloned(), name);
            let label = format!(
                "{}\\n{}",
                escape_dot_label(name),
                escape_dot_label(&call.file)
            );
            nodes.insert((node_id.clone(), label.clone()));

            if config.filters.cluster {
                file_to_nodes
                    .entry(call.file.clone())
                    .or_insert_with(Vec::new)
                    .push((node_id, label));
            }
        }
    }

    // Emit edges
    if config.filters.cluster {
        // Group nodes by file into subgraphs
        for (file, file_nodes) in &file_to_nodes {
            // Create a sanitized cluster ID from file path
            let cluster_id = file
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect::<String>();

            dot_output.push_str(&format!("  subgraph cluster_{} {{\n", cluster_id));
            dot_output.push_str(&format!("    label = {};\n", escape_dot_label(file)));
            dot_output.push_str("    style = dashed;\n");

            // Deduplicate nodes within this file
            let mut seen = BTreeSet::new();
            for (node_id, label) in file_nodes {
                if seen.insert(node_id.clone()) {
                    dot_output.push_str(&format!("    {} [label={}];\n", node_id, label));
                }
            }

            dot_output.push_str("  }\n");
        }
    } else {
        // Emit all nodes at top level
        for (node_id, label) in &nodes {
            dot_output.push_str(&format!("  {} [label={}];\n", node_id, label));
        }
    }

    // Emit edges
    for call in &calls {
        let caller_id = escape_dot_id(&call.caller_symbol_id, &call.caller);
        let callee_id = escape_dot_id(&call.callee_symbol_id, &call.callee);
        dot_output.push_str(&format!("  {} -> {};\n", caller_id, callee_id));
    }

    dot_output.push_str("}\n");

    Ok(dot_output)
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
                    "Symbol" => {
                        if config.include_symbols {
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
                    }
                    "Reference" => {
                        if config.include_references {
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
                    }
                    "Call" => {
                        if config.include_calls {
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
    }
}

// ============================================================================
// CSV Export
// ============================================================================

/// Unified CSV row for all record types
///
/// Single struct with optional fields for different record types ensures
/// consistent CSV headers across Symbol, Reference, and Call records.
///
/// NOTE: We do NOT use `skip_serializing_if` on optional fields because
/// the CSV crate writes headers based on the first record. If we skip fields,
/// subsequent records with different field sets will fail with "found record
/// with X fields, but the previous record has Y fields". Instead, we always
/// write all fields (empty strings for None values) to ensure consistent headers.
#[derive(Debug, Clone, Serialize)]
struct UnifiedCsvRow {
    // Universal fields (always present)
    record_type: String,
    file: String,
    byte_start: usize,
    byte_end: usize,
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,

    // Symbol-specific (optional, but always serialized as empty string if None)
    symbol_id: Option<String>,
    name: Option<String>,
    kind: Option<String>,
    kind_normalized: Option<String>,

    // Reference-specific (optional, but always serialized as empty string if None)
    referenced_symbol: Option<String>,
    target_symbol_id: Option<String>,

    // Call-specific (optional, but always serialized as empty string if None)
    caller: Option<String>,
    callee: Option<String>,
    caller_symbol_id: Option<String>,
    callee_symbol_id: Option<String>,
}

/// Export graph data to CSV format
///
/// Produces a combined CSV with a record_type column for discrimination.
/// Uses the csv crate for proper RFC 4180 compliance (quoting, escaping).
///
/// # Returns
/// CSV string with all requested entities, deterministically sorted
pub fn export_csv(graph: &mut CodeGraph, config: &ExportConfig) -> Result<String> {
    let mut records: Vec<UnifiedCsvRow> = Vec::new();

    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;

        match entity.kind.as_str() {
            "Symbol" => {
                if config.include_symbols {
                    if let Ok(symbol_node) =
                        serde_json::from_value::<SymbolNode>(entity.data.clone())
                    {
                        let file = get_file_path_from_symbol(graph, entity_id)?;
                        records.push(UnifiedCsvRow {
                            record_type: "Symbol".to_string(),
                            file,
                            byte_start: symbol_node.byte_start,
                            byte_end: symbol_node.byte_end,
                            start_line: symbol_node.start_line,
                            start_col: symbol_node.start_col,
                            end_line: symbol_node.end_line,
                            end_col: symbol_node.end_col,
                            symbol_id: symbol_node.symbol_id,
                            name: symbol_node.name,
                            kind: Some(symbol_node.kind),
                            kind_normalized: symbol_node.kind_normalized,
                            referenced_symbol: None,
                            target_symbol_id: None,
                            caller: None,
                            callee: None,
                            caller_symbol_id: None,
                            callee_symbol_id: None,
                        });
                    }
                }
            }
            "Reference" => {
                if config.include_references {
                    if let Ok(ref_node) =
                        serde_json::from_value::<ReferenceNode>(entity.data.clone())
                    {
                        let referenced_symbol = entity
                            .name
                            .strip_prefix("ref to ")
                            .unwrap_or("")
                            .to_string();

                        records.push(UnifiedCsvRow {
                            record_type: "Reference".to_string(),
                            file: ref_node.file,
                            byte_start: ref_node.byte_start as usize,
                            byte_end: ref_node.byte_end as usize,
                            start_line: ref_node.start_line as usize,
                            start_col: ref_node.start_col as usize,
                            end_line: ref_node.end_line as usize,
                            end_col: ref_node.end_col as usize,
                            symbol_id: None,
                            name: None,
                            kind: None,
                            kind_normalized: None,
                            referenced_symbol: Some(referenced_symbol),
                            target_symbol_id: None,
                            caller: None,
                            callee: None,
                            caller_symbol_id: None,
                            callee_symbol_id: None,
                        });
                    }
                }
            }
            "Call" => {
                if config.include_calls {
                    if let Ok(call_node) = serde_json::from_value::<CallNode>(entity.data.clone()) {
                        records.push(UnifiedCsvRow {
                            record_type: "Call".to_string(),
                            file: call_node.file,
                            byte_start: call_node.byte_start as usize,
                            byte_end: call_node.byte_end as usize,
                            start_line: call_node.start_line as usize,
                            start_col: call_node.start_col as usize,
                            end_line: call_node.end_line as usize,
                            end_col: call_node.end_col as usize,
                            symbol_id: None,
                            name: None,
                            kind: None,
                            kind_normalized: None,
                            referenced_symbol: None,
                            target_symbol_id: None,
                            caller: Some(call_node.caller),
                            callee: Some(call_node.callee),
                            caller_symbol_id: call_node.caller_symbol_id,
                            callee_symbol_id: call_node.callee_symbol_id,
                        });
                    }
                }
            }
            _ => {
                // Ignore File and unknown node types for CSV export
            }
        }
    }

    // Sort deterministically by record_type, then by type-specific fields
    records.sort_by(|a, b| {
        // First by record type
        let type_order = match (a.record_type.as_str(), b.record_type.as_str()) {
            ("Call", "Call") => std::cmp::Ordering::Equal,
            ("Call", "Reference") => std::cmp::Ordering::Greater,
            ("Call", "Symbol") => std::cmp::Ordering::Greater,
            ("Reference", "Call") => std::cmp::Ordering::Less,
            ("Reference", "Reference") => std::cmp::Ordering::Equal,
            ("Reference", "Symbol") => std::cmp::Ordering::Greater,
            ("Symbol", "Call") => std::cmp::Ordering::Less,
            ("Symbol", "Reference") => std::cmp::Ordering::Less,
            ("Symbol", "Symbol") => std::cmp::Ordering::Equal,
            _ => std::cmp::Ordering::Equal,
        };

        if type_order != std::cmp::Ordering::Equal {
            return type_order;
        }

        // Within same type, sort by applicable fields
        match a.record_type.as_str() {
            "Symbol" => (&a.file, a.name.as_ref().unwrap_or(&String::new()))
                .cmp(&(&b.file, b.name.as_ref().unwrap_or(&String::new()))),
            "Reference" => (&a.record_type, &a.file, a.referenced_symbol.as_ref().unwrap_or(&String::new()))
                .cmp(&(&b.record_type, &b.file, b.referenced_symbol.as_ref().unwrap_or(&String::new()))),
            "Call" => (&a.record_type, &a.file, a.caller.as_ref().unwrap_or(&String::new()), a.callee.as_ref().unwrap_or(&String::new()))
                .cmp(&(&b.record_type, &b.file, b.caller.as_ref().unwrap_or(&String::new()), b.callee.as_ref().unwrap_or(&String::new()))),
            _ => std::cmp::Ordering::Equal,
        }
    });

    // Write to buffer using csv::Writer
    let mut buffer = Vec::new();

    // Add version header comment
    use std::io::Write;
    writeln!(buffer, "# Magellan Export Version: 2.0.0")?;

    {
        let mut writer = csv::Writer::from_writer(&mut buffer);
        for record in records {
            writer.serialize(record)?;
        }
        writer.flush()?;
    }

    String::from_utf8(buffer).map_err(|e| anyhow::anyhow!("CSV output is not valid UTF-8: {}", e))
}
