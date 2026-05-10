//! File node operations for CodeGraph
//!
//! Handles file node CRUD operations and in-memory file indexing.
//!
//! # Thread Safety
//!
//! **This module is NOT thread-safe.**
//!
//! `FileOps` is designed for single-threaded use only:
//! - All methods require `&mut self` (exclusive access)
//! - `file_index: HashMap` has no synchronization primitives
//! - No `Send` or `Sync` impls
//!
//! # Usage Pattern
//!
//! `FileOps` is accessed exclusively through `CodeGraph`, which
//! enforces single-threaded access. The parent `CodeGraph` instance
//! must not be shared across threads.
//!
//! For concurrent file operations, use external synchronization
//! (e.g., mutex wrapper around CodeGraph).

use anyhow::Result;
use sqlitegraph::{GraphBackend, NodeId, NodeSpec, SnapshotId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use xxhash_rust::xxh3::Xxh3;

use crate::graph::schema::FileNode;
use crate::ingest::{SymbolFact, SymbolKind};

/// File operations for CodeGraph
pub struct FileOps {
    pub backend: Arc<dyn GraphBackend>,
    pub file_index: HashMap<String, NodeId>,
}

/// Normalize a path to absolute form for consistent indexing
///
/// This ensures paths stored in file_index match between:
/// - find_or_create_file_node() (during indexing)
/// - rebuild_file_index() (during database open)
/// - resolve_query_path() (during queries)
///
/// Note: Does NOT canonicalize (file doesn't need to exist). Just makes relative
/// paths absolute from current directory.
///
/// # Arguments
/// * `path` - The path to normalize (may be relative or absolute)
///
/// # Returns
/// Absolute path string
pub(crate) fn normalize_path_for_index(path: &str) -> String {
    let path_buf = PathBuf::from(path);
    if path_buf.is_absolute() {
        // Strip ./ and other non-semantic components from absolute paths
        let mut normalized = PathBuf::new();
        for component in path_buf.components() {
            match component {
                std::path::Component::CurDir => {} // skip .
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                other => normalized.push(other),
            }
        }
        return normalized.to_string_lossy().to_string();
    }

    // Relative path: make absolute from current directory (don't canonicalize - file may not exist)
    if let Ok(cwd) = std::env::current_dir() {
        let joined = cwd.join(&path_buf);
        // Normalize the joined path to remove ./ segments
        let mut normalized = PathBuf::new();
        for component in joined.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                other => normalized.push(other),
            }
        }
        return normalized.to_string_lossy().to_string();
    }

    // Fallback: return as-is
    path.to_string()
}

impl FileOps {
    /// Get current Unix timestamp in seconds
    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    /// Get filesystem modification time for a file path
    ///
    /// Returns 0 if file doesn't exist or mtime cannot be read
    fn get_file_mtime(path: &str) -> i64 {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(std::io::Error::other))
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Find file node by path, checking in-memory index
    ///
    /// Note: file_index is populated when CodeGraph opens, so this
    /// should find all existing File nodes. Returns None if not found.
    pub fn find_file_node(&mut self, path: &str) -> Result<Option<NodeId>> {
        // Normalize path to match how files are stored after index_file
        let normalized_path = normalize_path_for_index(path);
        Ok(self.file_index.get(&normalized_path).copied())
    }

    /// Find ALL file nodes matching a path by scanning the database.
    ///
    /// Unlike `find_file_node` which uses the in-memory HashMap (only holds one entry
    /// per path), this scans all entities and returns every File node whose path
    /// matches. Use this when cleaning up duplicates.
    pub fn find_all_file_nodes(&self, path: &str) -> Result<Vec<(NodeId, FileNode)>> {
        let normalized_path = normalize_path_for_index(path);
        let mut results = Vec::new();
        let ids = self.backend.entity_ids()?;
        let snapshot = SnapshotId::current();
        for id in ids {
            let node = match self.backend.get_node(snapshot, id) {
                Ok(n) => n,
                Err(_) => continue,
            };
            if node.kind == "File" {
                if let Ok(file_node) = serde_json::from_value::<FileNode>(node.data) {
                    let stored_path = normalize_path_for_index(&file_node.path);
                    if stored_path == normalized_path {
                        results.push((NodeId::from(id), file_node));
                    }
                }
            }
        }
        Ok(results)
    }

