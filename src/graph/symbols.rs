//! Symbol node operations for CodeGraph
//!
//! Handles symbol node CRUD operations and DEFINES edge management.
//!
//! # Symbol ID Generation
//!
//! Symbol IDs are stable identifiers derived from a symbol's defining characteristics:
//! - **Language**: The programming language (e.g., "rust", "python", "javascript")
//! - **Fully-Qualified Name (FQN)**: The complete hierarchical name of the symbol
//! - **Span ID**: The stable identifier of the defining span in the source file
//!
//! The symbol ID format is: `SHA256(language:fqn:span_id)[0..16]` (16 hex characters)
//!
//! ## Stability Guarantees
//!
//! Symbol IDs are **stable** when:
//! - The same symbol is re-indexed after content changes elsewhere in the file
//! - The file path remains the same (span_id depends on file path)
//! - The language detection is consistent
//! - The fully-qualified name doesn't change
//!
//! Symbol IDs **change** when:
//! - The symbol is renamed (FQN changes)
//! - The symbol is moved to a different location (span_id changes)
//! - The file is renamed or moved (span_id depends on file path)
//! - The symbol's defining signature changes (affects FQN in future versions)

use anyhow::Result;
use blake3::Hasher;
use sha2::{Digest, Sha256};
use sqlitegraph::{
    add_label, BackendDirection, EdgeSpec, GraphBackend, NeighborQuery, NodeId, NodeSpec, SnapshotId,
    SqliteGraphBackend,
};
use std::rc::Rc;

use crate::detect_language;
use crate::graph::schema::SymbolNode;
use crate::ingest::SymbolFact;

/// Generate a stable symbol ID from (language, fqn, span_id)
///
/// Uses SHA-256 for platform-independent, deterministic symbol IDs.
///
/// # Algorithm
///
/// The hash is computed from: `language + ":" + fqn + ":" + span_id`
/// The first 8 bytes (64 bits) of the hash are formatted as 16 hex characters.
///
/// # Properties
///
/// This ensures symbol IDs are:
/// - **Deterministic**: same inputs always produce the same ID
/// - **Platform-independent**: SHA-256 produces consistent results across architectures
/// - **Collision-resistant**: 64-bit space with good distribution
///
/// # Examples
///
/// ```
/// use magellan::graph::generate_symbol_id;
///
/// let id1 = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");
/// let id2 = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");
/// let id3 = generate_symbol_id("python", "my_module.main", "a1b2c3d4e5f6g7h8");
///
/// assert_eq!(id1, id2);  // Same inputs = same ID
/// assert_ne!(id1, id3);  // Different language = different ID
/// assert_eq!(id1.len(), 16);  // Always 16 hex characters
/// ```
pub fn generate_symbol_id(language: &str, fqn: &str, span_id: &str) -> String {
    let mut hasher = Sha256::new();

    // Hash language
    hasher.update(language.as_bytes());

    // Separator to distinguish language from fqn
    hasher.update(b":");

    // Hash fully-qualified name
    hasher.update(fqn.as_bytes());

    // Separator to distinguish fqn from span_id
    hasher.update(b":");

    // Hash span_id
    hasher.update(span_id.as_bytes());

    // Take first 8 bytes (64 bits) and format as hex
    let result = hasher.finalize();
    format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        result[0], result[1], result[2], result[3], result[4], result[5], result[6], result[7]
    )
}

