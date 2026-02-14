//! Import node operations for CodeGraph
//!
//! Handles import node CRUD operations and IMPORTS edge management.

use anyhow::Result;
use sqlitegraph::{
    BackendDirection, EdgeSpec, GraphBackend, NeighborQuery, NodeSpec, SnapshotId,
};
use std::path::PathBuf;
use std::sync::Arc;

use crate::graph::schema::ImportNode;
use crate::ingest::ImportFact;

/// Import operations for CodeGraph
pub struct ImportOps {
    pub backend: Arc<dyn GraphBackend>,
}

impl ImportOps {
    /// Delete all Import nodes that belong to a specific file path.
    ///
    /// Determinism: collects candidate entity IDs, sorts ascending, deletes in that order.
    pub fn delete_imports_in_file(&self, path: &str) -> Result<usize> {
        let entity_ids = self.backend.entity_ids()?;
        let snapshot = SnapshotId::current();

        let mut to_delete: Vec<i64> = Vec::new();
        for entity_id in entity_ids {
            let node = match self.backend.get_node(snapshot, entity_id) {
                Ok(n) => n,
                Err(_) => continue,
            };

            if node.kind != "Import" {
                continue;
            }

            let import_node: ImportNode = match serde_json::from_value(node.data) {
                Ok(value) => value,
                Err(_) => continue,
            };

            if import_node.file == path {
                to_delete.push(entity_id);
            }
        }

        to_delete.sort_unstable();

        for id in &to_delete {
            self.backend.delete_entity(*id)?;
        }

        Ok(to_delete.len())
    }

    /// Index imports for a file into the graph
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `file_id` - File node ID
    /// * `imports` - Vector of ImportFact to index
    /// * `module_resolver` - Optional ModuleResolver for path resolution
    ///
    /// # Returns
    /// Number of imports indexed
    pub fn index_imports(
        &self,
        path: &str,
        file_id: i64,
        imports: Vec<ImportFact>,
        module_resolver: Option<&crate::graph::module_resolver::ModuleResolver>,
    ) -> Result<usize> {
        for import_fact in &imports {
            // Resolve import path to file_id using ModuleResolver
            let resolved_file_id = if let Some(resolver) = module_resolver {
                resolver.resolve_path(path, &import_fact.import_path)
            } else {
                None
            };

            let import_node = ImportNode {
                file: path.to_string(),
                import_kind: import_fact.import_kind.normalized_key().to_string(),
                import_path: import_fact.import_path.clone(),
                imported_names: import_fact.imported_names.clone(),
                is_glob: import_fact.is_glob,
                byte_start: import_fact.byte_start as u64,
                byte_end: import_fact.byte_end as u64,
                start_line: import_fact.start_line as u64,
                start_col: import_fact.start_col as u64,
                end_line: import_fact.end_line as u64,
                end_col: import_fact.end_col as u64,
            };

            let node_spec = NodeSpec {
                kind: "Import".to_string(),
                name: format!(
                    "{} import from {}",
                    import_fact.import_kind.normalized_key(),
                    import_fact.file_path.display()
                ),
                file_path: Some(path.to_string()),
                // Include resolved file_id in metadata for Phase 61
                data: {
                    let mut data = serde_json::to_value(import_node)?;
                    if let Some(resolved_id) = resolved_file_id {
                        if let Some(obj) = data.as_object_mut() {
                            obj.insert("resolved_file_id".to_string(), serde_json::json!(resolved_id));
                        }
                    }
                    data
                },
            };

            let import_id = self.backend.insert_node(node_spec)?;

            // Create DEFINES edge from import to resolved file (if available)
            if let Some(target_file_id) = resolved_file_id {
                self.create_import_edge(import_id, target_file_id)?;
            }

            // Create IMPORTS edge from file to import
            let edge_spec = EdgeSpec {
                from: file_id,
                to: import_id,
                edge_type: "IMPORTS".to_string(),
                data: serde_json::json!({
                    "byte_start": import_fact.byte_start,
                    "byte_end": import_fact.byte_end,
                }),
            };

            self.backend.insert_edge(edge_spec)?;
        }

        Ok(imports.len())
    }

    /// Create DEFINES edge from import node to resolved file
    ///
    /// # Arguments
    /// * `import_id` - Node ID of the import
    /// * `target_id` - Node ID of the resolved file
    fn create_import_edge(&self, import_id: i64, target_id: i64) -> Result<()> {
        let edge_spec = EdgeSpec {
            from: import_id,
            to: target_id,
            edge_type: "DEFINES".to_string(),
            data: serde_json::json!({}),
        };

        self.backend.insert_edge(edge_spec)?;
        Ok(())
    }

