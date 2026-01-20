//! SCIP export functionality
//!
//! Implements SCIP (Source Code Intelligence Protocol) export for Magellan.
//! SCIP is a language-agnostic protocol for code indexing defined by Sourcegraph.

use anyhow::Result;
use protobuf::{EnumOrUnknown, Message};
use scip::types::{
    symbol_information::Kind, Document, Index, Metadata, Occurrence, SymbolInformation,
    SymbolRole, PositionEncoding,
};
use std::collections::HashMap;

use crate::graph::schema::SymbolNode;
use crate::ingest::detect::detect_language;

use super::CodeGraph;

// Import the GraphBackend trait for backend methods
use sqlitegraph::GraphBackend;

/// SCIP export configuration
#[derive(Debug, Clone)]
pub struct ScipExportConfig {
    /// Project root for SCIP metadata
    pub project_root: String,
    /// Optional project name
    pub project_name: Option<String>,
    /// Optional version string
    pub version: Option<String>,
}

impl Default for ScipExportConfig {
    fn default() -> Self {
        Self {
            project_root: ".".to_string(),
            project_name: None,
            version: None,
        }
    }
}

/// Convert Magellan FQN to SCIP symbol format
///
/// SCIP symbol format: `scheme package/descriptor1/descriptor2/symbol.`
/// For Magellan, we use "magellan" as scheme and language as package.
///
/// # Arguments
/// * `fqn` - Fully qualified name from Magellan
/// * `language` - Programming language string
///
/// # Returns
/// SCIP-encoded symbol string
fn magellan_symbol_to_scip(fqn: &str, language: &str) -> String {
    // Split FQN based on language-specific separator
    let parts: Vec<&str> = match language {
        "rust" | "cpp" => fqn.split("::").collect(),
        "python" | "java" | "javascript" | "typescript" => fqn.split('.').collect(),
        _ => vec![fqn],
    };

    // Build SCIP symbol string
    // Format: magellan lang/descriptor1/descriptor2/symbol.
    let symbol = parts.last().unwrap_or(&"");

    // Build descriptor path (all parts except the last symbol name)
    let descriptors: Vec<String> = parts
        .iter()
        .take(parts.len().saturating_sub(1))
        .map(|s| s.to_string())
        .collect();

    // Construct full symbol string
    if descriptors.is_empty() {
        format!("magellan {}/{}.", language, symbol)
    } else {
        format!("magellan {}/{}/{}.", language, descriptors.join("/"), symbol)
    }
}

/// Map Magellan symbol kind to SCIP symbol kind
fn map_symbol_kind(kind: &str) -> Kind {
    match kind {
        "Function" => Kind::Function,
        "Method" => Kind::Method,
        "Struct" => Kind::Class,
        "Enum" => Kind::Enum,
        "Module" => Kind::Namespace,
        "Class" => Kind::Class,
        "Interface" => Kind::Interface,
        "TypeAlias" => Kind::TypeAlias,
        "Union" => Kind::Union,
        "Namespace" => Kind::Namespace,
        _ => Kind::UnspecifiedKind,
    }
}