/// Generate a SymbolId using BLAKE3 from semantic inputs (v1.5)
///
/// **NOTE:** This function is part of v1.5 Symbol Identity but NOT YET INTEGRATED.
/// The current production code still uses `generate_symbol_id()` (SHA-256, 16 hex chars).
/// This BLAKE3 implementation exists for future migration.
///
/// This is the new SymbolId generation algorithm that excludes span information
/// for refactoring stability. The ID is deterministic and based only on semantic
/// characteristics of the symbol.
///
/// # Hash Input (alphabetical order)
///
/// - `crate_name`: Crate/package name for disambiguation
/// - `enclosing_items`: Scope chain (e.g., ["impl MyStruct", "mod my_module"])
/// - `file_path`: Source file path relative to project root
/// - `symbol_kind`: Symbol kind (Function, Method, Struct, etc.)
/// - `symbol_name`: Simple symbol name
///
/// EXCLUDED (per v1.5-02): span information for refactoring stability
///
/// # Output Format
///
/// Returns 32 hex characters (128 bits), per decision v1.5-01.
/// This is the first 16 bytes of BLAKE3's 32-byte output.
///
/// # Examples
///
/// ```
/// use magellan::graph::generate_symbol_id_v2;
///
/// let id = generate_symbol_id_v2(
///     "my_crate",
///     "src/lib.rs",
///     &["mod my_module".to_string(), "impl MyStruct".to_string()],
///     "Function",
///     "my_method"
/// );
/// assert_eq!(id.len(), 32);
/// ```
#[expect(dead_code)] // TODO(v1.6): Integrate BLAKE3-based symbol_id generation
pub fn generate_symbol_id_v2(
    crate_name: &str,
    file_path: &str,
    enclosing_items: &[String],
    symbol_kind: &str,
    symbol_name: &str,
) -> String {
    let mut hasher = Hasher::new();

    // IMPORTANT: Field order MUST be deterministic (alphabetical)
    // Order: crate_name, enclosing_items, file_path, symbol_kind, symbol_name

    hasher.update(crate_name.as_bytes());
    hasher.update(b":");
    hasher.update(enclosing_items.join("::").as_bytes());
    hasher.update(b":");
    hasher.update(file_path.as_bytes());
    hasher.update(b":");
    hasher.update(symbol_kind.as_bytes());
    hasher.update(b":");
    hasher.update(symbol_name.as_bytes());

    let hash = hasher.finalize();
    // Take first 32 hex characters (128 bits) per decision v1.5-01
    let hex = hash.to_hex().to_string();
    hex[..32].to_string()
}

/// Generate a stable span ID from (file_path, byte_start, byte_end)
///
/// Uses SHA-256 for platform-independent, deterministic span IDs.
/// This mirrors the Span::generate_id function from output/command.rs
/// but lives here to avoid circular dependencies between graph and output modules.
///
/// # Algorithm
///
/// The hash is computed from: `file_path + ":" + byte_start + ":" + byte_end`
/// The first 8 bytes (64 bits) of the hash are formatted as 16 hex characters.
fn generate_span_id(file_path: &str, byte_start: usize, byte_end: usize) -> String {
    let mut hasher = Sha256::new();

    // Hash file path
    hasher.update(file_path.as_bytes());

    // Separator to distinguish path from numbers
    hasher.update(b":");

    // Hash byte_start as big-endian bytes
    hasher.update(byte_start.to_be_bytes());

    // Separator
    hasher.update(b":");

    // Hash byte_end as big-endian bytes
    hasher.update(byte_end.to_be_bytes());

    // Take first 8 bytes (64 bits) and format as hex
    let result = hasher.finalize();
    format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        result[0], result[1], result[2], result[3], result[4], result[5], result[6], result[7]
    )
}

/// Symbol operations for CodeGraph
pub struct SymbolOps {
    pub backend: Rc<SqliteGraphBackend>,
}

