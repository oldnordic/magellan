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
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_normalization::UnicodeNormalization;
use xxhash_rust::xxh3::Xxh3;

use crate::graph::schema::FileNode;
use crate::ingest::{SymbolFact, SymbolKind};

/// File operations for CodeGraph
pub struct FileOps {
    pub backend: Arc<dyn GraphBackend>,
    pub file_index: HashMap<String, NodeId>,
    /// Canonical, absolute, NFC-normalized index root recorded at ingest time
    /// (path-identity contract phase 1). `None` for pre-v21 databases until
    /// the next ingest stamps it.
    pub index_root: Option<String>,
}

/// NFC-normalize a path key.
///
/// Path keys are compared as raw strings; on some platforms (notably macOS,
/// which returns NFD from filesystem APIs) the same logical path can have
/// different byte representations. Normalizing to NFC at write and at
/// compare keeps keys stable across machines (graphify #2221).
pub(crate) fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// Normalize a path to absolute form for consistent indexing
///
/// This ensures paths stored in file_index match between:
/// - find_or_create_file_node() (during indexing)
/// - resolve_query_path() (during queries)
///
/// Note: Does NOT canonicalize (file doesn't need to exist). Just makes relative
/// paths absolute from current directory.
///
/// # Path-normalization contract
///
/// This is the QUERY/INGEST-side normalization: relative paths are resolved
/// against the process current working directory. It must NOT be applied to
/// paths read back from the database (see `normalize_stored_path`), because the
/// opener's cwd is not necessarily the cwd that was in effect at ingest time.
///
/// # Arguments
/// * `path` - The path to normalize (may be relative or absolute)
///
/// # Returns
/// Absolute path string (NFC-normalized)
pub(crate) fn normalize_path_for_index(path: &str) -> String {
    let path_buf = PathBuf::from(path);
    if path_buf.is_absolute() {
        return nfc(&normalize_segments(&path_buf).to_string_lossy());
    }

    // Relative path: make absolute from current directory (don't canonicalize - file may not exist)
    if let Ok(cwd) = std::env::current_dir() {
        let joined = cwd.join(&path_buf);
        return nfc(&normalize_segments(&joined).to_string_lossy());
    }

    // Fallback: return as-is
    nfc(path)
}

/// Normalize a STORED path for use as a file_index key.
///
/// Unlike `normalize_path_for_index`, this NEVER resolves relative paths
/// against the process cwd: a relative path stored in the database stays
/// relative (only `.` / `..` segments are folded). Resolving stored paths
/// against the opener's cwd would silently corrupt index keys whenever the
/// database is opened from a directory other than the ingest-time cwd.
///
/// The result is NFC-normalized so keys compare equal regardless of the
/// Unicode normalization form the path was stored in.
pub(crate) fn normalize_stored_path(path: &str) -> String {
    nfc(&normalize_segments(&PathBuf::from(path)).to_string_lossy())
}

/// Convert an absolute, segment-normalized path to root-relative POSIX
/// storage form (path-identity contract phase 2, recommendation R2).
///
/// The result is relative to `index_root`, uses `/` separators on every
/// platform (SCIP `Document.relative_path` rule 4), is canonical by
/// construction (no `.` / `..` / `//`, because both inputs are
/// segment-normalized before the strip), and is NFC-normalized.
///
/// Returns `None` when `abs_path` is not under `index_root` (out-of-root
/// files are stored absolute, graphify's rule) or the strip is degenerate
/// (the root itself).
pub(crate) fn root_relative_posix(abs_path: &str, index_root: &str) -> Option<String> {
    let abs = normalize_segments(&PathBuf::from(abs_path));
    let root = normalize_segments(&PathBuf::from(index_root));
    if !abs.is_absolute() || !root.is_absolute() {
        return None;
    }
    let rel = abs.strip_prefix(&root).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    // Join components with '/' explicitly so the stored form is POSIX even
    // on platforms whose native separator differs.
    let joined = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Some(nfc(&joined))
}

