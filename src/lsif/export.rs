//! LSIF export functionality
//!
//! Converts Magellan's code graph to LSIF format for cross-repository navigation.

use anyhow::{Context, Result};
use std::path::Path;
use std::fs::File;
use std::io::Write;

use crate::graph::CodeGraph;
use super::schema::{LsifGraph, Vertex, Edge, PackageData, SymbolKind, generate_lsif_id};

/// Export Magellan graph to LSIF format
///
/// # Arguments
/// * `graph` - Magellan code graph
/// * `output_path` - Path to write LSIF JSON file
/// * `package_name` - Name of the package (e.g., crate name)
/// * `package_version` - Package version
///
/// # Returns
/// Number of symbols exported
pub fn export_lsif(
    graph: &mut CodeGraph,
    output_path: &Path,
    package_name: &str,
    package_version: &str,
) -> Result<usize> {
    let mut lsif = LsifGraph::new();
    let mut counter = 0u32;
    let mut exported_symbols = 0usize;

    // Add package vertex
    let package_id = generate_lsif_id("p", &mut counter);
    let package = Vertex::Package {
        id: package_id.clone(),
        label: "package".to_string(),
        data: PackageData {
            name: package_name.to_string(),
            version: package_version.to_string(),
            manager: "cargo".to_string(),
        },
    };
    lsif.add_vertex(package);

    // Get all files from the graph
    let files = graph.all_file_nodes()?;
    
    // Track documents (files) we've seen
    let mut documents: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for (file_path, _file_node) in files {
        // Get symbols for this file
        let symbols = graph.symbols_in_file(&file_path)?;
        
        // Create document vertex if we haven't seen this file
        let doc_id = if let Some(id) = documents.get(&file_path) {
            id.clone()
        } else {
            let doc_id = generate_lsif_id("d", &mut counter);
            let doc = Vertex::Document {
                id: doc_id.clone(),
                label: "document".to_string(),
                uri: file_path.clone(),
                language_id: detect_language_id(&file_path),
            };
            lsif.add_vertex(doc);
            
            // Add contains edge from package to document
            lsif.add_edge(Edge::Contains {
                id: generate_lsif_id("e", &mut counter),
                label: "contains".to_string(),
                out_v: package_id.clone(),
                in_vs: vec![doc_id.clone()],
            });
            
            documents.insert(file_path.clone(), doc_id.clone());
            doc_id
        };

        for symbol in symbols {
            let symbol_id = generate_lsif_id("s", &mut counter);
            let symbol_kind = magellan_symbol_to_lsif_kind(&symbol.kind_normalized);
            let _symbol_name = symbol.name.as_deref().unwrap_or("unknown");

            let symbol_vertex = Vertex::Symbol {
                id: symbol_id.clone(),
                label: "symbol".to_string(),
                kind: symbol_kind,
            };
            lsif.add_vertex(symbol_vertex);

            // Create range for the symbol
            let range_id = generate_lsif_id("r", &mut counter);
            let range = [
                symbol.start_line as u32,
                symbol.start_col as u32,
                symbol.end_line as u32,
                symbol.end_col as u32,
            ];
            let range_vertex = Vertex::Range {
                id: range_id.clone(),
                label: "range".to_string(),
                range,
            };
            lsif.add_vertex(range_vertex);

            // Add textDocument edge (document -> range)
            lsif.add_edge(Edge::TextDocument {
                id: generate_lsif_id("e", &mut counter),
                label: "textDocument".to_string(),
                out_v: doc_id.clone(),
                in_v: range_id.clone(),
            });

            // Add item edge (symbol -> range)
            lsif.add_edge(Edge::Item {
                id: generate_lsif_id("e", &mut counter),
                label: "item".to_string(),
                out_v: symbol_id.clone(),
                in_vs: vec![range_id.clone()],
                document: doc_id.clone(),
            });

            exported_symbols += 1;
        }
    }

    // Write LSIF to file
    let mut file = File::create(output_path)
        .with_context(|| format!("Failed to create LSIF file: {:?}", output_path))?;

    // Write as JSONL (one JSON object per line) for streaming
    for vertex in &lsif.vertices {
        let json = serde_json::to_string(vertex)?;
        writeln!(file, "{}", json)?;
    }

    for edge in &lsif.edges {
        let json = serde_json::to_string(edge)?;
        writeln!(file, "{}", json)?;
    }

    Ok(exported_symbols)
}

/// Convert Magellan symbol kind to LSIF symbol kind
fn magellan_symbol_to_lsif_kind(kind: &str) -> SymbolKind {
    match kind {
        "fn" | "function" => SymbolKind::Function,
        "method" => SymbolKind::Method,
        "struct" | "class" => SymbolKind::Struct,
        "trait" | "interface" => SymbolKind::Interface,
        "enum" => SymbolKind::Enum,
        "mod" | "module" => SymbolKind::Module,
        "type_alias" => SymbolKind::TypeParameter,
        "union" => SymbolKind::Struct,
        "namespace" => SymbolKind::Namespace,
        _ => SymbolKind::Variable,
    }
}

/// Detect language ID from file extension
fn detect_language_id(file_path: &str) -> String {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "rs" => "rust".to_string(),
        "py" => "python".to_string(),
        "c" | "h" => "c".to_string(),
        "cpp" | "hpp" | "cc" | "cxx" => "cpp".to_string(),
        "java" => "java".to_string(),
        "js" | "mjs" => "javascript".to_string(),
        "ts" | "tsx" => "typescript".to_string(),
        _ => "plaintext".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_export_lsif_basic() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let output_path = temp_dir.path().join("test.lsif");

        let mut graph = CodeGraph::open(&db_path).unwrap();
        
        // Create a test file
        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "fn main() {}").unwrap();

        // Index the file
        let _ = graph.scan_directory(temp_dir.path(), None);

        // Export to LSIF
        let result = export_lsif(&mut graph, &output_path, "test-crate", "0.1.0");
        assert!(result.is_ok());
        assert!(result.unwrap() >= 1);

        // Verify output file exists and has content
        assert!(output_path.exists());
        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(!content.is_empty());
        assert!(content.contains("\"type\":\"package\""));
    }
}