    /// Find existing file node or create new one.
    ///
    /// If multiple file nodes exist with the same path (duplicates from earlier
    /// indexing bugs), all are deleted before creating the new one.
    pub fn find_or_create_file_node(&mut self, path: &str, hash: &str) -> Result<NodeId> {
        let now = Self::now();
        let mtime = Self::get_file_mtime(path);

        // Normalize path to absolute canonical form for consistent indexing
        let normalized_path = normalize_path_for_index(path);

        // Find ALL file nodes with this path (not just the one in file_index)
        let all_existing = self.find_all_file_nodes(&normalized_path)?;

        if !all_existing.is_empty() {
            // If duplicates exist, delete all of them and their edges before creating fresh
            if all_existing.len() > 1 {
                for (old_id, _) in &all_existing {
                    let _ = self.backend.delete_entity(old_id.as_i64());
                }
                self.file_index.remove(&normalized_path);
            }

            // Use the first (or only) existing node's metadata as baseline
            let id = all_existing[0].0;
            let snapshot = SnapshotId::current();
            let node = self.backend.get_node(snapshot, id.as_i64())?;

            // Parse existing FileNode, update hash and timestamps, serialize back
            let mut file_node: FileNode =
                serde_json::from_value(node.data.clone()).unwrap_or_else(|_| FileNode {
                    path: path.to_string(),
                    hash: hash.to_string(),
                    last_indexed_at: now,
                    last_modified: mtime,
                });
            file_node.hash = hash.to_string();
            file_node.last_indexed_at = now;
            file_node.last_modified = mtime;

            let updated_data = serde_json::to_value(file_node)?;

            // Create new NodeSpec with updated data
            let node_spec = NodeSpec {
                kind: "File".to_string(),
                name: normalized_path.to_string(),
                file_path: Some(normalized_path.to_string()),
                data: updated_data,
            };

            // Delete old node and insert new one (sqlitegraph doesn't support update)
            self.backend.delete_entity(id.as_i64())?;
            let new_id = self.backend.insert_node(node_spec)?;
            let new_node_id = NodeId::from(new_id);

            // Update index with normalized path
            self.file_index
                .insert(normalized_path.to_string(), new_node_id);

            Ok(new_node_id)
        } else {
            // Create new file node with timestamps
            let file_node = FileNode {
                path: normalized_path.to_string(),
                hash: hash.to_string(),
                last_indexed_at: now,
                last_modified: mtime,
            };

            let node_spec = NodeSpec {
                kind: "File".to_string(),
                name: normalized_path.to_string(),
                file_path: Some(normalized_path.to_string()),
                data: serde_json::to_value(file_node)?,
            };

            let id = self.backend.insert_node(node_spec)?;
            let node_id = NodeId::from(id);

            // Update index with normalized path
            self.file_index.insert(normalized_path.to_string(), node_id);

            Ok(node_id)
        }
    }

    /// Rebuild in-memory file index by scanning all nodes
    pub fn rebuild_file_index(&mut self) -> Result<()> {
        self.file_index.clear();

        // Get all entity IDs from the backend
        let ids = self.backend.entity_ids()?;
        let snapshot = SnapshotId::current();

        for id in ids {
            let node = match self.backend.get_node(snapshot, id) {
                Ok(n) => n,
                Err(_) => continue,
            };

            if node.kind == "File" {
                if let Ok(file_node) = serde_json::from_value::<FileNode>(node.data) {
                    // Normalize path to match normalize_path_for_index() format
                    let normalized_path = normalize_path_for_index(&file_node.path);
                    self.file_index.insert(normalized_path, NodeId::from(id));
                }
            }
        }

        Ok(())
    }

    /// Compute xxHash3-128 of file contents
    pub fn compute_hash(&self, source: &[u8]) -> String {
        let mut hasher = Xxh3::new();
        hasher.update(source);
        format!("{:032x}", hasher.digest())
    }