/// Export graph to SCIP format
///
/// Builds a SCIP index containing:
/// - Metadata with tool info and project root
/// - Documents per file with occurrences (definitions and references)
/// - Proper SCIP symbol encoding
///
/// # Arguments
/// * `graph` - CodeGraph to export
/// * `config` - SCIP export configuration
///
/// # Returns
/// SCIP protobuf bytes
pub fn export_scip(graph: &CodeGraph, config: &ScipExportConfig) -> Result<Vec<u8>> {
    use sqlitegraph::{BackendDirection, NeighborQuery};

    let mut index = Index::new();

    // Build metadata
    let mut metadata = Metadata::new();

    // Create tool info
    let mut tool_info = scip::types::ToolInfo::new();
    tool_info.name = "magellan".to_string();
    tool_info.version = config
        .version
        .clone()
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    // Set metadata fields
    metadata.tool_info = protobuf::MessageField::some(tool_info);
    metadata.project_root = config.project_root.clone();

    // Set protocol version (UnspecifiedProtocolVersion is the only option in scip 0.6.1)
    metadata.version = EnumOrUnknown::new(scip::types::ProtocolVersion::UnspecifiedProtocolVersion);

    // Set metadata on index
    index.metadata = protobuf::MessageField::some(metadata);

    // Collect all entities by file
    let mut file_to_symbols: HashMap<String, Vec<(i64, SymbolNode)>> = HashMap::new();
    let mut file_to_references: HashMap<String, Vec<Occurrence>> = HashMap::new();

    // Get all entity IDs
    let entity_ids = graph.files.backend.entity_ids()?;

    // First pass: collect symbols and references by file
    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(entity_id)?;

        match entity.kind.as_str() {
            "Symbol" => {
                if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(entity.data.clone()) {
                    // Find the file path from DEFINES edge
                    let file_path = if let Some(file_id) = graph
                        .files
                        .backend
                        .neighbors(
                            entity_id,
                            NeighborQuery {
                                direction: BackendDirection::Incoming,
                                edge_type: Some("DEFINES".to_string()),
                            },
                        )?
                        .first()
                    {
                        let file_entity = graph.files.backend.get_node(*file_id)?;
                        if let Ok(file_node) = serde_json::from_value::<super::FileNode>(file_entity.data) {
                            file_node.path
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };

                    file_to_symbols
                        .entry(file_path)
                        .or_insert_with(Vec::new)
                        .push((entity_id, symbol_node));
                }
            }
            "Reference" => {
                // Extract file from reference node
                if let Ok(ref_node) = serde_json::from_value::<super::ReferenceNode>(entity.data.clone()) {
                    // For references, we'll create occurrences pointing to symbols
                    // The symbol name is stored in the entity name as "ref to {symbol}"
                    let symbol_name = entity
                        .name
                        .strip_prefix("ref to ")
                        .unwrap_or("")
                        .to_string();

                    // Create SCIP occurrence for reference
                    let mut occurrence = Occurrence::new();
                    occurrence.range = vec![
                        ref_node.start_line as i32,
                        ref_node.start_col as i32,
                        ref_node.end_line as i32,
                        ref_node.end_col as i32,
                    ];

                    // For references, we use a placeholder symbol that will be resolved
                    // to the actual definition symbol when we build the document
                    occurrence.symbol = format!("ref_{}", symbol_name);

                    // Set symbol roles: ReadAccess = 8 (used for references)
                    occurrence.symbol_roles = SymbolRole::ReadAccess as i32;

                    file_to_references
                        .entry(ref_node.file.clone())
                        .or_insert_with(Vec::new)
                        .push(occurrence);
                }
            }
            _ => {
                // Ignore other entity types
            }
        }
    }

    // Build SCIP documents
    for (file_path, symbols) in file_to_symbols {
        // Detect language from file path
        let language = if let Some(lang) = detect_language(std::path::Path::new(&file_path)) {
            lang.as_str().to_string()
        } else {
            "unknown".to_string()
        };

        // Create document
        let mut document = Document::new();
        document.relative_path = file_path.clone();
        document.language = language.clone();

        // Set position encoding to UTF-8 code units (line/col based)
        document.position_encoding = EnumOrUnknown::new(PositionEncoding::UTF8CodeUnitOffsetFromLineStart);

        // Build symbol lookup for resolving references
        let mut symbol_fqn_map: HashMap<String, String> = HashMap::new();

        // Add symbol occurrences (definitions)
        for (_node_id, symbol) in &symbols {
            let mut occurrence = Occurrence::new();

            // Set range [line_start, col_start, line_end, col_end]
            occurrence.range = vec![
                symbol.start_line as i32,
                symbol.start_col as i32,
                symbol.end_line as i32,
                symbol.end_col as i32,
            ];

            // Build SCIP symbol from FQN
            let fqn = symbol.fqn.as_deref().unwrap_or("");
            let scip_symbol = if !fqn.is_empty() {
                magellan_symbol_to_scip(fqn, &language)
            } else {
                // Fallback to simple name
                let name = symbol.name.as_deref().unwrap_or("");
                format!("magellan {}/{}.", language, name)
            };

            occurrence.symbol = scip_symbol.clone();

            // Set symbol roles: Definition = 1
            occurrence.symbol_roles = SymbolRole::Definition as i32;

            // Store for reference resolution
            if let Some(ref fqn) = symbol.fqn {
                symbol_fqn_map.insert(fqn.clone(), scip_symbol.clone());
            } else if let Some(ref name) = symbol.name {
                symbol_fqn_map.insert(name.clone(), scip_symbol.clone());
            }

            document.occurrences.push(occurrence);

            // Add symbol information to document.symbols
            let mut sym_info = SymbolInformation::new();
            sym_info.kind = EnumOrUnknown::new(map_symbol_kind(&symbol.kind));

            if let Some(ref name) = symbol.name {
                sym_info.display_name = name.clone();
            }

            sym_info.symbol = scip_symbol;

            document.symbols.push(sym_info);
        }

        // Add reference occurrences
        if let Some(refs) = file_to_references.get(&file_path) {
            for mut occurrence in refs.clone() {
                let ref_symbol = occurrence.symbol.clone();

                // Try to resolve the reference to a definition symbol
                // Strip the "ref_" prefix we added earlier
                let symbol_name = ref_symbol.strip_prefix("ref_").unwrap_or(&ref_symbol);

                // Look for matching symbol by name in this file
                let resolved_symbol = symbols
                    .iter()
                    .find(|(_, s)| s.name.as_deref() == Some(symbol_name))
                    .and_then(|(_, s)| {
                        s.fqn.as_ref().map(|fqn| magellan_symbol_to_scip(fqn, &language))
                    })
                    .or_else(|| {
                        // Try FQN lookup
                        symbol_fqn_map.get(symbol_name).cloned()
                    });

                if let Some(symbol) = resolved_symbol {
                    occurrence.symbol = symbol;
                }

                document.occurrences.push(occurrence);
            }
        }

        index.documents.push(document);
    }

    // Serialize to protobuf bytes
    let bytes = index.write_to_bytes()?;
    Ok(bytes)
}