/// Strip `./` segments and fold `..` segments without touching the filesystem.
fn normalize_segments(path_buf: &std::path::Path) -> PathBuf {
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
    normalized
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
    ///
    /// # Path-normalization contract (lookup side)
    ///
    /// Deterministic resolution order:
    /// 1. EXACT: the query, segment-normalized and NFC-normalized as-given
    ///    (no anchor join), is looked up directly. Hits absolute queries
    ///    against absolute stored keys and relative queries against relative
    ///    stored keys.
    /// 2. INDEX_ROOT-ANCHORED: a relative query is joined onto the recorded
    ///    `index_root` (ingest-time anchor) and looked up. This resolves
    ///    repo-relative queries from ANY caller cwd whenever the database
    ///    was ingested by a v21+ indexer.
    /// 3. INDEX_ROOT-STRIPPED (phase 2): an absolute query has the recorded
    ///    `index_root` stripped, yielding the root-relative POSIX key under
    ///    which phase-2 databases store file paths. This is the dual-read
    ///    counterpart of root-relative storage: absolute queries keep
    ///    working against both legacy absolute rows (stage 1) and
    ///    root-relative rows (this stage).
    /// 4. CWD: a relative query is joined onto the process cwd and looked
    ///    up (historical behavior for callers whose cwd is the index root).
    /// 5. SUFFIX FALLBACK (last resort, deprecation-logged): path-segment
    ///    suffix matching between the query and the stored index keys, in
    ///    both directions:
    ///    - relative query `q`: a stored key matches if it equals `q`
    ///      (segment-normalized, no cwd join) or ends with `/q`. Every match
    ///      therefore shares the query's trailing segments, including its
    ///      basename (same-basename preference is structural).
    ///    - absolute query: a *relative* stored key `k` matches if the
    ///      normalized query ends with `/k`.
    ///
    ///    This stage is transitional: it exists for pre-v21 databases that
    ///    lack a recorded `index_root`. Every firing is logged so remaining
    ///    callers can be fixed; do not rely on it in new code.
    /// 6. OUTCOME: exactly one matching node -> returned. Zero -> None. More
    ///    than one -> None (ambiguous; never guess).
    pub fn find_file_node(&mut self, path: &str) -> Result<Option<NodeId>> {
        // Stage 1: exact lookup of the query as-given (normalized, no anchor).
        let as_given = normalize_stored_path(path);
        if let Some(id) = self.file_index.get(&as_given) {
            return Ok(Some(*id));
        }
        if !PathBuf::from(path).is_absolute() {
            // Stage 2: index_root-anchored lookup for relative queries.
            if let Some(anchored) = self.anchor_to_index_root(path) {
                if let Some(id) = self.file_index.get(&anchored) {
                    return Ok(Some(*id));
                }
            }
        } else {
            // Stage 3: index_root-stripped lookup for absolute queries
            // (phase-2 root-relative stored keys).
            if let Some(stripped) = self.strip_to_index_root(path) {
                if let Some(id) = self.file_index.get(&stripped) {
                    return Ok(Some(*id));
                }
            }
        }
        // Stage 4: cwd-joined lookup (historical query-side normalization).
        let normalized_path = normalize_path_for_index(path);
        if let Some(id) = self.file_index.get(&normalized_path) {
            return Ok(Some(*id));
        }
        // Stage 5: deterministic suffix fallback against stored index keys
        // (transitional; deprecation-logged when it fires).
        Ok(self.suffix_match_file_node(path, &normalized_path))
    }

    /// Join a relative query path onto the recorded index root.
    ///
    /// Returns `None` when no index root is recorded (pre-v21 databases)
    /// or the query is absolute.
    fn anchor_to_index_root(&self, raw_query: &str) -> Option<String> {
        let root = self.index_root.as_ref()?;
        if PathBuf::from(raw_query).is_absolute() {
            return None;
        }
        let joined = PathBuf::from(root).join(raw_query);
        Some(normalize_stored_path(&joined.to_string_lossy()))
    }

    /// Strip the recorded index root from an absolute query, yielding the
    /// root-relative POSIX candidate under which phase-2 databases store
    /// file paths.
    ///
    /// Returns `None` when no index root is recorded, the query is
    /// relative, or the query is outside the recorded root.
    fn strip_to_index_root(&self, raw_query: &str) -> Option<String> {
        let root = self.index_root.as_ref()?;
        if !PathBuf::from(raw_query).is_absolute() {
            return None;
        }
        root_relative_posix(raw_query, root)
    }

    /// Storage form for a freshly indexed file (path-identity contract
    /// phase 2, write side).
    ///
    /// `abs_normalized` must be absolute and segment-normalized (the output
    /// of `normalize_path_for_index`). When an index root is recorded and
    /// the file lives under it, the stored form is root-relative POSIX;
    /// out-of-root files and root-less (in-memory or unstamped) databases
    /// keep the legacy absolute form.
    fn storage_form(&self, abs_normalized: &str) -> String {
        match &self.index_root {
            Some(root) => root_relative_posix(abs_normalized, root)
                .unwrap_or_else(|| abs_normalized.to_string()),
            None => abs_normalized.to_string(),
        }
    }

    /// Re-anchor a STORED path to an absolute filesystem path (dual-read
    /// side of the phase-2 contract).
    ///
    /// Relative stored paths are joined onto the recorded index root;
    /// absolute stored paths pass through unchanged (legacy rows). When no
    /// index root is recorded, the stored path is returned as-is — callers
    /// that need a filesystem path must treat that as best-effort.
    pub fn absolute_fs_path(&self, stored_path: &str) -> String {
        if PathBuf::from(stored_path).is_absolute() {
            return stored_path.to_string();
        }
        match &self.index_root {
            Some(root) => PathBuf::from(root)
                .join(stored_path)
                .to_string_lossy()
                .to_string(),
            None => stored_path.to_string(),
        }
    }

    /// Remove every file_index key under which `query_path` could resolve:
    /// the exact stored form, the cwd-joined form, the index_root-anchored
    /// form, and the root-stripped form. Deletion paths must purge all
    /// candidates — removing only one normalization leaves stale entries
    /// that resurrect deleted files on the next lookup.
    pub fn remove_index_keys_for(&mut self, query_path: &str) {
        let mut candidates = vec![
            normalize_stored_path(query_path),
            normalize_path_for_index(query_path),
        ];
        if let Some(anchored) = self.anchor_to_index_root(query_path) {
            candidates.push(anchored);
        }
        if let Some(stripped) = self.strip_to_index_root(query_path) {
            candidates.push(stripped);
        }
        for key in candidates {
            self.file_index.remove(&key);
        }
    }

    /// Deterministic suffix fallback for `find_file_node` (stage 4, last
    /// resort). Transitional: exists for pre-v21 databases without a
    /// recorded `index_root`; every successful firing is deprecation-logged.
    fn suffix_match_file_node(&self, raw_query: &str, normalized_query: &str) -> Option<NodeId> {
        let raw_is_absolute = PathBuf::from(raw_query).is_absolute();
        let mut matches: Vec<i64> = Vec::new();
        for (key, id) in &self.file_index {
            let hit = if raw_is_absolute {
                // Absolute query vs relative stored key.
                !PathBuf::from(key).is_absolute()
                    && normalized_query.ends_with(&format!("/{}", key))
            } else {
                // Relative query vs stored keys (absolute or relative).
                let q = normalize_stored_path(raw_query);
                !q.is_empty() && (key == &q || key.ends_with(&format!("/{}", q)))
            };
            if hit {
                matches.push(id.as_i64());
            }
        }
        matches.sort_unstable();
        matches.dedup();
        if matches.len() == 1 {
            tracing::warn!(
                query = %raw_query,
                "DEPRECATED path resolution: suffix fallback fired. \
                 Re-ingest to record index_root, or pass anchored paths; \
                 suffix matching is transitional and will be retired."
            );
            Some(NodeId::from(matches[0]))
        } else {
            // Zero matches (genuine miss) or ambiguous (more than one stored
            // root contains the same relative suffix) — both are None.
            None
        }
    }

    /// Find ALL file nodes matching a path by scanning the database.
    ///
    /// Unlike `find_file_node` which uses the in-memory HashMap (only holds one entry
    /// per path), this scans all entities and returns every File node whose path
    /// matches. Use this when cleaning up duplicates.
    pub fn find_all_file_nodes(&self, path: &str) -> Result<Vec<(NodeId, FileNode)>> {
        let normalized_path = normalize_path_for_index(path);
        let raw_is_absolute = PathBuf::from(path).is_absolute();
        // Index_root-anchored form of a relative query: an exact match
        // candidate that does not depend on the opener's cwd.
        let anchored_path = if raw_is_absolute {
            None
        } else {
            self.anchor_to_index_root(path)
        };
        // Index_root-stripped form of an absolute query (phase 2): the
        // root-relative POSIX key under which new databases store paths.
        let stripped_path = if raw_is_absolute {
            self.strip_to_index_root(path)
        } else {
            None
        };
        let query_rel = if raw_is_absolute {
            None
        } else {
            Some(normalize_stored_path(path))
        };
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
                    // Stored paths are normalized WITHOUT resolving against the
                    // opener's cwd (see normalize_stored_path). A stored path
                    // matches the query on exact equality or on a path-segment
                    // suffix in either direction, so ingest-time dedup keeps
                    // working when stored paths and query paths are anchored
                    // differently (relative vs absolute).
                    let stored_path = normalize_stored_path(&file_node.path);
                    let anchor_hit = anchored_path
                        .as_ref()
                        .is_some_and(|anchored| stored_path == *anchored);
                    let strip_hit = stripped_path
                        .as_ref()
                        .is_some_and(|stripped| stored_path == *stripped);
                    let suffix_hit = match &query_rel {
                        Some(q) => {
                            !q.is_empty()
                                && (stored_path.ends_with(&format!("/{}", q))
                                    || normalized_path.ends_with(&format!("/{}", stored_path)))
                        }
                        None => {
                            !stored_path.is_empty()
                                && !PathBuf::from(&stored_path).is_absolute()
                                && normalized_path.ends_with(&format!("/{}", stored_path))
                        }
                    };
                    if stored_path == normalized_path || anchor_hit || strip_hit || suffix_hit {
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
    ///
    /// # Storage form (path-identity contract phase 2)
    ///
    /// The persisted path is the `storage_form` of the absolute normalized
    /// path: root-relative POSIX when an index root is recorded and the file
    /// lives under it, absolute otherwise (out-of-root files, in-memory or
    /// unstamped databases). Re-indexing a legacy absolute row lazily
    /// migrates its stored path to the current storage form — no forced
    /// reindex required.
    pub fn find_or_create_file_node(&mut self, path: &str, hash: &str) -> Result<NodeId> {
        let now = Self::now();
        let mtime = Self::get_file_mtime(path);

        // Normalize path to absolute canonical form for consistent indexing
        let normalized_path = normalize_path_for_index(path);
        let stored_path = self.storage_form(&normalized_path);

        // Find ALL file nodes with this path (not just the one in file_index).
        // find_all dual-reads: it matches legacy absolute rows and
        // root-relative rows alike, so re-indexing never duplicates.
        let all_existing = self.find_all_file_nodes(&normalized_path)?;

        if !all_existing.is_empty() {
            // If duplicates exist, delete all of them and their edges before creating fresh
            if all_existing.len() > 1 {
                for (old_id, old_node) in &all_existing {
                    let _ = self.backend.delete_entity(old_id.as_i64());
                    self.file_index
                        .remove(&normalize_stored_path(&old_node.path));
                }
                self.remove_index_keys_for(&normalized_path);
            } else {
                // Single existing row: drop its current index key (the row is
                // deleted below and re-inserted under the storage-form key).
                self.file_index
                    .remove(&normalize_stored_path(&all_existing[0].1.path));
            }

            // Use the first (or only) existing node's metadata as baseline
            let id = all_existing[0].0;
            let snapshot = SnapshotId::current();
            let node = self.backend.get_node(snapshot, id.as_i64())?;

            // Parse existing FileNode, update path (lazy migration to the
            // current storage form), hash and timestamps, serialize back
            let mut file_node: FileNode =
                serde_json::from_value(node.data.clone()).unwrap_or_else(|_| FileNode {
                    path: stored_path.clone(),
                    hash: hash.to_string(),
                    last_indexed_at: now,
                    last_modified: mtime,
                });
            file_node.path = stored_path.clone();
            file_node.hash = hash.to_string();
            file_node.last_indexed_at = now;
            file_node.last_modified = mtime;

            let updated_data = serde_json::to_value(file_node)?;

            // Create new NodeSpec with updated data
            let node_spec = NodeSpec {
                kind: "File".to_string(),
                name: stored_path.clone(),
                file_path: Some(stored_path.clone()),
                data: updated_data,
            };

            // Delete old node and insert new one (sqlitegraph doesn't support update)
            self.backend.delete_entity(id.as_i64())?;
            let new_id = self.backend.insert_node(node_spec)?;
            let new_node_id = NodeId::from(new_id);

            // Update index with the storage-form key
            self.file_index.insert(stored_path, new_node_id);

            Ok(new_node_id)
        } else {
            // Create new file node with timestamps
            let file_node = FileNode {
                path: stored_path.clone(),
                hash: hash.to_string(),
                last_indexed_at: now,
                last_modified: mtime,
            };

            let node_spec = NodeSpec {
                kind: "File".to_string(),
                name: stored_path.clone(),
                file_path: Some(stored_path.clone()),
                data: serde_json::to_value(file_node)?,
            };

            let id = self.backend.insert_node(node_spec)?;
            let node_id = NodeId::from(id);

            // Update index with the storage-form key
            self.file_index.insert(stored_path, node_id);

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
                    // Stored paths are indexed as-stored (segment-normalized
                    // only). Relative stored paths must NOT be resolved against
                    // the opener's cwd — that would bake the wrong anchor into
                    // every index key when the DB is opened from a directory
                    // other than the ingest-time cwd.
                    let normalized_path = normalize_stored_path(&file_node.path);
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

    // ------------------------------------------------------------------
    // Path-normalization contract tests.
    //
    // These tests exercise the lookup contract from a cwd that is NOT the
    // index root: the indexed files live under a fresh TempDir, so the
    // stage-1 cwd-joined form of a relative query can never accidentally
    // equal the stored absolute path, regardless of the directory the test
    // binary runs from (crate root, /var/tmp, $HOME, ...). A pass therefore
    // proves the stored-path suffix fallback, not cwd flattery.
    // ------------------------------------------------------------------

    use sqlitegraph::{NodeSpec, SnapshotId};

    /// Insert a File node with a repo-relative stored path directly into the
    /// backend (simulates DBs/fixtures that store relative paths), then
    /// rebuild the in-memory index.
    fn insert_relative_file_node(graph: &mut crate::CodeGraph, rel_path: &str) {
        let file_node = crate::graph::schema::FileNode {
            path: rel_path.to_string(),
            hash: "deadbeef".to_string(),
            last_indexed_at: 0,
            last_modified: 0,
        };
        let spec = NodeSpec {
            kind: "File".to_string(),
            name: rel_path.to_string(),
            file_path: Some(rel_path.to_string()),
            data: serde_json::to_value(&file_node).unwrap(),
        };
        graph.files.backend.insert_node(spec).unwrap();
        graph.files.rebuild_file_index().unwrap();
    }

    #[test]
    fn test_relative_query_resolves_against_absolute_stored_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let mut graph = crate::CodeGraph::open(&db).unwrap();

        // Index via absolute path (this is how the live llama-rs DB was built:
        // 100% absolute stored paths).
        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let abs = src_dir.join("alpha_widget.rs");
        std::fs::write(&abs, b"fn alpha_widget_fn() {}\n").unwrap();
        graph
            .index_file(abs.to_str().unwrap(), b"fn alpha_widget_fn() {}\n")
            .unwrap();

        // Relative query from a foreign cwd: stage-1 (cwd-join) cannot hit the
        // TempDir-anchored stored path; the suffix fallback must resolve it.
        let found = graph.files.find_file_node("src/alpha_widget.rs").unwrap();
        assert!(
            found.is_some(),
            "relative query must resolve against the absolute stored path via suffix fallback"
        );

        // Full public stack: symbol lookup by relative path.
        let sym = crate::graph::query::symbol_id_by_name(
            &mut graph,
            "src/alpha_widget.rs",
            "alpha_widget_fn",
        )
        .unwrap();
        assert!(
            sym.is_some(),
            "symbol_id_by_name with a repo-relative path must hit from any cwd"
        );
    }

    #[test]
    fn test_relative_query_ambiguous_suffix_returns_none() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let mut graph = crate::CodeGraph::open(&db).unwrap();

        // Two stored roots containing the same relative suffix.
        for root in ["alpha_root", "beta_root"] {
            let dir = temp.path().join(root).join("src");
            std::fs::create_dir_all(&dir).unwrap();
            let abs = dir.join("dup_widget.rs");
            std::fs::write(&abs, b"fn dup_widget_fn() {}\n").unwrap();
            graph
                .index_file(abs.to_str().unwrap(), b"fn dup_widget_fn() {}\n")
                .unwrap();
        }

        // Ambiguous suffix: deterministic None, never a guess.
        let found = graph.files.find_file_node("src/dup_widget.rs").unwrap();
        assert!(
            found.is_none(),
            "ambiguous suffix (two stored roots) must return None, not an arbitrary root"
        );

        // A longer, unique suffix still resolves.
        let found = graph
            .files
            .find_file_node("alpha_root/src/dup_widget.rs")
            .unwrap();
        assert!(found.is_some(), "unique longer suffix must resolve");
    }

    #[test]
    fn test_absolute_query_resolves_against_relative_stored_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let mut graph = crate::CodeGraph::open(&db).unwrap();

        insert_relative_file_node(&mut graph, "src/rel_widget.rs");

        // Absolute query whose tail is the stored relative path.
        let abs_query = temp.path().join("src").join("rel_widget.rs");
        let found = graph
            .files
            .find_file_node(abs_query.to_str().unwrap())
            .unwrap();
        assert!(
            found.is_some(),
            "absolute query must resolve against a relative stored path via reverse suffix match"
        );

        // Relative query equal to the stored relative path also resolves.
        let found = graph.files.find_file_node("src/rel_widget.rs").unwrap();
        assert!(found.is_some(), "exact relative stored path must resolve");
    }

    #[test]
    fn test_rebuild_file_index_keeps_relative_stored_paths_relative() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let mut graph = crate::CodeGraph::open(&db).unwrap();

        insert_relative_file_node(&mut graph, "src/rel_indexed.rs");

        assert!(
            graph.files.file_index.contains_key("src/rel_indexed.rs"),
            "relative stored path must be indexed as-stored, not resolved against opener cwd"
        );
        let cwd_joined = std::env::current_dir()
            .unwrap()
            .join("src/rel_indexed.rs")
            .to_string_lossy()
            .to_string();
        assert!(
            !graph.files.file_index.contains_key(&cwd_joined),
            "index keys must not be baked against the opener's cwd"
        );
    }

    // ------------------------------------------------------------------
    // Path-identity contract phase 1: index_root anchor + NFC keys.
    // ------------------------------------------------------------------

    #[test]
    fn test_nfc_normalization_of_path_keys() {
        // "é" as a single codepoint (NFC) vs "e" + combining acute (NFD).
        let nfc_path = "src/caf\u{00e9}.rs";
        let nfd_path = "src/cafe\u{0301}.rs";
        assert_ne!(nfc_path, nfd_path, "fixtures must differ byte-wise");
        assert_eq!(
            super::normalize_stored_path(nfd_path),
            super::normalize_stored_path(nfc_path),
            "NFD and NFC forms of the same path must produce the same key"
        );
        assert_eq!(
            super::normalize_path_for_index(nfd_path),
            super::normalize_path_for_index(nfc_path),
            "query-side normalization must also be NFC-stable"
        );
    }

    #[test]
    fn test_index_root_recorded_and_reloaded() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let root = temp.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        let expected =
            super::normalize_stored_path(&root.canonicalize().unwrap().to_string_lossy());

        {
            let mut graph = crate::CodeGraph::open(&db).unwrap();
            assert_eq!(graph.index_root(), None, "fresh DB has no index_root");
            graph.set_index_root(&root).unwrap();
            assert_eq!(graph.index_root(), Some(expected.as_str()));
        }

        // Reopen: the recorded root must survive in magellan_meta.
        let graph = crate::CodeGraph::open(&db).unwrap();
        assert_eq!(
            graph.index_root(),
            Some(expected.as_str()),
            "index_root must persist across CodeGraph::open"
        );
    }

    #[test]
    fn test_index_root_anchor_disambiguates_relative_query() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let mut graph = crate::CodeGraph::open(&db).unwrap();

        // Two roots with the same relative layout: the suffix fallback alone
        // can only return None (ambiguous). Anchoring to the recorded
        // index_root must deterministically pick the indexed root's node.
        let mut alpha_id = None;
        for root_name in ["alpha_root", "beta_root"] {
            let dir = temp.path().join(root_name).join("src");
            std::fs::create_dir_all(&dir).unwrap();
            let abs = dir.join("dup_widget.rs");
            std::fs::write(&abs, b"fn dup_widget_fn() {}\n").unwrap();
            let id = graph
                .files
                .find_or_create_file_node(abs.to_str().unwrap(), "hash")
                .unwrap();
            if root_name == "alpha_root" {
                alpha_id = Some(id);
            }
        }

        // Sanity: without an anchor the ambiguous suffix yields None.
        assert!(graph
            .files
            .find_file_node("src/dup_widget.rs")
            .unwrap()
            .is_none());

        graph
            .set_index_root(&temp.path().join("alpha_root"))
            .unwrap();
        let found = graph.files.find_file_node("src/dup_widget.rs").unwrap();
        assert_eq!(
            found, alpha_id,
            "index_root-anchored resolution must pick the anchored root's node"
        );
    }

    #[test]
    fn test_index_root_anchored_find_all_file_nodes() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let mut graph = crate::CodeGraph::open(&db).unwrap();

        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let abs = src_dir.join("all_widget.rs");
        std::fs::write(&abs, b"fn all_widget_fn() {}\n").unwrap();
        graph
            .index_file(abs.to_str().unwrap(), b"fn all_widget_fn() {}\n")
            .unwrap();

        graph.set_index_root(temp.path()).unwrap();
        let all = graph
            .files
            .find_all_file_nodes("src/all_widget.rs")
            .unwrap();
        assert_eq!(
            all.len(),
            1,
            "index_root-anchored find_all must hit the TempDir-stored absolute path from any cwd"
        );
    }

    // ------------------------------------------------------------------
    // Path-identity contract phase 2: root-relative POSIX storage +
    // dual-read of legacy absolute rows.
    // ------------------------------------------------------------------

    /// Canonical path of a TempDir, so set_index_root (which canonicalizes)
    /// and test-constructed absolute paths agree byte-for-byte.
    fn canonical_root(temp: &tempfile::TempDir) -> std::path::PathBuf {
        temp.path().canonicalize().unwrap()
    }

    /// Count graph entities of `kind` whose JSON `file` field equals
    /// `file_path` — ground truth for location-record assertions.
    fn count_location_entities(graph: &crate::CodeGraph, kind: &str, file_path: &str) -> usize {
        let snapshot = SnapshotId::current();
        graph
            .files
            .backend
            .entity_ids()
            .unwrap()
            .iter()
            .filter_map(|id| graph.files.backend.get_node(snapshot, *id).ok())
            .filter(|node| {
                node.kind == kind
                    && node
                        .data
                        .get("file")
                        .and_then(|v| v.as_str())
                        .map(|f| f == file_path)
                        .unwrap_or(false)
            })
            .count()
    }

    /// The phase-2 gate: index from the repo root, then query with a
    /// root-relative path from a foreign cwd (the test binary's cwd is never
    /// the TempDir root, so a pass proves cwd-independence).
    #[test]
    fn test_phase2_root_relative_storage_roundtrip_from_foreign_cwd() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let root = canonical_root(&temp).join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        let abs = root.join("src").join("roundtrip_widget.rs");
        std::fs::write(&abs, b"fn roundtrip_widget_fn() {}\n").unwrap();

        {
            let mut graph = crate::CodeGraph::open(&db).unwrap();
            graph.set_index_root(&root).unwrap();
            graph
                .index_file(abs.to_str().unwrap(), b"fn roundtrip_widget_fn() {}\n")
                .unwrap();

            // Write side: the stored path is root-relative POSIX.
            let node = graph
                .files
                .get_file_node("src/roundtrip_widget.rs")
                .unwrap()
                .expect("root-relative query must resolve immediately after indexing");
            assert_eq!(
                node.path, "src/roundtrip_widget.rs",
                "phase-2 databases must store file paths root-relative POSIX"
            );
            assert!(graph
                .files
                .file_index
                .contains_key("src/roundtrip_widget.rs"));
        }

        // Reopen (index_root reloads from magellan_meta) and query both ways.
        let mut graph = crate::CodeGraph::open(&db).unwrap();
        assert!(
            graph
                .files
                .find_file_node("src/roundtrip_widget.rs")
                .unwrap()
                .is_some(),
            "root-relative query must resolve from any cwd"
        );
        assert!(
            graph
                .files
                .find_file_node(abs.to_str().unwrap())
                .unwrap()
                .is_some(),
            "absolute query must resolve via the index_root-strip stage"
        );
        let sym = crate::graph::query::symbol_id_by_name(
            &mut graph,
            "src/roundtrip_widget.rs",
            "roundtrip_widget_fn",
        )
        .unwrap();
        assert!(
            sym.is_some(),
            "symbol lookup by root-relative path must hit from any cwd"
        );
        let all = graph
            .files
            .find_all_file_nodes(abs.to_str().unwrap())
            .unwrap();
        assert_eq!(
            all.len(),
            1,
            "absolute find_all must match the root-relative stored row exactly once"
        );
    }

    #[test]
    fn test_phase2_out_of_root_file_stored_absolute() {
        let temp = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let root = canonical_root(&temp);
        let abs = outside.path().canonicalize().unwrap().join("outsider.rs");
        std::fs::write(&abs, b"fn outsider_fn() {}\n").unwrap();

        let mut graph = crate::CodeGraph::open(&db).unwrap();
        graph.set_index_root(&root).unwrap();
        graph
            .index_file(abs.to_str().unwrap(), b"fn outsider_fn() {}\n")
            .unwrap();

        // Out-of-root files keep the absolute storage form (graphify's rule).
        let node = graph
            .files
            .get_file_node(abs.to_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            node.path,
            super::normalize_path_for_index(abs.to_str().unwrap()),
            "out-of-root file must be stored absolute"
        );
    }

    #[test]
    fn test_phase2_dual_read_legacy_absolute_rows() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let root = canonical_root(&temp);
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let abs = src_dir.join("legacy_widget.rs");
        std::fs::write(&abs, b"fn legacy_widget_fn() {}\n").unwrap();

        let mut graph = crate::CodeGraph::open(&db).unwrap();
        // Legacy ingest: no stamped root -> absolute stored path.
        graph
            .index_file(abs.to_str().unwrap(), b"fn legacy_widget_fn() {}\n")
            .unwrap();
        let stored = graph
            .files
            .get_file_node(abs.to_str().unwrap())
            .unwrap()
            .unwrap();
        assert!(
            std::path::Path::new(&stored.path).is_absolute(),
            "without an index root the legacy absolute storage form must be kept"
        );

        // The phase-1 stamp arrives later (watch pipeline / next ingest).
        graph.set_index_root(&root).unwrap();

        // Dual-read: a root-relative query resolves the legacy ABSOLUTE row
        // via the index_root-anchored stage.
        assert!(
            graph
                .files
                .find_file_node("src/legacy_widget.rs")
                .unwrap()
                .is_some(),
            "relative query must dual-read legacy absolute rows"
        );
        // Absolute query still hits exact.
        assert!(graph
            .files
            .find_file_node(abs.to_str().unwrap())
            .unwrap()
            .is_some());
        let all = graph
            .files
            .find_all_file_nodes("src/legacy_widget.rs")
            .unwrap();
        assert_eq!(
            all.len(),
            1,
            "find_all must not double-count the dual-read row"
        );
    }

    #[test]
    fn test_phase2_reindex_migrates_legacy_row_without_duplicate() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let root = canonical_root(&temp);
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let abs = src_dir.join("migrate_widget.rs");
        std::fs::write(&abs, b"fn migrate_widget_fn() {}\n").unwrap();

        let mut graph = crate::CodeGraph::open(&db).unwrap();
        // Legacy ingest (no root), then the stamp, then a re-index of the
        // same file — the graphify migration pattern without a forced
        // reindex of the whole DB.
        graph
            .index_file(abs.to_str().unwrap(), b"fn migrate_widget_fn() {}\n")
            .unwrap();
        graph.set_index_root(&root).unwrap();
        std::fs::write(
            &abs,
            b"fn migrate_widget_fn() {}\nfn migrate_widget_v2() {}\n",
        )
        .unwrap();
        graph
            .index_file(
                abs.to_str().unwrap(),
                b"fn migrate_widget_fn() {}\nfn migrate_widget_v2() {}\n",
            )
            .unwrap();

        // Exactly one File node, lazily migrated to the root-relative form.
        let all = graph
            .files
            .find_all_file_nodes(abs.to_str().unwrap())
            .unwrap();
        assert_eq!(
            all.len(),
            1,
            "re-indexing a dual-read legacy row must not create a duplicate"
        );
        assert_eq!(
            all[0].1.path, "src/migrate_widget.rs",
            "re-indexing must lazily migrate the stored path to root-relative POSIX"
        );
        let syms = graph.symbols_in_file("src/migrate_widget.rs").unwrap();
        assert_eq!(
            syms.len(),
            2,
            "re-indexed file must expose both symbols via the relative path"
        );
    }

    #[test]
    fn test_phase2_delete_via_relative_path_removes_all_records() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("graph.db");
        let root = canonical_root(&temp);
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let abs = src_dir.join("del_widget.rs");
        let source = b"fn del_callee() {}\nfn del_caller() { del_callee(); }\n";
        std::fs::write(&abs, source).unwrap();
        let abs_str = abs.to_string_lossy().to_string();

        let mut graph = crate::CodeGraph::open(&db).unwrap();
        graph.set_index_root(&root).unwrap();
        graph.index_file(&abs_str, source).unwrap();

        // Ground truth before deletion.
        let calls_before = count_location_entities(&graph, "Call", &abs_str);
        assert!(
            graph
                .files
                .find_file_node("src/del_widget.rs")
                .unwrap()
                .is_some(),
            "relative stored key must resolve before deletion"
        );

        // Delete via the root-relative stored path.
        let result = graph.delete_file("src/del_widget.rs").unwrap();
        assert_eq!(result.symbols_deleted, 2, "both functions must be deleted");
        assert_eq!(
            result.calls_deleted, calls_before,
            "every Call location record must be deleted (matched via re-anchored absolute form)"
        );
        assert!(
            result.chunks_deleted >= 1,
            "code chunks keyed by the ingest-time absolute path must be deleted"
        );

        // No resurrection through any query form.
        assert!(graph
            .files
            .find_file_node("src/del_widget.rs")
            .unwrap()
            .is_none());
        assert!(graph.files.find_file_node(&abs_str).unwrap().is_none());
        assert!(
            graph.all_file_nodes().unwrap().is_empty(),
            "file_index must not retain a stale entry for the deleted file"
        );
        assert_eq!(
            count_location_entities(&graph, "Call", &abs_str),
            0,
            "no orphaned Call rows may survive deletion"
        );
        assert_eq!(
            count_location_entities(&graph, "Reference", &abs_str),
            0,
            "no orphaned Reference rows may survive deletion"
        );
    }
}