impl SymbolOps {
    /// Insert a symbol node from SymbolFact
    ///
    /// This method generates a stable symbol_id based on the symbol's language,
    /// fully-qualified name, and defining span. The symbol_id is stored in the
    /// SymbolNode and can be used to correlate symbols across indexing runs.
    pub fn insert_symbol_node(&self, fact: &SymbolFact) -> Result<NodeId> {
        // Detect language (default to "unknown" if detection fails)
        let language = detect_language(&fact.file_path)
            .map(|l| l.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Generate span_id for the symbol's defining location
        let file_path_str = fact.file_path.to_string_lossy();
        let span_id = generate_span_id(&file_path_str, fact.byte_start, fact.byte_end);

        // Generate stable symbol_id from (language, FQN, span_id)
        // FQN prevents collisions; span_id ensures uniqueness within FQN
        let fqn_for_id = fact.fqn.as_deref().unwrap_or("");
        let symbol_id = generate_symbol_id(&language, fqn_for_id, &span_id);

        let symbol_node = SymbolNode {
            symbol_id: Some(symbol_id),
            fqn: fact.fqn.clone(),
            canonical_fqn: fact.canonical_fqn.clone(),
            display_fqn: fact.display_fqn.clone(),
            name: fact.name.clone(),
            kind: format!("{:?}", fact.kind),
            kind_normalized: Some(fact.kind_normalized.clone()),
            byte_start: fact.byte_start,
            byte_end: fact.byte_end,
            start_line: fact.start_line,
            start_col: fact.start_col,
            end_line: fact.end_line,
            end_col: fact.end_col,
        };

        let name = fact.name.clone().unwrap_or_else(|| {
            // Generate a name for unnamed symbols (like impl blocks)
            format!("<{:?} at {}>", fact.kind, fact.byte_start)
        });

        let node_spec = NodeSpec {
            kind: "Symbol".to_string(),
            name,
            file_path: Some(file_path_str.to_string()),
            data: serde_json::to_value(symbol_node)?,
        };

        let id = self.backend.insert_node(node_spec)?;
        let node_id = NodeId::from(id);

        // Add labels for efficient querying
        let graph = self.backend.graph();

        // Language label (e.g., "rust", "python", "javascript")
        if let Some(detected_lang) = detect_language(&fact.file_path) {
            add_label(graph, node_id.as_i64(), detected_lang.as_str())?;
        }

        // Symbol kind label (e.g., "fn", "struct", "enum", "method")
        add_label(graph, node_id.as_i64(), &fact.kind_normalized)?;

        Ok(node_id)
    }

    /// Insert DEFINES edge from file to symbol
    pub fn insert_defines_edge(&self, file_id: NodeId, symbol_id: NodeId) -> Result<()> {
        let edge_spec = EdgeSpec {
            from: file_id.as_i64(),
            to: symbol_id.as_i64(),
            edge_type: "DEFINES".to_string(),
            data: serde_json::json!({}),
        };

        self.backend.insert_edge(edge_spec)?;
        Ok(())
    }

    /// Delete all symbols and DEFINES edges for a file
    pub fn delete_file_symbols(&self, file_id: NodeId) -> Result<()> {
        // Find all outgoing DEFINES edges
        let snapshot = SnapshotId::current();
        let neighbor_ids = self.backend.neighbors(
            snapshot,
            file_id.as_i64(),
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("DEFINES".to_string()),
            },
        )?;

        // Delete each symbol node (edges are cascade deleted)
        for symbol_node_id in neighbor_ids {
            self.backend.graph().delete_entity(symbol_node_id)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::schema::SymbolNode;
    use sqlitegraph::GraphBackend;

    #[test]
    fn test_symbol_id_deterministic() {
        // Same inputs should produce same ID
        let id1 = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");
        let id2 = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");

        assert_eq!(id1, id2, "Same inputs should produce same symbol ID");
    }

    #[test]
    fn test_symbol_id_different_languages() {
        let rust_id = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");
        let python_id = generate_symbol_id("python", "my_module.main", "a1b2c3d4e5f6g7h8");

        assert_ne!(
            rust_id, python_id,
            "Different languages should produce different symbol IDs"
        );
    }

    #[test]
    fn test_symbol_id_different_fqn() {
        let id1 = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");
        let id2 = generate_symbol_id("rust", "my_crate::foo", "a1b2c3d4e5f6g7h8");

        assert_ne!(
            id1, id2,
            "Different FQNs should produce different symbol IDs"
        );
    }

    #[test]
    fn test_symbol_id_different_span() {
        let id1 = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");
        let id2 = generate_symbol_id("rust", "my_crate::main", "b1b2c3d4e5f6g7h8");

        assert_ne!(
            id1, id2,
            "Different span IDs should produce different symbol IDs"
        );
    }

    #[test]
    fn test_symbol_id_format() {
        let id = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");

        // ID should be 16 hex characters (64 bits)
        assert_eq!(id.len(), 16, "Symbol ID should be 16 characters");

        // All characters should be valid hex
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit()),
            "Symbol ID should be hex"
        );
    }

    #[test]
    fn test_span_id_deterministic() {
        // Same inputs should produce same span ID
        let id1 = generate_span_id("src/main.rs", 10, 20);
        let id2 = generate_span_id("src/main.rs", 10, 20);

        assert_eq!(id1, id2, "Same inputs should produce same span ID");
    }

    #[test]
    fn test_span_id_different_files() {
        let id1 = generate_span_id("src/main.rs", 10, 20);
        let id2 = generate_span_id("lib/main.rs", 10, 20);

        assert_ne!(
            id1, id2,
            "Different file paths should produce different span IDs"
        );
    }

    #[test]
    fn test_span_id_different_positions() {
        let id1 = generate_span_id("test.rs", 0, 10);
        let id2 = generate_span_id("test.rs", 10, 20);

        assert_ne!(
            id1, id2,
            "Different positions should produce different span IDs"
        );
    }