    /// Query all imports for a specific file
    ///
    /// # Arguments
    /// * `file_id` - Node ID of the file
    ///
    /// # Returns
    /// Vector of ImportFact for all imports in the file
    pub fn get_imports_for_file(&self, file_id: i64) -> Result<Vec<ImportFact>> {
        let snapshot = SnapshotId::current();

        // Query incoming IMPORTS edges from the file
        let neighbor_ids = self.backend.neighbors(
            snapshot,
            file_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("IMPORTS".to_string()),
            },
        )?;

        let mut imports = Vec::new();
        for import_node_id in neighbor_ids {
            if let Ok(Some(import)) = self.import_fact_from_node(import_node_id) {
                imports.push(import);
            }
        }

        Ok(imports)
    }

    /// Convert an import node to ImportFact
    fn import_fact_from_node(&self, node_id: i64) -> Result<Option<ImportFact>> {
        let snapshot = SnapshotId::current();
        let node = self.backend.get_node(snapshot, node_id)?;

        let import_node: Option<ImportNode> = serde_json::from_value(node.data).ok();

        let import_node = match import_node {
            Some(n) => n,
            None => return Ok(None),
        };

        // Parse import_kind from normalized key
        let import_kind = ImportKind::from_str(&import_node.import_kind)
            .unwrap_or(ImportKind::PlainUse);

        Ok(Some(ImportFact {
            file_path: PathBuf::from(&import_node.file),
            import_kind,
            import_path: import_node.import_path,
            imported_names: import_node.imported_names,
            is_glob: import_node.is_glob,
            byte_start: import_node.byte_start as usize,
            byte_end: import_node.byte_end as usize,
            start_line: import_node.start_line as usize,
            start_col: import_node.start_col as usize,
            end_line: import_node.end_line as usize,
            end_col: import_node.end_col as usize,
        }))
    }
}