    /// Convert a symbol node to SymbolFact
    pub fn symbol_fact_from_node(
        &self,
        node_id: i64,
        file_path: std::path::PathBuf,
    ) -> Result<Option<SymbolFact>> {
        let snapshot = SnapshotId::current();
        let node = self.backend.get_node(snapshot, node_id)?;

        let symbol_node: Option<crate::graph::schema::SymbolNode> =
            serde_json::from_value(node.data).ok();

        let symbol_node = match symbol_node {
            Some(n) => n,
            None => return Ok(None),
        };

        let kind = match symbol_node.kind.as_str() {
            "Function" => SymbolKind::Function,
            "Method" => SymbolKind::Method,
            "Class" => SymbolKind::Class,
            "Interface" => SymbolKind::Interface,
            "Enum" => SymbolKind::Enum,
            "Module" => SymbolKind::Module,
            "Union" => SymbolKind::Union,
            "Namespace" => SymbolKind::Namespace,
            "TypeAlias" => SymbolKind::TypeAlias,
            "Unknown" => SymbolKind::Unknown,
            _ => SymbolKind::Unknown,
        };

        let normalized_kind = match symbol_node.kind_normalized.clone() {
            Some(value) => value,
            None => kind.normalized_key().to_string(),
        };

        Ok(Some(SymbolFact {
            file_path,
            kind,
            kind_normalized: normalized_kind,
            name: symbol_node.name.clone(),
            fqn: symbol_node.fqn,
            canonical_fqn: None,
            display_fqn: None,
            byte_start: symbol_node.byte_start,
            byte_end: symbol_node.byte_end,
            start_line: symbol_node.start_line,
            start_col: symbol_node.start_col,
            end_line: symbol_node.end_line,
            end_col: symbol_node.end_col,
        }))
    }

    /// Get the FileNode for a given file path
    ///
    /// # Arguments
    /// * `path` - File path to query
    ///
    /// # Returns
    /// Option<FileNode> with file metadata including timestamps, or None if not found
    pub fn get_file_node(&mut self, path: &str) -> Result<Option<FileNode>> {
        let node_id = match self.find_file_node(path)? {
            Some(id) => id,
            None => return Ok(None),
        };

        let snapshot = SnapshotId::current();
        let entity = self.backend.get_node(snapshot, node_id.as_i64())?;
        let file_node: FileNode = serde_json::from_value(entity.data)?;
        Ok(Some(file_node))
    }

    /// Get all FileNodes from the database
    ///
    /// # Returns
    /// HashMap of file path -> FileNode for all files in the database
    pub fn all_file_nodes(&mut self) -> Result<std::collections::HashMap<String, FileNode>> {
        self.all_file_nodes_readonly()
    }

    /// Get all FileNodes from the database (read-only, doesn't rebuild index)
    ///
    /// # Returns
    /// HashMap of file path -> FileNode for all files in the database
    pub fn all_file_nodes_readonly(&self) -> Result<std::collections::HashMap<String, FileNode>> {
        use std::collections::HashMap;
        let mut result = HashMap::new();

        let entity_ids = self.backend.entity_ids()?;
        let snapshot = SnapshotId::current();
        for id in entity_ids {
            let entity = self.backend.get_node(snapshot, id)?;
            if entity.kind == "File" {
                if let Ok(file_node) = serde_json::from_value::<FileNode>(entity.data) {
                    result.insert(file_node.path.clone(), file_node);
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash_deterministic() {
        let graph = crate::CodeGraph::open(":memory:").unwrap();
        let ops = graph.files;

        let data = b"fn main() { println!(\"hello\"); }";
        let hash1 = ops.compute_hash(data);
        let hash2 = ops.compute_hash(data);

        assert_eq!(hash1, hash2, "Hash should be deterministic");
        assert_eq!(hash1.len(), 32, "xxHash3-128 produces 32 hex chars");
    }

    #[test]
    fn test_compute_hash_different_inputs() {
        let graph = crate::CodeGraph::open(":memory:").unwrap();
        let ops = graph.files;

        let hash1 = ops.compute_hash(b"fn a() {}");
        let hash2 = ops.compute_hash(b"fn b() {}");

        assert_ne!(
            hash1, hash2,
            "Different inputs should produce different hashes"
        );
    }
}