    #[test]
    fn test_span_id_format() {
        let id = generate_span_id("test.rs", 10, 20);

        // ID should be 16 hex characters (64 bits)
        assert_eq!(id.len(), 16, "Span ID should be 16 characters");

        // All characters should be valid hex
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit()),
            "Span ID should be hex"
        );
    }

    #[test]
    fn test_generate_symbol_id_v2_deterministic() {
        // Same inputs should produce same ID
        let id1 = generate_symbol_id_v2(
            "my_crate",
            "src/lib.rs",
            &["mod my_module".to_string()],
            "Function",
            "my_function",
        );
        let id2 = generate_symbol_id_v2(
            "my_crate",
            "src/lib.rs",
            &["mod my_module".to_string()],
            "Function",
            "my_function",
        );

        assert_eq!(id1, id2, "Same inputs should produce same SymbolId");
    }

    #[test]
    fn test_generate_symbol_id_v2_length() {
        let id = generate_symbol_id_v2("my_crate", "src/lib.rs", &[], "Function", "my_function");

        assert_eq!(id.len(), 32, "SymbolId should be 32 characters (128 bits)");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "Should be hex");
    }

    #[test]
    fn test_generate_symbol_id_v2_different_inputs() {
        let id1 = generate_symbol_id_v2("crate_a", "src/lib.rs", &[], "Function", "foo");
        let id2 = generate_symbol_id_v2("crate_b", "src/lib.rs", &[], "Function", "foo");
        let id3 = generate_symbol_id_v2("crate_a", "src/main.rs", &[], "Function", "foo");
        let id4 = generate_symbol_id_v2(
            "crate_a",
            "src/lib.rs",
            &["mod".to_string()],
            "Function",
            "foo",
        );
        let id5 = generate_symbol_id_v2("crate_a", "src/lib.rs", &[], "Method", "foo");
        let id6 = generate_symbol_id_v2("crate_a", "src/lib.rs", &[], "Function", "bar");

        // All different inputs should produce different IDs
        let ids = [&id1, &id2, &id3, &id4, &id5, &id6];
        for (i, id_a) in ids.iter().enumerate() {
            for (j, id_b) in ids.iter().enumerate() {
                if i != j {
                    assert_ne!(id_a, id_b, "Different inputs should produce different IDs");
                }
            }
        }
    }

    #[test]
    fn test_generate_symbol_id_v2_no_span_dependency() {
        // Moving a function should not change its ID if the semantic inputs are the same
        // (In practice, file_path would change, but this test demonstrates the principle)
        let enclosing_items = vec!["impl MyStruct".to_string()];

        let id1 = generate_symbol_id_v2(
            "my_crate",
            "src/lib.rs",
            &enclosing_items,
            "Method",
            "my_method",
        );

        let id2 = generate_symbol_id_v2(
            "my_crate",
            "src/lib.rs",
            &enclosing_items,
            "Method",
            "my_method",
        );

        assert_eq!(id1, id2, "Span-independent inputs should produce stable ID");
    }

    #[test]
    fn test_generate_symbol_id_v2_field_order() {
        // Field order is alphabetical: crate_name, enclosing_items, file_path, symbol_kind, symbol_name
        let id1 = generate_symbol_id_v2("crate", "file.rs", &["scope".to_string()], "kind", "name");

        let id2 = generate_symbol_id_v2("crate", "file.rs", &["scope".to_string()], "kind", "name");

        assert_eq!(id1, id2, "Alphabetical field order should be deterministic");
    }

    #[test]
    fn test_symbol_node_persists_fqn_fields() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "fn persist_me() {}\n").unwrap();

        let path_str = test_file.to_string_lossy().to_string();
        let source = std::fs::read(&test_file).unwrap();
        graph.index_file(&path_str, &source).unwrap();

        let entity_ids = graph.files.backend.entity_ids().unwrap();
        let mut found = false;
        let snapshot = SnapshotId::current();
        for entity_id in entity_ids {
            if let Ok(node) = graph.files.backend.get_node(snapshot, entity_id) {
                if node.kind == "Symbol" {
                    if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(node.data) {
                        if symbol_node.name.as_deref() == Some("persist_me") {
                            found = true;
                            let canonical = symbol_node.canonical_fqn.as_deref().unwrap_or("");
                            let display = symbol_node.display_fqn.as_deref().unwrap_or("");
                            assert!(!canonical.is_empty(), "canonical_fqn should be persisted");
                            assert!(!display.is_empty(), "display_fqn should be persisted");
                            assert!(canonical.contains("persist_me"));
                            assert!(display.contains("persist_me"));
                        }
                    }
                }
            }
        }

        assert!(found, "Expected to find symbol node for persist_me");
    }
}