// Re-export ImportKind for use within this module
use crate::ingest::ImportKind;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_delete_imports_in_file() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        // Create a file node first
        let test_file = "test.rs";
        let source = b"use std::collections::HashMap;";
        graph.index_file(test_file, source).unwrap();

        // index_file already creates import nodes, so we should have at least 1
        // Get the file_id
        let file_id = graph.files.find_file_node(test_file).unwrap().unwrap();

        // Create some additional test imports
        let imports = vec![
            ImportFact {
                file_path: PathBuf::from(test_file),
                import_kind: ImportKind::PlainUse,
                import_path: vec!["std".to_string(), "collections".to_string()],
                imported_names: vec!["HashSet".to_string()], // Different import
                is_glob: false,
                byte_start: 100,
                byte_end: 200,
                start_line: 2,
                start_col: 0,
                end_line: 3,
                end_col: 0,
            },
        ];

        // Index additional imports (without module resolver for this test)
        let count = graph
            .imports
            .index_imports(test_file, file_id.as_i64(), imports, None)
            .unwrap();
        assert_eq!(count, 1);

        // Delete all imports for this file (should be 2 total now)
        let deleted = graph
            .imports
            .delete_imports_in_file(test_file)
            .unwrap();
        assert_eq!(deleted, 2); // 1 from index_file + 1 from manual index_imports
    }

    #[test]
    fn test_index_imports_creates_nodes() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        let test_file = "test.rs";
        let source = b"use crate::foo::bar;";
        graph.index_file(test_file, source).unwrap();

        // Get the file_id
        let file_id = graph.files.find_file_node(test_file).unwrap().unwrap();

        let imports = vec![
            ImportFact {
                file_path: PathBuf::from(test_file),
                import_kind: ImportKind::UseCrate,
                import_path: vec!["crate".to_string(), "foo".to_string()],
                imported_names: vec!["bar".to_string()],
                is_glob: false,
                byte_start: 0,
                byte_end: 50,
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 50,
            },
        ];

        let count = graph
            .imports
            .index_imports(test_file, file_id.as_i64(), imports, None)
            .unwrap();

        assert_eq!(count, 1);

        // Verify the import node was created
        let snapshot = SnapshotId::current();
        let entity_ids = graph.imports.backend.entity_ids().unwrap();
        let import_node = entity_ids
            .iter()
            .find(|&&id| {
                let node = graph
                    .imports
                    .backend
                    .get_node(snapshot, id)
                    .unwrap();
                node.kind == "Import"
            })
            .map(|&id| {
                graph
                    .imports
                    .backend
                    .get_node(snapshot, id)
                    .unwrap()
            });

        assert!(import_node.is_some(), "Import node should be created");
    }

    #[test]
    fn test_index_imports_with_module_resolver() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        // Create test files with relative paths
        let lib_file = "src/lib.rs";
        let foo_file = "src/foo.rs";

        // Create directories
        std::fs::create_dir_all(temp_dir.path().join("src")).unwrap();

        // Index lib.rs
        graph.index_file(lib_file, b"fn lib() {}").unwrap();

        // Index foo.rs
        graph.index_file(foo_file, b"fn foo() {}").unwrap();

        // Build module index for resolver
        graph.module_resolver.build_module_index().unwrap();

        // Get file_id for lib.rs
        let file_id = graph.files.find_file_node(lib_file).unwrap().unwrap();

        // Create import that references crate::foo
        let imports = vec![
            ImportFact {
                file_path: PathBuf::from(lib_file),
                import_kind: ImportKind::UseCrate,
                import_path: vec!["crate".to_string(), "foo".to_string()],
                imported_names: vec!["foo".to_string()],
                is_glob: false,
                byte_start: 0,
                byte_end: 50,
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 50,
            },
        ];

        // Index imports with module resolver
        let count = graph
            .imports
            .index_imports(lib_file, file_id.as_i64(), imports, Some(&graph.module_resolver))
            .unwrap();

        assert_eq!(count, 1);

        // Verify the import node was created with resolved_file_id
        let snapshot = SnapshotId::current();
        let entity_ids = graph.imports.backend.entity_ids().unwrap();
        let import_node_option = entity_ids
            .iter()
            .find(|&&id| {
                let node = graph
                    .imports
                    .backend
                    .get_node(snapshot, id)
                    .unwrap();
                node.kind == "Import"
            })
            .map(|&id| {
                graph
                    .imports
                    .backend
                    .get_node(snapshot, id)
                    .unwrap()
            });

        assert!(import_node_option.is_some(), "Import node should be created");

        // Check that resolved_file_id is in the metadata
        let import_node = import_node_option.unwrap();
        let resolved_id = import_node.data.get("resolved_file_id");
        assert!(resolved_id.is_some(), "Import should have resolved_file_id in metadata");
    }

    #[test]
    fn test_cross_file_import_edges() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        // Create test files with relative paths
        let lib_file = "src/lib.rs";
        let helper_file = "src/helper.rs";

        // Create directories
        std::fs::create_dir_all(temp_dir.path().join("src")).unwrap();

        // Index lib.rs
        graph.index_file(lib_file, b"fn lib() {}").unwrap();

        // Index helper.rs
        graph.index_file(helper_file, b"fn helper() {}").unwrap();

        // Build module index for resolver
        graph.module_resolver.build_module_index().unwrap();

        // Get file_id for lib.rs
        let lib_file_id = graph.files.find_file_node(lib_file).unwrap().unwrap();

        // Get file_id for helper.rs (target of import)
        let helper_file_id = graph.files.find_file_node(helper_file).unwrap().unwrap();

        // Create import that references crate::helper
        let imports = vec![
            ImportFact {
                file_path: PathBuf::from(lib_file),
                import_kind: ImportKind::UseCrate,
                import_path: vec!["crate".to_string(), "helper".to_string()],
                imported_names: vec!["helper".to_string()],
                is_glob: false,
                byte_start: 0,
                byte_end: 50,
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 50,
            },
        ];

        // Index imports with module resolver
        let count = graph
            .imports
            .index_imports(lib_file, lib_file_id.as_i64(), imports, Some(&graph.module_resolver))
            .unwrap();

        assert_eq!(count, 1);

        // Verify the import node was created with resolved_file_id
        let snapshot = SnapshotId::current();
        let entity_ids = graph.imports.backend.entity_ids().unwrap();
        let import_node_option = entity_ids
            .iter()
            .find(|&&id| {
                let node = graph
                    .imports
                    .backend
                    .get_node(snapshot, id)
                    .unwrap();
                node.kind == "Import"
            })
            .map(|&id| {
                graph
                    .imports
                    .backend
                    .get_node(snapshot, id)
                    .unwrap()
            });

        assert!(import_node_option.is_some(), "Import node should be created");

        // Get the import node and its ID
        let import_node = import_node_option.unwrap();
        let import_id = entity_ids
            .iter()
            .find(|&&id| {
                let node = graph
                    .imports
                    .backend
                    .get_node(snapshot, id)
                    .unwrap();
                node.kind == "Import"
            })
            .unwrap();

        // Check that resolved_file_id matches helper_file_id
        let resolved_id = import_node.data.get("resolved_file_id");
        assert!(resolved_id.is_some(), "Import should have resolved_file_id in metadata");

        let resolved_value = resolved_id.unwrap().as_i64();
        assert_eq!(
            resolved_value,
            Some(helper_file_id.as_i64()),
            "resolved_file_id should match helper.rs file ID"
        );

        // Verify DEFINES edge exists from import to helper.rs file node
        use sqlitegraph::BackendDirection;
        let outgoing_edges = graph
            .imports
            .backend
            .neighbors(
                snapshot,
                *import_id,
                sqlitegraph::NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: Some("DEFINES".to_string()),
                },
            )
            .unwrap();

        assert_eq!(
            outgoing_edges.len(),
            1,
            "Import should have exactly one DEFINES edge"
        );
        assert_eq!(
            outgoing_edges[0],
            helper_file_id.as_i64(),
            "DEFINES edge should point to helper.rs file"
        );
    }
}
