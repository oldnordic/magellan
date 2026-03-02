//! LSIF schema definitions
//!
//! LSIF uses a graph structure with vertices and edges.
//! See: https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/

use serde::{Deserialize, Serialize};

/// LSIF graph containing all vertices and edges
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LsifGraph {
    /// LSIF version
    pub id: String,
    /// Protocol version
    pub protocol_version: String,
    /// All vertices in the graph
    pub vertices: Vec<Vertex>,
    /// All edges in the graph
    pub edges: Vec<Edge>,
}

impl LsifGraph {
    /// Create a new LSIF graph
    pub fn new() -> Self {
        Self {
            id: "lsif".to_string(),
            protocol_version: "0.6.0".to_string(),
            vertices: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Add a vertex to the graph
    pub fn add_vertex(&mut self, vertex: Vertex) {
        self.vertices.push(vertex);
    }

    /// Add an edge to the graph
    pub fn add_edge(&mut self, edge: Edge) {
        self.edges.push(edge);
    }
}

impl Default for LsifGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// LSIF vertex types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Vertex {
    /// Package information (e.g., crate, module)
    Package {
        id: String,
        label: String,
        data: PackageData,
    },
    /// Source file
    Document {
        id: String,
        label: String,
        uri: String,
        language_id: String,
    },
    /// Symbol (function, struct, etc.)
    Symbol {
        id: String,
        label: String,
        kind: SymbolKind,
    },
    /// Result set for a symbol
    ResultSet {
        id: String,
        label: String,
    },
    /// Range within a document
    Range {
        id: String,
        label: String,
        range: [u32; 4], // [start_line, start_col, end_line, end_col]
    },
    /// Moniker for cross-project resolution
    Moniker {
        id: String,
        label: String,
        data: MonikerData,
    },
}

/// Package data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageData {
    /// Package name (e.g., "serde")
    pub name: String,
    /// Package version (e.g., "1.0.195")
    pub version: String,
    /// Package manager (e.g., "cargo", "pip", "npm")
    pub manager: String,
}

/// Moniker data for cross-project symbol resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonikerData {
    /// Moniker kind (import, export, local)
    pub kind: MonikerKind,
    /// Moniker identifier
    pub identifier: String,
    /// Package containing the moniker
    pub package: PackageData,
}

/// Moniker kind
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MonikerKind {
    /// Symbol is imported from another package
    Import,
    /// Symbol is exported to other packages
    Export,
    /// Symbol is local to this package
    Local,
}

/// Symbol kinds
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolKind {
    Module,
    Namespace,
    Package,
    Class,
    Method,
    Property,
    Field,
    Constructor,
    Enum,
    Interface,
    Function,
    Variable,
    Constant,
    String,
    Number,
    Boolean,
    Array,
    Object,
    Key,
    Null,
    EnumMember,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

/// LSIF edge types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Edge {
    /// Contains relationship
    Contains {
        id: String,
        label: String,
        out_v: String,
        in_vs: Vec<String>,
    },
    /// Moniker relationship
    Moniker {
        id: String,
        label: String,
        out_v: String,
        in_v: String,
    },
    /// Next moniker relationship
    NextMoniker {
        id: String,
        label: String,
        out_v: String,
        in_v: String,
    },
    /// Item relationship (symbol to range)
    Item {
        id: String,
        label: String,
        out_v: String,
        in_vs: Vec<String>,
        document: String,
    },
    /// Text document relationship
    TextDocument {
        id: String,
        label: String,
        out_v: String,
        in_v: String,
    },
}

/// Create a unique ID for LSIF elements
pub fn generate_lsif_id(prefix: &str, counter: &mut u32) -> String {
    let id = format!("{}{}", prefix, counter);
    *counter += 1;
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsif_graph_creation() {
        let graph = LsifGraph::new();
        assert_eq!(graph.protocol_version, "0.6.0");
        assert!(graph.vertices.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn test_vertex_serialization() {
        let vertex = Vertex::Package {
            id: "p1".to_string(),
            label: "package".to_string(),
            data: PackageData {
                name: "serde".to_string(),
                version: "1.0.195".to_string(),
                manager: "cargo".to_string(),
            },
        };

        let json = serde_json::to_string(&vertex).unwrap();
        assert!(json.contains("\"type\":\"package\""));
        assert!(json.contains("\"name\":\"serde\""));
    }

    #[test]
    fn test_edge_serialization() {
        let edge = Edge::Contains {
            id: "e1".to_string(),
            label: "contains".to_string(),
            out_v: "p1".to_string(),
            in_vs: vec!["d1".to_string()],
        };

        let json = serde_json::to_string(&edge).unwrap();
        assert!(json.contains("\"type\":\"contains\""));
        assert!(json.contains("\"out_v\":\"p1\""));
    }
}
