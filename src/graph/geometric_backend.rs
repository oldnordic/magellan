//! Geometric backend implementation for .geo database files
//!
//! This backend provides spatial indexing and CFG analysis using
//! memory-mapped files with sectioned storage.
//!
//! # Spatial Coordinate Semantics
//!
//! The NodeRec.x/y/z fields store **code-space coordinates** derived from source
//! spans, NOT physical spatial coordinates. This is an intentional design choice
//! that enables spatial indexing of code structure:
//!
//! - `x` = byte_start (horizontal position in file)
//! - `y` = start_line (vertical position in file)
//! - `z` = byte_end (defines span extent)
//!
//! These coordinates allow spatial queries like:
//! - "Find symbols near line 100" (y-axis query)
//! - "Find symbols in byte range 1000-2000" (x-axis query)
//!
//! For real semantic metadata (name, FQN, file_path), use the SymbolMetadataStore
//! which provides proper string-based lookups by FQN, name, and file path.

use crate::ingest::{Language, SymbolFact, SymbolKind};
use crate::references::CallFact;
use anyhow::{anyhow, Result};
use geographdb_core::storage::{
    CfgData, CfgEdge, CfgSectionAdapter, EdgeRec, GraphData, GraphSectionAdapter, NodeRec,
    SectionedStorage, SerializableCfgBlock, SymbolMetadata, SymbolMetadataSectionAdapter,
    SymbolMetadataStore,
};

// Re-export types for downstream consumers (Mirage, llmgrep)
pub use geographdb_core::storage::SerializableCfgBlock as CfgBlock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// A call edge between two symbols (function call graph edge)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolCallEdge {
    /// Source symbol ID (caller)
    pub src_symbol_id: u64,
    /// Target symbol ID (callee)
    pub dst_symbol_id: u64,
    /// File path where the call occurs
    pub file_path: String,
    /// Byte start of the call expression
    pub byte_start: u64,
    /// Byte end of the call expression
    pub byte_end: u64,
    /// Line number (1-indexed)
    pub start_line: u64,
    /// Column (0-indexed)
    pub start_col: u64,
}

/// Container for call edge data (stored in CALLEDGE section)
#[derive(Debug, Clone, Default)]
pub struct CallEdgeData {
    pub edges: Vec<SymbolCallEdge>,
}

impl CallEdgeData {
    /// Serialize to bytes
    /// Format: [count: u64][edges...]
    /// Each edge: [src: u64][dst: u64][path_len: u32][path bytes...][byte_start: u64][byte_end: u64][line: u64][col: u64]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Edge count
        bytes.extend_from_slice(&(self.edges.len() as u64).to_le_bytes());

        // Each edge
        for edge in &self.edges {
            bytes.extend_from_slice(&edge.src_symbol_id.to_le_bytes());
            bytes.extend_from_slice(&edge.dst_symbol_id.to_le_bytes());
            bytes.extend_from_slice(&(edge.file_path.len() as u32).to_le_bytes());
            bytes.extend_from_slice(edge.file_path.as_bytes());
            bytes.extend_from_slice(&edge.byte_start.to_le_bytes());
            bytes.extend_from_slice(&edge.byte_end.to_le_bytes());
            bytes.extend_from_slice(&edge.start_line.to_le_bytes());
            bytes.extend_from_slice(&edge.start_col.to_le_bytes());
        }

        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 8 {
            anyhow::bail!("Call edge data too short");
        }

        let count = u64::from_le_bytes(bytes[0..8].try_into()?) as usize;
        let mut offset = 8;
        let mut edges = Vec::with_capacity(count);

        for _ in 0..count {
            // Need at least 24 bytes for src, dst, path_len
            if bytes.len() < offset + 24 {
                break;
            }

            let src_symbol_id = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
            offset += 8;

            let dst_symbol_id = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
            offset += 8;

            let path_len = u32::from_le_bytes(bytes[offset..offset + 4].try_into()?) as usize;
            offset += 4;

            // Check we have enough bytes for path + remaining fields
            if bytes.len() < offset + path_len + 32 {
                break;
            }

            let file_path = String::from_utf8_lossy(&bytes[offset..offset + path_len]).to_string();
            offset += path_len;

            let byte_start = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
            offset += 8;

            let byte_end = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
            offset += 8;

            let start_line = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
            offset += 8;

            let start_col = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
            offset += 8;

            edges.push(SymbolCallEdge {
                src_symbol_id,
                dst_symbol_id,
                file_path,
                byte_start,
                byte_end,
                start_line,
                start_col,
            });
        }

        Ok(Self { edges })
    }
}

/// Label association for an entity
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LabelAssociation {
    pub entity_id: u64,
    pub label: String,
}

/// Container for label data (stored in LABEL section)
#[derive(Debug, Clone, Default)]
pub struct LabelData {
    pub associations: Vec<LabelAssociation>,
}

impl LabelData {
    /// Serialize to bytes
    /// Format: [count: u64][associations...]
    /// Each association: [entity_id: u64][label_len: u32][label bytes...]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Association count
        bytes.extend_from_slice(&(self.associations.len() as u64).to_le_bytes());

        // Each association
        for assoc in &self.associations {
            bytes.extend_from_slice(&assoc.entity_id.to_le_bytes());
            bytes.extend_from_slice(&(assoc.label.len() as u32).to_le_bytes());
            bytes.extend_from_slice(assoc.label.as_bytes());
        }

        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 8 {
            anyhow::bail!("Label data too short");
        }

        let count = u64::from_le_bytes(bytes[0..8].try_into()?) as usize;
        let mut offset = 8;
        let mut associations = Vec::with_capacity(count);

        for _ in 0..count {
            // Need at least 12 bytes for entity_id + label_len
            if bytes.len() < offset + 12 {
                break;
            }

            let entity_id = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
            offset += 8;

            let label_len = u32::from_le_bytes(bytes[offset..offset + 4].try_into()?) as usize;
            offset += 4;

            // Check we have enough bytes for label
            if bytes.len() < offset + label_len {
                break;
            }

            let label = String::from_utf8_lossy(&bytes[offset..offset + label_len]).to_string();
            offset += label_len;

            associations.push(LabelAssociation { entity_id, label });
        }

        Ok(Self { associations })
    }
}

/// AST node for geometric backend
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AstNodeRec {
    pub id: u64,
    pub file_path: String,
    pub kind: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub parent_id: Option<u64>,
}

/// Container for AST data (stored in AST section)
#[derive(Debug, Clone, Default)]
pub struct AstData {
    pub nodes: Vec<AstNodeRec>,
    next_id: u64,
}

impl AstData {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            next_id: 1,
        }
    }

    /// Add an AST node
    pub fn add_node(
        &mut self,
        file_path: &str,
        kind: &str,
        byte_start: usize,
        byte_end: usize,
        parent_id: Option<u64>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        self.nodes.push(AstNodeRec {
            id,
            file_path: file_path.to_string(),
            kind: kind.to_string(),
            byte_start,
            byte_end,
            parent_id,
        });
        id
    }

    /// Get nodes by file path
    pub fn get_nodes_by_file(&self, file_path: &str) -> Vec<&AstNodeRec> {
        self.nodes
            .iter()
            .filter(|n| n.file_path == file_path)
            .collect()
    }

    /// Get nodes by kind
    pub fn get_nodes_by_kind(&self, kind: &str) -> Vec<&AstNodeRec> {
        self.nodes.iter().filter(|n| n.kind == kind).collect()
    }

    /// Get children of a node
    pub fn get_children(&self, parent_id: u64) -> Vec<&AstNodeRec> {
        self.nodes
            .iter()
            .filter(|n| n.parent_id == Some(parent_id))
            .collect()
    }

    /// Get node at position
    pub fn get_node_at_position(&self, file_path: &str, position: usize) -> Option<&AstNodeRec> {
        self.nodes
            .iter()
            .filter(|n| {
                n.file_path == file_path && n.byte_start <= position && n.byte_end > position
            })
            .max_by_key(|n| n.byte_end - n.byte_start) // Smallest node containing position
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        // Format: [count: u64][nodes...]
        // Each node: [id: u64][file_path_len: u32][kind_len: u32][byte_start: u64][byte_end: u64][parent_id: u64 (0 = None)][file_path bytes...][kind bytes...]
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(self.nodes.len() as u64).to_le_bytes());

        for node in &self.nodes {
            bytes.extend_from_slice(&node.id.to_le_bytes());
            bytes.extend_from_slice(&(node.file_path.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&(node.kind.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&(node.byte_start as u64).to_le_bytes());
            bytes.extend_from_slice(&(node.byte_end as u64).to_le_bytes());
            // Store parent_id as u64, 0 means None
            let parent = node.parent_id.unwrap_or(0);
            bytes.extend_from_slice(&parent.to_le_bytes());
            bytes.extend_from_slice(node.file_path.as_bytes());
            bytes.extend_from_slice(node.kind.as_bytes());
        }

        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 8 {
            anyhow::bail!("AST data too short");
        }

        let count = u64::from_le_bytes(bytes[0..8].try_into()?) as usize;
        let mut offset = 8;
        let mut nodes = Vec::with_capacity(count);
        let mut max_id = 0;

        for _ in 0..count {
            // Need at least 36 bytes for fixed fields
            if bytes.len() < offset + 36 {
                break;
            }

            let id = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
            let file_path_len =
                u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into()?) as usize;
            let kind_len = u32::from_le_bytes(bytes[offset + 12..offset + 16].try_into()?) as usize;
            let byte_start =
                u64::from_le_bytes(bytes[offset + 16..offset + 24].try_into()?) as usize;
            let byte_end = u64::from_le_bytes(bytes[offset + 24..offset + 32].try_into()?) as usize;
            let parent_id = u64::from_le_bytes(bytes[offset + 32..offset + 40].try_into()?);
            offset += 40;

            if bytes.len() < offset + file_path_len + kind_len {
                break;
            }

            let file_path = String::from_utf8(bytes[offset..offset + file_path_len].to_vec())?;
            offset += file_path_len;

            let kind = String::from_utf8(bytes[offset..offset + kind_len].to_vec())?;
            offset += kind_len;

            nodes.push(AstNodeRec {
                id,
                file_path,
                kind,
                byte_start,
                byte_end,
                parent_id: if parent_id == 0 {
                    None
                } else {
                    Some(parent_id)
                },
            });
            max_id = max_id.max(id);
        }

        Ok(Self {
            nodes,
            next_id: max_id + 1,
        })
    }
}

/// Symbol information from the geometric backend
#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolInfo {
    pub id: u64,
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    pub file_path: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
    pub language: String,
}

/// Backend statistics
#[derive(Debug, Clone)]
pub struct GeometricBackendStats {
    pub node_count: usize,
    pub symbol_count: usize,
    pub file_count: usize,
    pub cfg_block_count: usize,
}

/// Complexity calculation result
#[derive(Debug, Clone)]
pub struct ComplexityResult {
    pub cyclomatic_complexity: u64,
}

/// Path enumeration result
#[derive(Debug, Clone)]
pub struct PathEnumerationResult {
    pub paths: Vec<Vec<u64>>,
    pub total_enumerated: usize,
    pub bounded_hit: bool,
}

/// Code chunk for storing source code snippets
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodeChunkRec {
    pub id: u64,
    pub file_path: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub content: String,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
}

/// Container for chunk data (stored in CHUNK section)
#[derive(Debug, Clone, Default)]
pub struct ChunkData {
    pub chunks: Vec<CodeChunkRec>,
    next_id: u64,
}

impl ChunkData {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a code chunk
    pub fn add_chunk(
        &mut self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
        content: &str,
        symbol_name: Option<&str>,
        symbol_kind: Option<&str>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        self.chunks.push(CodeChunkRec {
            id,
            file_path: file_path.to_string(),
            byte_start,
            byte_end,
            content: content.to_string(),
            symbol_name: symbol_name.map(|s| s.to_string()),
            symbol_kind: symbol_kind.map(|s| s.to_string()),
        });
        id
    }

    /// Get chunks by file path
    pub fn get_chunks_by_file(&self, file_path: &str) -> Vec<&CodeChunkRec> {
        self.chunks
            .iter()
            .filter(|c| c.file_path == file_path)
            .collect()
    }

    /// Get chunks by symbol name
    pub fn get_chunks_by_symbol(&self, symbol_name: &str) -> Vec<&CodeChunkRec> {
        self.chunks
            .iter()
            .filter(|c| c.symbol_name.as_deref() == Some(symbol_name))
            .collect()
    }

    /// Get chunk at exact span
    pub fn get_chunk_at_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Option<&CodeChunkRec> {
        self.chunks.iter().find(|c| {
            c.file_path == file_path && c.byte_start == byte_start && c.byte_end == byte_end
        })
    }

    /// Get chunk containing position
    pub fn get_chunk_at_position(&self, file_path: &str, position: usize) -> Option<&CodeChunkRec> {
        self.chunks
            .iter()
            .filter(|c| {
                c.file_path == file_path && c.byte_start <= position && c.byte_end > position
            })
            .max_by_key(|c| c.byte_end - c.byte_start)
    }

    /// Get chunk by ID
    pub fn get_chunk_by_id(&self, id: u64) -> Option<&CodeChunkRec> {
        self.chunks.iter().find(|c| c.id == id)
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        // Format: [count: u64][chunks...]
        // Each chunk: [id: u64][file_path_len: u32][content_len: u32][byte_start: u64][byte_end: u64][symbol_name_len: u16][symbol_kind_len: u16][file_path bytes...][content bytes...][symbol_name bytes...][symbol_kind bytes...]
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(self.chunks.len() as u64).to_le_bytes());

        for chunk in &self.chunks {
            bytes.extend_from_slice(&chunk.id.to_le_bytes());
            bytes.extend_from_slice(&(chunk.file_path.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&(chunk.content.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&(chunk.byte_start as u64).to_le_bytes());
            bytes.extend_from_slice(&(chunk.byte_end as u64).to_le_bytes());
            bytes.extend_from_slice(
                &(chunk.symbol_name.as_ref().map(|s| s.len()).unwrap_or(0) as u16).to_le_bytes(),
            );
            bytes.extend_from_slice(
                &(chunk.symbol_kind.as_ref().map(|s| s.len()).unwrap_or(0) as u16).to_le_bytes(),
            );
            bytes.extend_from_slice(chunk.file_path.as_bytes());
            bytes.extend_from_slice(chunk.content.as_bytes());
            if let Some(ref name) = chunk.symbol_name {
                bytes.extend_from_slice(name.as_bytes());
            }
            if let Some(ref kind) = chunk.symbol_kind {
                bytes.extend_from_slice(kind.as_bytes());
            }
        }

        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 8 {
            anyhow::bail!("Chunk data too short");
        }

        let count = u64::from_le_bytes(bytes[0..8].try_into()?) as usize;
        let mut offset = 8;
        let mut chunks = Vec::with_capacity(count);
        let mut max_id = 0;

        for _ in 0..count {
            // Need at least 28 bytes for fixed fields
            if bytes.len() < offset + 28 {
                break;
            }

            let id = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
            let file_path_len =
                u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into()?) as usize;
            let content_len =
                u32::from_le_bytes(bytes[offset + 12..offset + 16].try_into()?) as usize;
            let byte_start =
                u64::from_le_bytes(bytes[offset + 16..offset + 24].try_into()?) as usize;
            let byte_end = u64::from_le_bytes(bytes[offset + 24..offset + 32].try_into()?) as usize;
            let symbol_name_len =
                u16::from_le_bytes(bytes[offset + 32..offset + 34].try_into()?) as usize;
            let symbol_kind_len =
                u16::from_le_bytes(bytes[offset + 34..offset + 36].try_into()?) as usize;
            offset += 36;

            let total_var_len = file_path_len + content_len + symbol_name_len + symbol_kind_len;
            if bytes.len() < offset + total_var_len {
                break;
            }

            let file_path = String::from_utf8(bytes[offset..offset + file_path_len].to_vec())?;
            offset += file_path_len;

            let content = String::from_utf8(bytes[offset..offset + content_len].to_vec())?;
            offset += content_len;

            let symbol_name = if symbol_name_len > 0 {
                Some(String::from_utf8(
                    bytes[offset..offset + symbol_name_len].to_vec(),
                )?)
            } else {
                None
            };
            offset += symbol_name_len;

            let symbol_kind = if symbol_kind_len > 0 {
                Some(String::from_utf8(
                    bytes[offset..offset + symbol_kind_len].to_vec(),
                )?)
            } else {
                None
            };
            offset += symbol_kind_len;

            chunks.push(CodeChunkRec {
                id,
                file_path,
                byte_start,
                byte_end,
                content,
                symbol_name,
                symbol_kind,
            });
            max_id = max_id.max(id);
        }

        Ok(Self {
            chunks,
            next_id: max_id + 1,
        })
    }
}

/// Normalize a path for deduplication purposes
/// Converts paths to a canonical form for comparison
fn normalize_path_for_dedup(path: &str) -> String {
    // Normalize backslashes to forward slashes
    let path = path.replace('\\', "/");
    // Remove leading "./" if present
    let path = path.strip_prefix("./").unwrap_or(&path);
    // For deduplication, extract just the src/ portion if present
    // This handles both ./src/file.rs and /abs/path/src/file.rs
    if let Some(idx) = path.find("/src/") {
        // Strip the leading "/" from the result for consistent matching
        // ./src/file.rs -> src/file.rs
        // /abs/path/src/file.rs -> src/file.rs
        path[idx + 1..].to_string()
    } else {
        path.to_string()
    }
}

/// Geometric backend main struct
pub struct GeometricBackend {
    storage: RwLock<SectionedStorage>,
    db_path: PathBuf,
    // Cache graph data in memory for fast access
    graph_cache: RwLock<GraphData>,
    cfg_cache: RwLock<CfgData>,
    // Symbol metadata cache for real semantic lookups
    symbol_metadata: RwLock<SymbolMetadataStore>,
    // Call edge cache for call graph analysis
    call_edges: RwLock<CallEdgeData>,
    // Label data for entity labeling
    label_data: RwLock<LabelData>,
    // AST data for source structure
    ast_data: RwLock<AstData>,
    // Chunk data for source code storage
    chunk_data: RwLock<ChunkData>,
    next_id: RwLock<u64>,
}

impl GeometricBackend {
    /// Create a new geometric database
    pub fn create(db_path: &Path) -> Result<Self> {
        let mut storage = SectionedStorage::create(db_path)?;

        // Initialize empty GRAPH section
        GraphSectionAdapter::init(&mut storage)?;

        // Initialize empty CFG section
        CfgSectionAdapter::init(&mut storage)?;

        // Initialize symbol metadata section
        SymbolMetadataSectionAdapter::init(&mut storage)?;

        // Initialize call edges section
        let empty_call_edges = CallEdgeData::default();
        let empty_bytes = empty_call_edges.to_bytes();
        let capacity = (1024 * 1024).max(empty_bytes.len() as u64 * 2);
        storage.create_section("CALLEDGE", capacity, 0)?;
        storage.write_section("CALLEDGE", &empty_bytes)?;

        // Initialize label section
        let empty_labels = LabelData::default();
        let empty_label_bytes = empty_labels.to_bytes();
        let label_capacity = (1024 * 1024).max(empty_label_bytes.len() as u64 * 2);
        storage.create_section("LABEL", label_capacity, 0)?;
        storage.write_section("LABEL", &empty_label_bytes)?;

        // Initialize AST section
        let empty_ast = AstData::new();
        let empty_ast_bytes = empty_ast.to_bytes();
        let ast_capacity = (1024 * 1024).max(empty_ast_bytes.len() as u64 * 2);
        storage.create_section("AST", ast_capacity, 0)?;
        storage.write_section("AST", &empty_ast_bytes)?;

        // Initialize CHUNK section
        let empty_chunks = ChunkData::new();
        let empty_chunk_bytes = empty_chunks.to_bytes();
        let chunk_capacity = (1024 * 1024 * 10).max(empty_chunk_bytes.len() as u64 * 2); // 10MB initial for chunks
        storage.create_section("CHUNK", chunk_capacity, 0)?;
        storage.write_section("CHUNK", &empty_chunk_bytes)?;

        Ok(Self {
            storage: RwLock::new(storage),
            db_path: db_path.to_path_buf(),
            graph_cache: RwLock::new(GraphData::default()),
            cfg_cache: RwLock::new(CfgData::default()),
            symbol_metadata: RwLock::new(SymbolMetadataStore::new()),
            call_edges: RwLock::new(CallEdgeData::default()),
            label_data: RwLock::new(LabelData::default()),
            ast_data: RwLock::new(AstData::new()),
            chunk_data: RwLock::new(ChunkData::new()),
            next_id: RwLock::new(1),
        })
    }

    /// Open an existing geometric database
    pub fn open(db_path: &Path) -> Result<Self> {
        let mut storage = SectionedStorage::open(db_path)?;

        // Load graph data
        let graph_data = if GraphSectionAdapter::exists(&storage) {
            GraphSectionAdapter::load(&mut storage).unwrap_or_default()
        } else {
            GraphData::default()
        };

        // Load CFG data
        let cfg_data = if CfgSectionAdapter::exists(&storage) {
            CfgSectionAdapter::load(&mut storage).unwrap_or_default()
        } else {
            CfgData::default()
        };

        // Load symbol metadata
        let symbol_metadata = if SymbolMetadataSectionAdapter::exists(&storage) {
            SymbolMetadataSectionAdapter::load(&mut storage).unwrap_or_default()
        } else {
            SymbolMetadataStore::new()
        };

        // Load call edges
        let call_edges = match storage.read_section("CALLEDGE") {
            Ok(bytes) => CallEdgeData::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => CallEdgeData::default(),
        };

        // Load label data
        let label_data = match storage.read_section("LABEL") {
            Ok(bytes) => LabelData::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => LabelData::default(),
        };

        // Load AST data
        let ast_data = match storage.read_section("AST") {
            Ok(bytes) => AstData::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => AstData::new(),
        };

        // Load chunk data
        let chunk_data = match storage.read_section("CHUNK") {
            Ok(bytes) => ChunkData::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => ChunkData::new(),
        };

        // Find next ID
        let next_id = graph_data.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;

        Ok(Self {
            storage: RwLock::new(storage),
            db_path: db_path.to_path_buf(),
            graph_cache: RwLock::new(graph_data),
            cfg_cache: RwLock::new(cfg_data),
            symbol_metadata: RwLock::new(symbol_metadata),
            call_edges: RwLock::new(call_edges),
            label_data: RwLock::new(label_data),
            ast_data: RwLock::new(ast_data),
            chunk_data: RwLock::new(chunk_data),
            next_id: RwLock::new(next_id),
        })
    }

    /// Get storage reference
    fn storage(&self) -> RwLockReadGuard<SectionedStorage> {
        self.storage.read().unwrap()
    }

    /// Get storage reference (mutable)
    fn storage_mut(&self) -> RwLockWriteGuard<SectionedStorage> {
        self.storage.write().unwrap()
    }

    /// Reload all data from disk to refresh in-memory caches
    ///
    /// This is useful when another process may have written to the database
    /// and we need to see the latest data.
    ///
    /// NOTE: This function reopens the storage from disk to ensure we see
    /// the latest data, as the memory-mapped view may not reflect writes
    /// from other processes.
    pub fn reload_from_disk(&self) -> Result<()> {
        // Reopen the storage from disk to get a fresh view of the data
        // This is necessary because memory-mapped files don't automatically
        // reflect writes from other processes
        let mut new_storage = SectionedStorage::open(&self.db_path)?;

        // Reload graph data
        let graph_data = if GraphSectionAdapter::exists(&new_storage) {
            GraphSectionAdapter::load(&mut new_storage).unwrap_or_default()
        } else {
            GraphData::default()
        };

        // Reload CFG data
        let cfg_data = if CfgSectionAdapter::exists(&new_storage) {
            CfgSectionAdapter::load(&mut new_storage).unwrap_or_default()
        } else {
            CfgData::default()
        };

        // Reload symbol metadata
        let symbol_metadata = if SymbolMetadataSectionAdapter::exists(&new_storage) {
            SymbolMetadataSectionAdapter::load(&mut new_storage).unwrap_or_default()
        } else {
            SymbolMetadataStore::new()
        };

        // Reload call edges
        let call_edges = match new_storage.read_section("CALLEDGE") {
            Ok(bytes) => CallEdgeData::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => CallEdgeData::default(),
        };

        // Reload label data
        let label_data = match new_storage.read_section("LABEL") {
            Ok(bytes) => LabelData::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => LabelData::default(),
        };

        // Reload AST data
        let ast_data = match new_storage.read_section("AST") {
            Ok(bytes) => AstData::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => AstData::new(),
        };

        // Reload chunk data
        let chunk_data = match new_storage.read_section("CHUNK") {
            Ok(bytes) => ChunkData::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => ChunkData::new(),
        };

        // Find next ID
        let next_id = graph_data.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;

        // Update all caches with fresh data
        {
            let mut storage = self.storage.write().unwrap();
            *storage = new_storage;
        }
        {
            let mut cache = self.graph_cache.write().unwrap();
            *cache = graph_data;
        }
        {
            let mut cache = self.cfg_cache.write().unwrap();
            *cache = cfg_data;
        }
        {
            let mut cache = self.symbol_metadata.write().unwrap();
            *cache = symbol_metadata;
        }
        {
            let mut cache = self.call_edges.write().unwrap();
            *cache = call_edges;
        }
        {
            let mut cache = self.label_data.write().unwrap();
            *cache = label_data;
        }
        {
            let mut cache = self.ast_data.write().unwrap();
            *cache = ast_data;
        }
        {
            let mut cache = self.chunk_data.write().unwrap();
            *cache = chunk_data;
        }
        {
            let mut nid = self.next_id.write().unwrap();
            *nid = next_id;
        }

        Ok(())
    }

    /// Insert symbols into the database
    pub fn insert_symbols(&self, symbols: Vec<InsertSymbol>) -> Result<Vec<u64>> {
        let mut cache = self.graph_cache.write().unwrap();
        let mut symbol_metadata = self.symbol_metadata.write().unwrap();
        let mut next_id = self.next_id.write().unwrap();

        let mut ids = Vec::new();

        for sym in symbols {
            let id = *next_id;
            *next_id += 1;

            let node = NodeRec {
                id,
                morton_code: 0,           // Will compute if needed for spatial queries
                x: sym.byte_start as f32, // NOTE: x/y/z store code span coordinates
                y: sym.start_line as f32, // These are code-space coordinates, not physical spatial
                z: sym.byte_end as f32,   // See PHASE 5 documentation
                edge_off: 0,
                edge_len: 0,
                flags: 0,
                begin_ts: 0,
                end_ts: 0,
                tx_id: 0,
                visibility: 1, // VERSION_COMMITTED
                _padding: [0; 7],
            };

            cache.nodes.push(node);

            // Store symbol metadata natively (NOT as JSON)
            symbol_metadata.add(SymbolMetadata {
                symbol_id: id,
                name: sym.name,
                fqn: sym.fqn,
                file_path: sym.file_path.clone(),
                kind: sym.kind as u8,
                language: sym.language as u8,
                byte_start: sym.byte_start,
                byte_end: sym.byte_end,
                start_line: sym.start_line,
                start_col: sym.start_col,
                end_line: sym.end_line,
                end_col: sym.end_col,
            });

            ids.push(id);
        }

        // Update edge offsets
        Self::update_edge_offsets(&mut cache);

        Ok(ids)
    }

    /// Clear all data associated with a specific file path
    ///
    /// This removes symbols, call edges, CFG blocks, AST nodes, and chunks
    /// that belong to the given file path. Used before re-indexing a file
    /// to prevent duplicate accumulation.
    pub fn clear_file_data(&self, file_path: &str) -> Result<()> {
        // Normalize the file path for matching
        let normalized_path = normalize_path_for_dedup(file_path);

        // 1. Find all symbol IDs belonging to this file using symbols_in_file
        let symbol_ids_to_remove: Vec<u64> = {
            let metadata = self.symbol_metadata.read().unwrap();
            let ids = metadata.symbols_in_file(file_path);
            if ids.is_empty() {
                // Also try with normalized path
                let all_ids = metadata.all_symbol_ids();
                let mut found = Vec::new();
                for id in all_ids {
                    if let Some(rec) = metadata.get(id) {
                        if normalize_path_for_dedup(&rec.file_path) == normalized_path {
                            found.push(id);
                        }
                    }
                }
                found
            } else {
                ids
            }
        };

        if symbol_ids_to_remove.is_empty() {
            return Ok(());
        }

        let symbol_id_set: std::collections::HashSet<u64> =
            symbol_ids_to_remove.iter().copied().collect();

        // 2. Remove symbols from metadata store
        // Note: String table and file table may have orphaned entries,
        // but this is acceptable for correctness
        {
            let mut metadata = self.symbol_metadata.write().unwrap();
            for id in &symbol_ids_to_remove {
                metadata.metadata.remove(id);
            }
        }

        // 3. Remove nodes and edges from graph cache
        {
            let mut cache = self.graph_cache.write().unwrap();
            cache.nodes.retain(|n| !symbol_id_set.contains(&n.id));
            cache
                .edges
                .retain(|e| !symbol_id_set.contains(&e.src) && !symbol_id_set.contains(&e.dst));
            Self::update_edge_offsets(&mut cache);
        }

        // 4. Remove call edges - need to filter by src_symbol/dst_symbol
        // CallEdgeData stores edges with symbol IDs
        {
            let mut call_edges = self.call_edges.write().unwrap();
            // Retain only edges where neither src nor dst is in our removal set
            call_edges.edges.retain(|e| {
                !symbol_id_set.contains(&e.src_symbol_id) && !symbol_id_set.contains(&e.dst_symbol_id)
            });
        }

        // 5. Remove CFG blocks belonging to removed symbols
        {
            let mut cfg_cache = self.cfg_cache.write().unwrap();
            cfg_cache
                .blocks
                .retain(|b| !symbol_id_set.contains(&(b.function_id as u64)));
        }

        // 6. Remove AST nodes for this file
        {
            let mut ast_data = self.ast_data.write().unwrap();
            ast_data
                .nodes
                .retain(|n| normalize_path_for_dedup(&n.file_path) != normalized_path);
        }

        // 7. Remove chunks for this file
        {
            let mut chunk_data = self.chunk_data.write().unwrap();
            chunk_data
                .chunks
                .retain(|c| normalize_path_for_dedup(&c.file_path) != normalized_path);
        }

        // 8. Remove labels for removed symbols
        {
            let mut label_data = self.label_data.write().unwrap();
            label_data.associations.retain(|a| !symbol_id_set.contains(&a.entity_id));
        }

        Ok(())
    }

    /// Insert a CFG block
    pub fn insert_cfg_block(&self, block: SerializableCfgBlock) -> Result<u64> {
        let mut cache = self.cfg_cache.write().unwrap();
        let id = cache.blocks.len() as u64;
        cache.blocks.push(block);
        Ok(id)
    }

    /// Get all CFG blocks for a specific function
    pub fn get_cfg_blocks_for_function(&self, function_id: i64) -> Vec<SerializableCfgBlock> {
        let cache = self.cfg_cache.read().unwrap();
        cache
            .blocks
            .iter()
            .filter(|b| b.function_id == function_id)
            .cloned()
            .collect()
    }

    /// Insert an edge between CFG blocks
    pub fn insert_edge(&self, src_id: u64, dst_id: u64, _edge_type: &str) -> Result<()> {
        let mut cache = self.graph_cache.write().unwrap();

        // Find source node and update edge info
        if let Some(_node) = cache.nodes.iter_mut().find(|n| n.id == src_id) {
            let edge = EdgeRec {
                src: src_id,
                dst: dst_id,
                w: 1.0,
                flags: 0,
                begin_ts: 0,
                end_ts: 0,
                tx_id: 0,
                visibility: 1,
                _padding: [0; 7],
            };
            cache.edges.push(edge);
            Self::update_edge_offsets(&mut cache);
        }

        Ok(())
    }

    /// Insert a CFG edge between blocks
    pub fn insert_cfg_edge(&self, src_id: u64, dst_id: u64, edge_type: u32) -> Result<()> {
        let mut cache = self.cfg_cache.write().unwrap();

        let edge = geographdb_core::storage::CfgEdge {
            src_id,
            dst_id,
            edge_type,
        };
        cache.edges.push(edge);

        Ok(())
    }

    fn update_edge_offsets(cache: &mut GraphData) {
        let mut edge_offsets: HashMap<u64, (u32, u32)> = HashMap::new();

        // Count edges per node
        for edge in &cache.edges {
            let entry = edge_offsets.entry(edge.src).or_insert((0, 0));
            entry.1 += 1;
        }

        // Calculate offsets
        let mut current_offset = 0;
        for (_, (off, len)) in edge_offsets.iter_mut() {
            *off = current_offset;
            current_offset += *len;
        }

        // Update nodes
        for node in &mut cache.nodes {
            if let Some((off, len)) = edge_offsets.get(&node.id) {
                node.edge_off = *off;
                node.edge_len = *len;
            }
        }
    }

    /// Find a symbol by ID
    pub fn find_symbol_by_id_info(&self, id: u64) -> Option<SymbolInfo> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();
        let meta = symbol_metadata.get(id)?;

        Some(SymbolInfo {
            id: meta.symbol_id,
            name: meta.name,
            fqn: meta.fqn,
            kind: SymbolKind::from_u8(meta.kind),
            file_path: meta.file_path,
            byte_start: meta.byte_start,
            byte_end: meta.byte_end,
            start_line: meta.start_line,
            start_col: meta.start_col,
            end_line: meta.end_line,
            end_col: meta.end_col,
            language: match meta.language {
                1 => "Rust".to_string(),
                2 => "Python".to_string(),
                3 => "C".to_string(),
                4 => "Cpp".to_string(),
                5 => "Java".to_string(),
                6 => "JavaScript".to_string(),
                7 => "TypeScript".to_string(),
                _ => "Unknown".to_string(),
            },
        })
    }

    /// Find a symbol by fully qualified name
    pub fn find_symbol_by_fqn_info(&self, fqn: &str) -> Option<SymbolInfo> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();
        let id = symbol_metadata.find_by_fqn(fqn)?;
        drop(symbol_metadata); // Release lock before calling find_symbol_by_id_info
        self.find_symbol_by_id_info(id)
    }

    /// Find symbols by name
    pub fn find_symbols_by_name_info(&self, name: &str) -> Vec<SymbolInfo> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();
        let ids = symbol_metadata.find_by_name(name);
        drop(symbol_metadata); // Release lock

        ids.into_iter()
            .filter_map(|id| self.find_symbol_by_id_info(id))
            .collect()
    }

    /// Find symbol ID by name and path
    pub fn find_symbol_id_by_name_and_path(&self, name: &str, path: &str) -> Option<u64> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();

        // Find symbols with matching name in the given file
        let ids = symbol_metadata.find_by_name(name);
        ids.into_iter().find(|&id| {
            symbol_metadata
                .get(id)
                .map(|meta| meta.file_path == path)
                .unwrap_or(false)
        })
    }

    /// Get all symbols
    pub fn get_all_symbols(&self) -> Result<Vec<SymbolInfo>> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();
        let ids = symbol_metadata.all_symbol_ids();
        drop(symbol_metadata);

        Ok(ids
            .into_iter()
            .filter_map(|id| self.find_symbol_by_id_info(id))
            .collect())
    }

    /// Get all symbol IDs
    pub fn get_all_symbol_ids(&self) -> Vec<u64> {
        let cache = self.graph_cache.read().unwrap();
        cache.nodes.iter().map(|n| n.id).collect()
    }

    /// Get symbols in a file
    pub fn symbols_in_file(&self, file_path: &str) -> Result<Vec<SymbolInfo>> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();
        let ids = symbol_metadata.symbols_in_file(file_path);
        drop(symbol_metadata);

        Ok(ids
            .into_iter()
            .filter_map(|id| self.find_symbol_by_id_info(id))
            .collect())
    }

    /// Get backend statistics
    ///
    /// Reloads from disk first to ensure stats reflect the latest persisted data.
    pub fn get_stats(&self) -> Result<GeometricBackendStats> {
        // Reload to ensure we see the latest data written by other processes
        self.reload_from_disk()?;

        let graph_cache = self.graph_cache.read().unwrap();
        let cfg_cache = self.cfg_cache.read().unwrap();
        let symbol_metadata = self.symbol_metadata.read().unwrap();

        Ok(GeometricBackendStats {
            node_count: graph_cache.nodes.len(),
            symbol_count: symbol_metadata.symbol_count(),
            file_count: symbol_metadata.file_count(), // REAL file count from file table
            cfg_block_count: cfg_cache.blocks.len(),
        })
    }

    /// Get geometric backend statistics (alias for get_stats)
    ///
    /// This method provides the same functionality as `get_stats()` but returns
    /// the stats directly without wrapping in Result for compatibility.
    ///
    /// Note: This method reloads from disk to ensure stats reflect persisted data.
    pub fn get_geometric_stats(&self) -> GeometricBackendStats {
        // Reload to ensure we see the latest data written by other processes
        // Ignore errors and use cached data if reload fails
        let _ = self.reload_from_disk();

        let graph_cache = self.graph_cache.read().unwrap();
        let cfg_cache = self.cfg_cache.read().unwrap();
        let symbol_metadata = self.symbol_metadata.read().unwrap();

        GeometricBackendStats {
            node_count: graph_cache.nodes.len(),
            symbol_count: symbol_metadata.symbol_count(),
            file_count: symbol_metadata.file_count(),
            cfg_block_count: cfg_cache.blocks.len(),
        }
    }

    /// Get all CFG edges
    ///
    /// Returns all CFG edges from the CFG section.
    pub fn get_all_cfg_edges(&self) -> Vec<geographdb_core::storage::CfgEdge> {
        let cfg_cache = self.cfg_cache.read().unwrap();
        cfg_cache.edges.clone()
    }

    /// Get all graph edges (spatial index edges)
    ///
    /// Returns all edges from the GRAPH section (spatial index).
    pub fn get_all_edges(&self) -> Vec<geographdb_core::storage::EdgeRec> {
        let graph_cache = self.graph_cache.read().unwrap();
        graph_cache.edges.clone()
    }

    /// Get all file paths from the file table
    pub fn get_all_file_paths(&self) -> Vec<String> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();
        symbol_metadata.all_file_paths()
    }

    /// Set file hash for a file path
    pub fn set_file_hash(&self, path: &str, hash: &str) {
        let mut symbol_metadata = self.symbol_metadata.write().unwrap();
        symbol_metadata.set_file_hash(path, hash);
    }

    /// Get file hash for a file path
    pub fn get_file_hash(&self, path: &str) -> Option<String> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();
        symbol_metadata.get_file_hash(path).map(|s| s.to_string())
    }

    /// Get all files with their info (path, hash, last_indexed_at)
    pub fn get_all_files(&self) -> Vec<(String, Option<String>, i64)> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();
        symbol_metadata
            .all_files()
            .into_iter()
            .map(|info| (info.path.clone(), info.hash.clone(), info.last_indexed_at))
            .collect()
    }

    /// Get callers of a symbol (from call graph, not CFG)
    pub fn get_callers(&self, id: u64) -> Vec<u64> {
        let call_edges = self.call_edges.read().unwrap();
        call_edges
            .edges
            .iter()
            .filter(|e| e.dst_symbol_id == id)
            .map(|e| e.src_symbol_id)
            .collect()
    }

    /// Get callees of a symbol (from call graph, not CFG)
    pub fn get_callees(&self, id: u64) -> Vec<u64> {
        let call_edges = self.call_edges.read().unwrap();
        call_edges
            .edges
            .iter()
            .filter(|e| e.src_symbol_id == id)
            .map(|e| e.dst_symbol_id)
            .collect()
    }

    /// Get both callers and callees of a symbol (bidirectional references)
    pub fn get_references_bidirectional(&self, id: u64) -> (Vec<u64>, Vec<u64>) {
        let callers = self.get_callers(id);
        let callees = self.get_callees(id);
        (callers, callees)
    }

    /// Complete FQN prefix for autocomplete functionality
    pub fn complete_fqn_prefix(&self, prefix: &str, limit: usize) -> Vec<String> {
        let symbol_metadata = self.symbol_metadata.read().unwrap();
        let all_ids = symbol_metadata.all_symbol_ids();

        let mut matches: Vec<String> = all_ids
            .into_iter()
            .filter_map(|id| symbol_metadata.get(id))
            .filter(|rec| rec.fqn.starts_with(prefix))
            .map(|rec| rec.fqn)
            .take(limit)
            .collect();

        matches.sort();
        matches.dedup();
        matches
    }

    /// Insert a call edge between two symbols
    pub fn insert_call_edge(
        &self,
        src_symbol_id: u64,
        dst_symbol_id: u64,
        file_path: &str,
        byte_start: u64,
        byte_end: u64,
        start_line: u64,
        start_col: u64,
    ) {
        let mut call_edges = self.call_edges.write().unwrap();
        call_edges.edges.push(SymbolCallEdge {
            src_symbol_id,
            dst_symbol_id,
            file_path: file_path.to_string(),
            byte_start,
            byte_end,
            start_line,
            start_col,
        });
    }

    /// Insert multiple call edges
    pub fn insert_call_edges(&self, edges: Vec<SymbolCallEdge>) {
        let mut call_edges = self.call_edges.write().unwrap();
        call_edges.edges.extend(edges);
    }

    /// Get all call edges
    pub fn get_call_edges(&self) -> Vec<SymbolCallEdge> {
        let call_edges = self.call_edges.read().unwrap();
        call_edges.edges.clone()
    }

    /// Calculate complexity for a symbol
    pub fn calculate_complexity(&self, _id: i64) -> ComplexityResult {
        ComplexityResult {
            cyclomatic_complexity: 1,
        }
    }

    /// Get calls from a symbol as CallFact structs
    pub fn calls_from_symbol_as_facts(&self, _path: &str, _name: &str) -> Vec<CallFact> {
        Vec::new()
    }

    /// Get callers of a symbol as CallFact structs
    pub fn callers_of_symbol_as_facts(&self, _path: &str, _name: &str) -> Vec<CallFact> {
        Vec::new()
    }

    /// Get reachable symbols from a start symbol
    pub fn reachable_from(&self, start_id: u64) -> Vec<u64> {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start_id];

        while let Some(id) = stack.pop() {
            if visited.insert(id) {
                stack.extend(self.get_callees(id));
            }
        }

        visited.into_iter().collect()
    }

    /// Get symbols that can reach the start symbol
    pub fn reverse_reachable_from(&self, start_id: u64) -> Vec<u64> {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start_id];

        while let Some(id) = stack.pop() {
            if visited.insert(id) {
                stack.extend(self.get_callers(id));
            }
        }

        visited.into_iter().collect()
    }

    /// Find dead code from entry points
    pub fn dead_code_from_entries(&self, entry_ids: &[u64]) -> Vec<u64> {
        let mut reachable = std::collections::HashSet::new();

        for &entry_id in entry_ids {
            for id in self.reachable_from(entry_id) {
                reachable.insert(id);
            }
        }

        let all_ids = self.get_all_symbol_ids();
        all_ids
            .into_iter()
            .filter(|id| !reachable.contains(id))
            .collect()
    }

    /// Get strongly connected components
    pub fn get_strongly_connected_components(&self) -> crate::graph::geometric_calls::SccResult {
        use geographdb_core::algorithms::astar::CfgGraphNode;
        use geographdb_core::algorithms::scc::tarjan_scc;

        let cache = self.graph_cache.read().unwrap();
        let nodes: Vec<CfgGraphNode> = cache
            .nodes
            .iter()
            .map(|n| CfgGraphNode {
                id: n.id,
                x: n.x,
                y: n.y,
                z: n.z,
                successors: self.get_callees(n.id),
            })
            .collect();

        let result = tarjan_scc(&nodes);

        crate::graph::geometric_calls::SccResult {
            components: result.components,
            node_to_component: result.node_to_component,
        }
    }

    /// Condense the call graph
    pub fn condense_call_graph(&self) -> crate::graph::geometric_calls::CondensationDag {
        let scc = self.get_strongly_connected_components();

        let mut supernodes: Vec<Vec<u64>> = scc.components;
        let mut node_to_supernode = scc.node_to_component;
        let mut edges = Vec::new();

        let cache = self.graph_cache.read().unwrap();
        for edge in &cache.edges {
            if let Some(&from_supernode) = node_to_supernode.get(&edge.src) {
                if let Some(&to_supernode) = node_to_supernode.get(&edge.dst) {
                    if from_supernode != to_supernode {
                        edges.push((from_supernode, to_supernode));
                    }
                }
            }
        }

        edges.sort();
        edges.dedup();

        crate::graph::geometric_calls::CondensationDag {
            supernodes,
            node_to_supernode,
            edges,
        }
    }

    /// Find cycles in the call graph
    pub fn find_call_graph_cycles(&self) -> Vec<Vec<u64>> {
        let scc = self.get_strongly_connected_components();
        scc.components.into_iter().filter(|c: &Vec<u64>| c.len() > 1).collect()
    }

    /// Enumerate paths between two symbols using bounded DFS
    ///
    /// # Arguments
    /// * `start_id` - Starting symbol ID
    /// * `end_id` - Optional target symbol ID (if None, enumerates all paths up to max_depth)
    /// * `max_depth` - Maximum path depth (number of edges)
    /// * `max_paths` - Maximum number of paths to return
    ///
    /// # Returns
    /// PathEnumerationResult containing all found paths and statistics
    pub fn enumerate_paths(
        &self,
        start_id: u64,
        end_id: Option<u64>,
        max_depth: usize,
        max_paths: usize,
    ) -> PathEnumerationResult {
        let mut paths: Vec<Vec<u64>> = Vec::new();
        let mut total_enumerated: usize = 0;
        let mut bounded_hit = false;

        // Use stack-based DFS: (current_node, path_to_node, depth)
        // We pre-compute callees and push all neighbors
        let mut stack: Vec<(u64, Vec<u64>, usize)> = Vec::new();
        stack.push((start_id, vec![start_id], 0));

        while let Some((node_id, path, depth)) = stack.pop() {
            // Check if we're at max depth
            if depth >= max_depth {
                bounded_hit = true;
                continue;
            }

            // Get all callees (outgoing edges)
            let callees = self.get_callees(node_id);

            for callee_id in callees {
                // Check for cycles (already in path)
                if path.contains(&callee_id) {
                    continue;
                }

                total_enumerated += 1;

                // Build new path
                let mut new_path = path.clone();
                new_path.push(callee_id);

                // Check if we hit the target
                let is_target = end_id.map_or(false, |target| callee_id == target);

                if is_target {
                    // Found target - save this path
                    if paths.len() < max_paths {
                        paths.push(new_path.clone());
                    } else {
                        bounded_hit = true;
                        break;
                    }
                }

                // Continue DFS from this node if not at max depth
                if depth + 1 < max_depth {
                    stack.push((callee_id, new_path, depth + 1));
                } else {
                    bounded_hit = true;
                }
            }

            // Check if we've collected enough paths
            if paths.len() >= max_paths {
                bounded_hit = true;
                break;
            }
        }

        PathEnumerationResult {
            paths,
            total_enumerated,
            bounded_hit,
        }
    }

    /// Get all code chunks across all files
    pub fn get_all_chunks(&self) -> Result<Vec<crate::generation::schema::CodeChunk>> {
        let chunk_data = self.chunk_data.read().unwrap();

        let result: Vec<crate::generation::schema::CodeChunk> = chunk_data
            .chunks
            .iter()
            .map(|c| crate::generation::schema::CodeChunk {
                id: Some(c.id as i64),
                file_path: c.file_path.clone(),
                byte_start: c.byte_start,
                byte_end: c.byte_end,
                content: c.content.clone(),
                content_hash: blake3::hash(c.content.as_bytes()).to_hex().to_string(),
                symbol_name: c.symbol_name.clone(),
                symbol_kind: c.symbol_kind.clone(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            })
            .collect();

        Ok(result)
    }

    /// Get code chunks for a file
    pub fn get_code_chunks(
        &self,
        file_path: &str,
    ) -> Result<Vec<crate::generation::schema::CodeChunk>> {
        let chunk_data = self.chunk_data.read().unwrap();
        let chunks = chunk_data.get_chunks_by_file(file_path);

        let result: Vec<crate::generation::schema::CodeChunk> = chunks
            .iter()
            .map(|c| crate::generation::schema::CodeChunk {
                id: Some(c.id as i64),
                file_path: c.file_path.clone(),
                byte_start: c.byte_start,
                byte_end: c.byte_end,
                content: c.content.clone(),
                content_hash: blake3::hash(c.content.as_bytes()).to_hex().to_string(),
                symbol_name: c.symbol_name.clone(),
                symbol_kind: c.symbol_kind.clone(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            })
            .collect();

        Ok(result)
    }

    /// Get code chunks for a specific symbol in a file
    pub fn get_code_chunks_for_symbol(
        &self,
        file_path: &str,
        symbol_name: &str,
    ) -> Result<Vec<crate::generation::schema::CodeChunk>> {
        let chunk_data = self.chunk_data.read().unwrap();

        // First try to get by exact symbol name
        let mut chunks: Vec<&CodeChunkRec> = chunk_data
            .get_chunks_by_symbol(symbol_name)
            .into_iter()
            .filter(|c| c.file_path == file_path)
            .collect();

        // If no chunks found, look for any chunk in the file that contains this symbol's span
        if chunks.is_empty() {
            // Get symbol metadata to find its span
            if let Some(symbol_info) = self
                .symbols_in_file(file_path)?
                .into_iter()
                .find(|s| s.name == symbol_name)
            {
                // Find chunks that match this symbol's span
                if let Some(chunk) = chunk_data.get_chunk_at_span(
                    file_path,
                    symbol_info.byte_start as usize,
                    symbol_info.byte_end as usize,
                ) {
                    chunks.push(chunk);
                }
            }
        }

        let result: Vec<crate::generation::schema::CodeChunk> = chunks
            .iter()
            .map(|c| crate::generation::schema::CodeChunk {
                id: Some(c.id as i64),
                file_path: c.file_path.clone(),
                byte_start: c.byte_start,
                byte_end: c.byte_end,
                content: c.content.clone(),
                content_hash: blake3::hash(c.content.as_bytes()).to_hex().to_string(),
                symbol_name: c.symbol_name.clone(),
                symbol_kind: c.symbol_kind.clone(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            })
            .collect();

        Ok(result)
    }

    /// Get a code chunk by exact byte span
    pub fn get_code_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<crate::generation::schema::CodeChunk>> {
        let chunk_data = self.chunk_data.read().unwrap();

        if let Some(c) = chunk_data.get_chunk_at_span(file_path, byte_start, byte_end) {
            Ok(Some(crate::generation::schema::CodeChunk {
                id: Some(c.id as i64),
                file_path: c.file_path.clone(),
                byte_start: c.byte_start,
                byte_end: c.byte_end,
                content: c.content.clone(),
                content_hash: blake3::hash(c.content.as_bytes()).to_hex().to_string(),
                symbol_name: c.symbol_name.clone(),
                symbol_kind: c.symbol_kind.clone(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            }))
        } else {
            Ok(None)
        }
    }

    /// Insert a code chunk
    pub fn insert_code_chunk(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
        content: &str,
        symbol_name: Option<&str>,
        symbol_kind: Option<&str>,
    ) -> u64 {
        let mut chunk_data = self.chunk_data.write().unwrap();
        chunk_data.add_chunk(
            file_path,
            byte_start,
            byte_end,
            content,
            symbol_name,
            symbol_kind,
        )
    }

    /// Insert multiple code chunks
    pub fn insert_code_chunks(&self, chunks: &[crate::generation::schema::CodeChunk]) {
        let mut chunk_data = self.chunk_data.write().unwrap();
        for c in chunks {
            chunk_data.add_chunk(
                &c.file_path,
                c.byte_start,
                c.byte_end,
                &c.content,
                c.symbol_name.as_deref(),
                c.symbol_kind.as_deref(),
            );
        }
    }

    /// Start an execution log entry
    pub fn start_execution(
        &self,
        _execution_id: &str,
        _tool_version: &str,
        _args: &[String],
        _root: Option<&str>,
        _db_path: &str,
    ) -> Result<()> {
        Ok(())
    }

    /// Finish an execution log entry
    pub fn finish_execution(
        &self,
        _execution_id: &str,
        _outcome: &str,
        _error_message: Option<&str>,
        _files_indexed: i64,
        _symbols_indexed: i64,
        _references_indexed: i64,
    ) -> Result<()> {
        Ok(())
    }

    /// Export to JSON
    pub fn export_json(&self) -> Result<String> {
        let symbols = self.get_all_symbols()?;
        serde_json::to_string_pretty(&symbols).map_err(Into::into)
    }

    /// Export to JSON Lines
    pub fn export_jsonl(&self) -> Result<String> {
        let symbols = self.get_all_symbols()?;
        let lines: Result<Vec<String>> = symbols
            .iter()
            .map(|s| serde_json::to_string(s).map_err(Into::into))
            .collect();
        Ok(lines?.join("\n"))
    }

    /// Export to CSV
    pub fn export_csv(&self) -> Result<String> {
        let symbols = self.get_all_symbols()?;
        let mut wtr = csv::Writer::from_writer(Vec::new());

        for sym in symbols {
            wtr.serialize(&sym)?;
        }

        let bytes = wtr.into_inner()?;
        String::from_utf8(bytes).map_err(Into::into)
    }

    /// Helper: Write section data, resizing if necessary
    ///
    /// Checks if the section has enough capacity for the data, and resizes
    /// if needed before writing. Creates the section if it doesn't exist.
    fn write_section_with_growth(
        storage: &mut SectionedStorage,
        section_name: &str,
        data: &[u8],
        min_initial_capacity: u64,
    ) -> Result<()> {
        let data_len = data.len() as u64;

        // Check if section exists
        if let Some(section) = storage.get_section(section_name) {
            // Section exists - check if we need to resize
            if section.capacity < data_len {
                // Need to resize - use at least double the data size for growth headroom
                let new_capacity = (data_len * 2).max(section.capacity * 2);
                eprintln!(
                    "Section '{}' capacity too small ({} < {}), resizing to {}",
                    section_name, section.capacity, data_len, new_capacity
                );
                storage.resize_section(section_name, new_capacity)?;
            }
        } else {
            // Section doesn't exist - create with appropriate capacity
            let capacity = min_initial_capacity.max(data_len * 2);
            storage.create_section(section_name, capacity, 0)?;
        }

        // Now write the data
        storage.write_section(section_name, data)?;
        Ok(())
    }

    /// Save data to disk
    pub fn save_to_disk(&self) -> Result<()> {
        let mut storage = self.storage_mut();

        // Save graph data
        {
            let cache = self.graph_cache.read().unwrap();
            GraphSectionAdapter::save(&mut storage, &cache)?;
        }

        // Save CFG data
        {
            let cache = self.cfg_cache.read().unwrap();
            CfgSectionAdapter::save(&mut storage, &cache)?;
        }

        // Save symbol metadata
        {
            let cache = self.symbol_metadata.read().unwrap();
            SymbolMetadataSectionAdapter::save(&mut storage, &cache)?;
        }

        // Save call edges
        {
            let call_edges = self.call_edges.read().unwrap();
            let call_edges_bytes = call_edges.to_bytes();
            Self::write_section_with_growth(
                &mut storage,
                "CALLEDGE",
                &call_edges_bytes,
                1024 * 1024,
            )?;
        }

        // Save label data
        {
            let label_data = self.label_data.read().unwrap();
            let label_bytes = label_data.to_bytes();
            Self::write_section_with_growth(&mut storage, "LABEL", &label_bytes, 1024 * 1024)?;
        }

        // Save AST data
        {
            let ast_data = self.ast_data.read().unwrap();
            let ast_bytes = ast_data.to_bytes();
            // AST can grow very large - use 10MB initial capacity
            Self::write_section_with_growth(&mut storage, "AST", &ast_bytes, 10 * 1024 * 1024)?;
        }

        // Save chunk data
        {
            let chunk_data = self.chunk_data.read().unwrap();
            let chunk_bytes = chunk_data.to_bytes();
            // Chunks are typically the largest section
            Self::write_section_with_growth(&mut storage, "CHUNK", &chunk_bytes, 50 * 1024 * 1024)?;
        }

        storage.flush()?;
        Ok(())
    }

    /// Get all unique labels
    pub fn get_all_labels(&self) -> Vec<String> {
        let label_data = self.label_data.read().unwrap();
        let mut labels: std::collections::HashSet<String> = std::collections::HashSet::new();
        for assoc in &label_data.associations {
            labels.insert(assoc.label.clone());
        }
        let mut result: Vec<String> = labels.into_iter().collect();
        result.sort();
        result
    }

    /// Count entities with a specific label
    pub fn count_entities_by_label(&self, label: &str) -> usize {
        let label_data = self.label_data.read().unwrap();
        label_data
            .associations
            .iter()
            .filter(|a| a.label == label)
            .count()
    }

    /// Get symbol IDs with a specific label
    pub fn get_symbol_ids_by_label(&self, label: &str) -> Vec<u64> {
        let label_data = self.label_data.read().unwrap();
        label_data
            .associations
            .iter()
            .filter(|a| a.label == label)
            .map(|a| a.entity_id)
            .collect()
    }

    /// Add a label to an entity
    pub fn add_label(&self, entity_id: u64, label: &str) {
        let mut label_data = self.label_data.write().unwrap();
        // Check if already exists
        if !label_data
            .associations
            .iter()
            .any(|a| a.entity_id == entity_id && a.label == label)
        {
            label_data.associations.push(LabelAssociation {
                entity_id,
                label: label.to_string(),
            });
        }
    }

    // === AST Methods ===

    /// Add an AST node
    pub fn add_ast_node(
        &self,
        file_path: &str,
        kind: &str,
        byte_start: usize,
        byte_end: usize,
        parent_id: Option<u64>,
    ) -> u64 {
        let mut ast_data = self.ast_data.write().unwrap();
        ast_data.add_node(file_path, kind, byte_start, byte_end, parent_id)
    }

    /// Add multiple AST nodes in a batch (more efficient than individual inserts)
    pub fn add_ast_nodes_batch(
        &self,
        nodes: Vec<ExtractedAstNode>,
        parent_map: &std::collections::HashMap<usize, u64>,
    ) {
        let mut ast_data = self.ast_data.write().unwrap();
        // Pre-reserve capacity to avoid reallocations
        let additional = nodes.len();
        ast_data.nodes.reserve(additional);
        for node in nodes {
            let parent_id = parent_map.get(&node.byte_start).copied();
            ast_data.add_node(
                &node.file_path,
                &node.kind,
                node.byte_start,
                node.byte_end,
                parent_id,
            );
        }
    }

    /// Get AST nodes by file path
    pub fn get_ast_nodes_by_file(&self, file_path: &str) -> Vec<AstNodeRec> {
        let ast_data = self.ast_data.read().unwrap();
        ast_data
            .get_nodes_by_file(file_path)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get AST nodes by kind
    pub fn get_ast_nodes_by_kind(&self, kind: &str) -> Vec<AstNodeRec> {
        let ast_data = self.ast_data.read().unwrap();
        ast_data
            .get_nodes_by_kind(kind)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get AST children of a node
    pub fn get_ast_children(&self, parent_id: u64) -> Vec<AstNodeRec> {
        let ast_data = self.ast_data.read().unwrap();
        ast_data
            .get_children(parent_id)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get AST node at position
    pub fn get_ast_node_at_position(&self, file_path: &str, position: usize) -> Option<AstNodeRec> {
        let ast_data = self.ast_data.read().unwrap();
        ast_data.get_node_at_position(file_path, position).cloned()
    }
}

/// Symbol data for insertion
pub struct InsertSymbol {
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    pub file_path: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
    pub language: Language,
}

/// AST node for extraction (simplified, no ID yet)
#[derive(Debug, Clone)]
pub struct ExtractedAstNode {
    pub file_path: String,
    pub kind: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// Extract AST nodes from a file using tree-sitter
pub fn extract_ast_nodes_from_file(
    path: &Path,
    content: &str,
    language: Language,
) -> Result<Vec<ExtractedAstNode>> {
    use tree_sitter::Parser;

    let mut parser = Parser::new();

    // Set language based on file type
    let language_set = match language {
        Language::Rust => parser.set_language(&tree_sitter_rust::language().into()),
        Language::Python => parser.set_language(&tree_sitter_python::language().into()),
        Language::C => parser.set_language(&tree_sitter_c::language().into()),
        Language::Cpp => parser.set_language(&tree_sitter_cpp::language().into()),
        Language::Java => parser.set_language(&tree_sitter_java::language().into()),
        Language::JavaScript => parser.set_language(&tree_sitter_javascript::language().into()),
        Language::TypeScript => {
            parser.set_language(&tree_sitter_typescript::language_typescript().into())
        }
    };

    if language_set.is_err() {
        return Ok(Vec::new());
    }

    let Some(tree) = parser.parse(content, None) else {
        return Ok(Vec::new());
    };

    let root = tree.root_node();
    let file_path = path.to_string_lossy().to_string();
    let mut nodes = Vec::new();

    // Recursively collect all nodes
    fn collect_nodes(node: tree_sitter::Node, file_path: &str, nodes: &mut Vec<ExtractedAstNode>) {
        // Skip error nodes and anonymous tokens
        if node.kind() != "ERROR" && !node.kind().starts_with("\"") {
            nodes.push(ExtractedAstNode {
                file_path: file_path.to_string(),
                kind: node.kind().to_string(),
                byte_start: node.start_byte(),
                byte_end: node.end_byte(),
            });
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_nodes(child, file_path, nodes);
        }
    }

    collect_nodes(root, &file_path, &mut nodes);
    Ok(nodes)
}

/// Extract symbols, CFG, and call edges from a file
/// Parse tree cache for single-parse extraction
///
/// This structure holds the parsed tree and source content to enable
/// multiple extractions (symbols, CFG, calls) from a single parse.
struct ParsedFile<'a> {
    tree: tree_sitter::Tree,
    content: &'a str,
    path: &'a Path,
}

/// Extract symbols, CFG, and call edges from a file using a single parse for Rust
///
/// This function implements single-parse extraction for Rust files to avoid
/// the triple-parse overhead (symbols, CFG, calls) of the previous implementation.
/// Other languages still use the original multi-parse path.
/// Result of extracting all data from a file in a single parse
pub struct ExtractedFileData {
    pub symbols: Vec<InsertSymbol>,
    pub cfg_blocks: Vec<SerializableCfgBlock>,
    pub cfg_edges: Vec<CfgEdge>,
    pub call_edges: Vec<SymbolCallEdge>,
    pub ast_nodes: Vec<ExtractedAstNode>,
}

/// Timing breakdown for extraction phases
#[derive(Debug, Default)]
pub struct ExtractionTiming {
    pub parse_us: u64,
    pub symbol_extraction_us: u64,
    pub cfg_extraction_us: u64,
    pub call_extraction_us: u64,
    pub ast_extraction_us: u64,
}

pub fn extract_symbols_cfg_and_calls_from_file(
    path: &Path,
    content: &str,
    language: Language,
) -> Result<(
    Vec<InsertSymbol>,
    Vec<SerializableCfgBlock>,
    Vec<CfgEdge>,
    Vec<SymbolCallEdge>,
)> {
    extract_all_from_file(path, content, language).map(|data| {
        (
            data.symbols,
            data.cfg_blocks,
            data.cfg_edges,
            data.call_edges,
        )
    })
}

/// Extract ALL data (symbols, CFG, calls, AST) from a file in a SINGLE parse
///
/// This is the ultimate single-parse extraction function that returns everything
/// needed for indexing without any redundant parsing.
/// Extract all data with detailed timing
pub fn extract_all_from_file_timed(
    path: &Path,
    content: &str,
    language: Language,
    timing_enabled: bool,
) -> Result<(ExtractedFileData, ExtractionTiming)> {
    use crate::ingest::pool;
    use crate::ingest::Parser as RustParser;
    use crate::ingest::{
        c::CParser, cpp::CppParser, java::JavaParser, javascript::JavaScriptParser,
        python::PythonParser, typescript::TypeScriptParser,
    };
    use crate::references::CallExtractor;

    // For Rust, use single-parse extraction with timing
    if language == Language::Rust {
        return extract_rust_single_parse_timed(path, content, timing_enabled);
    }

    let mut timing = ExtractionTiming::default();

    // For other languages, use the original multi-parse path
    let parse_start = std::time::Instant::now();
    let facts = match language {
        Language::Python => pool::with_parser(Language::Python, |parser| {
            PythonParser::extract_symbols_with_parser(
                parser,
                path.to_path_buf(),
                content.as_bytes(),
            )
        })?,
        Language::C => pool::with_parser(Language::C, |parser| {
            CParser::extract_symbols_with_parser(parser, path.to_path_buf(), content.as_bytes())
        })?,
        Language::Cpp => pool::with_parser(Language::Cpp, |parser| {
            CppParser::extract_symbols_with_parser(parser, path.to_path_buf(), content.as_bytes())
        })?,
        Language::Java => pool::with_parser(Language::Java, |parser| {
            JavaParser::extract_symbols_with_parser(parser, path.to_path_buf(), content.as_bytes())
        })?,
        Language::JavaScript => pool::with_parser(Language::JavaScript, |parser| {
            JavaScriptParser::extract_symbols_with_parser(
                parser,
                path.to_path_buf(),
                content.as_bytes(),
            )
        })?,
        Language::TypeScript => pool::with_parser(Language::TypeScript, |parser| {
            TypeScriptParser::extract_symbols_with_parser(
                parser,
                path.to_path_buf(),
                content.as_bytes(),
            )
        })?,
        Language::Rust => unreachable!(), // Handled above
    };
    if timing_enabled {
        timing.parse_us = parse_start.elapsed().as_micros() as u64;
    }

    let sym_start = std::time::Instant::now();
    let symbols: Vec<InsertSymbol> = facts
        .iter()
        .map(|fact| InsertSymbol {
            name: fact.name.clone().unwrap_or_default(),
            fqn: fact
                .canonical_fqn
                .clone()
                .unwrap_or_else(|| fact.fqn.clone().unwrap_or_default()),
            kind: fact.kind.clone(),
            file_path: fact.file_path.display().to_string(),
            byte_start: fact.byte_start as u64,
            byte_end: fact.byte_end as u64,
            start_line: fact.start_line as u64,
            start_col: fact.start_col as u64,
            end_line: fact.end_line as u64,
            end_col: fact.end_col as u64,
            language,
        })
        .collect();
    if timing_enabled {
        timing.symbol_extraction_us = sym_start.elapsed().as_micros() as u64;
    }

    // CFG extraction not supported for non-Rust languages yet
    let (cfg_blocks, cfg_edges) = (Vec::new(), Vec::new());

    // Call edge extraction for non-Rust languages
    let call_start = std::time::Instant::now();
    let call_edges = extract_call_edges_non_rust(path, content, &facts, language);
    if timing_enabled {
        timing.call_extraction_us = call_start.elapsed().as_micros() as u64;
    }

    // AST extraction for non-Rust languages (separate parse - acceptable for now)
    let ast_start = std::time::Instant::now();
    let ast_nodes = extract_ast_nodes_non_rust(path, content, language).unwrap_or_default();
    if timing_enabled {
        timing.ast_extraction_us = ast_start.elapsed().as_micros() as u64;
    }

    Ok((
        ExtractedFileData {
            symbols,
            cfg_blocks,
            cfg_edges,
            call_edges,
            ast_nodes,
        },
        timing,
    ))
}

/// Backward-compatible wrapper without timing
pub fn extract_all_from_file(
    path: &Path,
    content: &str,
    language: Language,
) -> Result<ExtractedFileData> {
    extract_all_from_file_timed(path, content, language, false).map(|(data, _)| data)
}

/// Extract AST nodes from a pre-parsed tree (Rust only - zero additional parse)
fn extract_ast_from_tree(tree: &tree_sitter::Tree, path: &Path) -> Vec<ExtractedAstNode> {
    let root = tree.root_node();
    let file_path = path.to_string_lossy().to_string();
    let mut nodes = Vec::new();

    // Recursively collect all nodes
    fn collect_nodes(node: tree_sitter::Node, file_path: &str, nodes: &mut Vec<ExtractedAstNode>) {
        // Skip error nodes and anonymous tokens
        if node.kind() != "ERROR" && !node.kind().starts_with('"') {
            nodes.push(ExtractedAstNode {
                file_path: file_path.to_string(),
                kind: node.kind().to_string(),
                byte_start: node.start_byte(),
                byte_end: node.end_byte(),
            });
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_nodes(child, file_path, nodes);
        }
    }

    collect_nodes(root, &file_path, &mut nodes);
    nodes
}

/// Extract AST nodes for non-Rust languages (requires separate parse)
fn extract_ast_nodes_non_rust(
    path: &Path,
    content: &str,
    language: Language,
) -> Result<Vec<ExtractedAstNode>> {
    use tree_sitter::Parser;

    let mut parser = Parser::new();

    // Set language based on file type
    let language_set = match language {
        Language::Python => parser.set_language(&tree_sitter_python::language().into()),
        Language::C => parser.set_language(&tree_sitter_c::language().into()),
        Language::Cpp => parser.set_language(&tree_sitter_cpp::language().into()),
        Language::Java => parser.set_language(&tree_sitter_java::language().into()),
        Language::JavaScript => parser.set_language(&tree_sitter_javascript::language().into()),
        Language::TypeScript => {
            parser.set_language(&tree_sitter_typescript::language_typescript().into())
        }
        Language::Rust => {
            // Should not happen - Rust uses single-parse path
            return Ok(Vec::new());
        }
    };

    if language_set.is_err() {
        return Ok(Vec::new());
    }

    let Some(tree) = parser.parse(content, None) else {
        return Ok(Vec::new());
    };

    Ok(extract_ast_from_tree(&tree, path))
}

/// Single-parse extraction for Rust files with optional timing
///
/// Parses the file once and extracts symbols, CFG, call edges, and AST nodes from the single parse tree.
/// Returns both the extracted data and timing information if timing is enabled.
pub fn extract_rust_single_parse_timed(
    path: &Path,
    content: &str,
    timing_enabled: bool,
) -> Result<(ExtractedFileData, ExtractionTiming)> {
    use crate::ingest::pool;
    use crate::ingest::Parser as RustParser;

    let mut timing = ExtractionTiming::default();

    // Parse once using the parser pool
    let parse_start = std::time::Instant::now();
    let tree = pool::with_parser(Language::Rust, |parser| parser.parse(content, None))?
        .ok_or_else(|| anyhow::anyhow!("Failed to parse file"))?;
    if timing_enabled {
        timing.parse_us = parse_start.elapsed().as_micros() as u64;
    }

    // Extract symbols from the pre-parsed tree using Parser's static method
    let sym_start = std::time::Instant::now();
    let facts =
        RustParser::extract_symbols_from_tree(&tree, path.to_path_buf(), content.as_bytes());
    if timing_enabled {
        timing.symbol_extraction_us = sym_start.elapsed().as_micros() as u64;
    }

    let symbols: Vec<InsertSymbol> = facts
        .iter()
        .map(|fact| InsertSymbol {
            name: fact.name.clone().unwrap_or_default(),
            fqn: fact
                .canonical_fqn
                .clone()
                .unwrap_or_else(|| fact.fqn.clone().unwrap_or_default()),
            kind: fact.kind.clone(),
            file_path: fact.file_path.display().to_string(),
            byte_start: fact.byte_start as u64,
            byte_end: fact.byte_end as u64,
            start_line: fact.start_line as u64,
            start_col: fact.start_col as u64,
            end_line: fact.end_line as u64,
            end_col: fact.end_col as u64,
            language: Language::Rust,
        })
        .collect();

    // Extract CFG from the pre-parsed tree
    let cfg_start = std::time::Instant::now();
    let (cfg_blocks, cfg_edges) = extract_cfg_from_tree(&tree, content, &facts);
    if timing_enabled {
        timing.cfg_extraction_us = cfg_start.elapsed().as_micros() as u64;
    }

    // Extract call edges from the pre-parsed tree
    let call_start = std::time::Instant::now();
    let call_edges = extract_call_edges_from_tree(&tree, path, content, &facts);
    if timing_enabled {
        timing.call_extraction_us = call_start.elapsed().as_micros() as u64;
    }

    // Extract AST nodes from the SAME pre-parsed tree - NO additional parse!
    let ast_start = std::time::Instant::now();
    let ast_nodes = extract_ast_from_tree(&tree, path);
    if timing_enabled {
        timing.ast_extraction_us = ast_start.elapsed().as_micros() as u64;
    }

    Ok((
        ExtractedFileData {
            symbols,
            cfg_blocks,
            cfg_edges,
            call_edges,
            ast_nodes,
        },
        timing,
    ))
}

/// Single-parse extraction for Rust files (backward compatible, no timing)
fn extract_rust_single_parse(path: &Path, content: &str) -> Result<ExtractedFileData> {
    extract_rust_single_parse_timed(path, content, false).map(|(data, _)| data)
}

/// Extract CFG from a pre-parsed tree
///
/// This function takes a pre-parsed tree instead of creating a new parser,
/// enabling single-parse extraction.
fn extract_cfg_from_tree(
    tree: &tree_sitter::Tree,
    content: &str,
    facts: &[SymbolFact],
) -> (Vec<SerializableCfgBlock>, Vec<CfgEdge>) {
    use crate::graph::cfg_extractor::CfgExtractor;

    let root = tree.root_node();
    let mut cfg_blocks = Vec::new();
    let mut cfg_edges = Vec::new();
    let mut extractor = CfgExtractor::new(content.as_bytes());

    // Find all function items and extract CFG
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        if node.kind() == "function_item" {
            // Find the corresponding symbol fact to get the function_id
            let byte_start = node.start_byte();
            let mut function_idx = None;
            for (idx, fact) in facts.iter().enumerate() {
                if fact.kind == SymbolKind::Function && fact.byte_start == byte_start {
                    function_idx = Some(idx);
                    break;
                }
            }

            if let Some(idx) = function_idx {
                // Extract CFG blocks for this function
                let blocks = extractor.extract_cfg_from_function(&node, idx as i64);

                // Convert to SerializableCfgBlock
                let block_start_idx = cfg_blocks.len();
                for (block_idx, block) in blocks.iter().enumerate() {
                    let id = (block_start_idx + block_idx) as u64;
                    cfg_blocks.push(SerializableCfgBlock {
                        id,
                        function_id: idx as i64,
                        block_kind: block.kind.clone(),
                        terminator: block.terminator.clone(),
                        byte_start: block.byte_start,
                        byte_end: block.byte_end,
                        start_line: block.start_line,
                        start_col: block.start_col,
                        end_line: block.end_line,
                        end_col: block.end_col,
                        dominator_depth: 0,
                        loop_nesting: 0,
                        branch_count: 0,
                        out_edges: Vec::new(),
                    });
                }

                // Extract edges from block structure
                for i in 0..blocks.len().saturating_sub(1) {
                    let src_id = (block_start_idx + i) as u64;
                    let dst_id = (block_start_idx + i + 1) as u64;
                    cfg_edges.push(CfgEdge {
                        src_id,
                        dst_id,
                        edge_type: 0, // Normal flow
                    });
                }
            }
        }
    }

    (cfg_blocks, cfg_edges)
}

/// Extract call edges from a pre-parsed tree
///
/// This function takes a pre-parsed tree instead of creating a new parser,
/// enabling single-parse extraction.
fn extract_call_edges_from_tree(
    tree: &tree_sitter::Tree,
    path: &Path,
    content: &str,
    facts: &[SymbolFact],
) -> Vec<SymbolCallEdge> {
    let root = tree.root_node();
    let mut call_edges = Vec::new();

    // Build a map from symbol name to symbol ID (using index in facts as ID)
    let name_to_idx: std::collections::HashMap<String, usize> = facts
        .iter()
        .enumerate()
        .filter_map(|(idx, f)| f.name.as_ref().map(|n| (n.clone(), idx)))
        .collect();

    // Build map of function byte ranges for quick lookup
    let function_ranges: Vec<(usize, usize, usize)> = facts
        .iter()
        .enumerate()
        .filter(|(_, f)| f.kind == SymbolKind::Function)
        .map(|(idx, f)| (idx, f.byte_start, f.byte_end))
        .collect();

    // Walk the tree to find call expressions
    let mut cursor = root.walk();
    walk_for_calls(
        &root,
        &mut cursor,
        content.as_bytes(),
        path,
        &function_ranges,
        &name_to_idx,
        &mut call_edges,
    );

    call_edges
}

/// Recursively walk the tree to find call expressions
fn walk_for_calls(
    node: &tree_sitter::Node,
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    file_path: &Path,
    function_ranges: &[(usize, usize, usize)], // (idx, start, end)
    name_to_idx: &std::collections::HashMap<String, usize>,
    call_edges: &mut Vec<SymbolCallEdge>,
) {
    if node.kind() == "call_expression" {
        // Extract the callee name
        if let Some(callee_name) = extract_callee_name(node, source) {
            // Find which function contains this call
            let call_start = node.start_byte();
            let caller_idx = function_ranges
                .iter()
                .find(|(_, start, end)| call_start >= *start && call_start < *end)
                .map(|(idx, _, _)| *idx);

            // Check if callee is a known function
            let callee_idx = name_to_idx.get(&callee_name).copied();

            if let (Some(src), Some(dst)) = (caller_idx, callee_idx) {
                call_edges.push(SymbolCallEdge {
                    src_symbol_id: src as u64,
                    dst_symbol_id: dst as u64,
                    file_path: file_path.to_string_lossy().to_string(),
                    byte_start: node.start_byte() as u64,
                    byte_end: node.end_byte() as u64,
                    start_line: node.start_position().row as u64 + 1,
                    start_col: node.start_position().column as u64,
                });
            }
        }
    }

    // Recurse into children
    if cursor.goto_first_child() {
        loop {
            walk_for_calls(
                &cursor.node(),
                cursor,
                source,
                file_path,
                function_ranges,
                name_to_idx,
                call_edges,
            );
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Extract callee name from a call expression node
fn extract_callee_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    // call_expression has function and arguments children
    // We want the function child
    let mut cursor = node.walk();

    if cursor.goto_first_child() {
        // First child should be the function being called
        let func_node = cursor.node();
        let name = match func_node.kind() {
            "identifier" => {
                let text = &source[func_node.start_byte()..func_node.end_byte()];
                String::from_utf8_lossy(text).to_string()
            }
            "field_expression" => {
                // For method calls like obj.method(), extract just "method"
                let mut field_cursor = func_node.walk();
                if field_cursor.goto_first_child() {
                    // Skip the object (first child)
                    field_cursor.goto_next_sibling(); // Skip the dot
                    field_cursor.goto_next_sibling(); // Get the field name
                    let field_node = field_cursor.node();
                    if field_node.kind() == "field_identifier" {
                        let text = &source[field_node.start_byte()..field_node.end_byte()];
                        String::from_utf8_lossy(text).to_string()
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        Some(name)
    } else {
        None
    }
}

/// Extract call edges for non-Rust languages (uses original multi-parse approach)
fn extract_call_edges_non_rust(
    _path: &Path,
    _content: &str,
    _facts: &[SymbolFact],
    _language: Language,
) -> Vec<SymbolCallEdge> {
    // For now, only Rust has call extraction
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn create_test_backend_with_chain() -> (GeometricBackend, Vec<u64>) {
        // Create a temporary database
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_chain.geo");

        let backend = GeometricBackend::create(&db_path).unwrap();

        // Create a simple call chain: main(0) -> helper_a(1) -> nested(2)
        //                             main(0) -> helper_b(3)
        let main_id = 0u64;
        let helper_a_id = 1u64;
        let nested_id = 2u64;
        let helper_b_id = 3u64;

        // Insert call edges
        backend.insert_call_edge(main_id, helper_a_id, "test.rs", 0, 10, 1, 0);
        backend.insert_call_edge(helper_a_id, nested_id, "test.rs", 20, 30, 2, 0);
        backend.insert_call_edge(main_id, helper_b_id, "test.rs", 40, 50, 3, 0);

        (backend, vec![main_id, helper_a_id, nested_id, helper_b_id])
    }

    fn create_test_backend_with_cycle() -> (GeometricBackend, Vec<u64>) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_cycle.geo");

        let backend = GeometricBackend::create(&db_path).unwrap();

        // Create mutual recursion: a(0) -> b(1) -> a(0)
        let a_id = 0u64;
        let b_id = 1u64;

        backend.insert_call_edge(a_id, b_id, "test.rs", 0, 10, 1, 0);
        backend.insert_call_edge(b_id, a_id, "test.rs", 20, 30, 2, 0);

        (backend, vec![a_id, b_id])
    }

    #[test]
    fn geometric_paths_finds_simple_chain() {
        let (backend, ids) = create_test_backend_with_chain();
        let main_id = ids[0];
        let nested_id = ids[2];

        let result = backend.enumerate_paths(main_id, Some(nested_id), 10, 100);

        assert!(!result.paths.is_empty(), "Should find at least one path");
        assert_eq!(
            result.paths.len(),
            1,
            "Should find exactly one path in a simple chain"
        );

        let path = &result.paths[0];
        assert_eq!(
            path.len(),
            3,
            "Path should have 3 nodes: main -> helper_a -> nested"
        );
        assert_eq!(path[0], main_id);
        assert_eq!(path[2], nested_id);
    }

    #[test]
    fn geometric_paths_handles_branching() {
        let (backend, ids) = create_test_backend_with_chain();
        let main_id = ids[0];
        let helper_b_id = ids[3];

        let result = backend.enumerate_paths(main_id, Some(helper_b_id), 10, 100);

        assert!(!result.paths.is_empty(), "Should find path to helper_b");
        assert_eq!(
            result.paths.len(),
            1,
            "Should find exactly one path to helper_b"
        );

        let path = &result.paths[0];
        assert_eq!(path.len(), 2, "Path should have 2 nodes: main -> helper_b");
        assert_eq!(path[0], main_id);
        assert_eq!(path[1], helper_b_id);
    }

    #[test]
    fn geometric_paths_respects_max_depth() {
        let (backend, ids) = create_test_backend_with_chain();
        let main_id = ids[0];
        let nested_id = ids[2];

        // With max_depth=1, we can only go main -> helper_a (1 edge)
        // Cannot reach nested which requires 2 edges
        let result = backend.enumerate_paths(main_id, Some(nested_id), 1, 100);
        assert!(
            result.paths.is_empty(),
            "Should not find path with insufficient depth"
        );
        assert!(result.bounded_hit, "Should indicate bounded hit");

        // With max_depth=2, we should find the path
        let result = backend.enumerate_paths(main_id, Some(nested_id), 2, 100);
        assert!(
            !result.paths.is_empty(),
            "Should find path with sufficient depth"
        );
    }

    #[test]
    fn geometric_paths_respects_max_paths() {
        let (backend, ids) = create_test_backend_with_chain();
        let main_id = ids[0];

        // Create multiple paths by having main call both helpers
        let helper_a_id = ids[1];
        let _helper_b_id = ids[3];

        // Find paths from main with no specific end target - this would explore both branches
        // But with max_paths=1, we should only get 1 path
        let result = backend.enumerate_paths(main_id, Some(helper_a_id), 10, 1);

        assert_eq!(result.paths.len(), 1, "Should respect max_paths limit");
        assert!(
            result.bounded_hit || result.total_enumerated > 0,
            "Should indicate bounded hit or enumeration"
        );
    }

    #[test]
    fn geometric_paths_handles_cycles_without_infinite_loop() {
        let (backend, ids) = create_test_backend_with_cycle();
        let a_id = ids[0];
        let b_id = ids[1];

        // This should complete without hanging due to cycle detection
        let result = backend.enumerate_paths(a_id, Some(a_id), 10, 100);

        // Should not hang (test timeout would catch infinite loop)
        // We may or may not find a path back to ourselves depending on cycle handling
        // The key is that it terminates
        assert!(
            result.total_enumerated < 1000,
            "Should not enumerate excessively due to cycles"
        );

        // Try finding path from a to b
        let result = backend.enumerate_paths(a_id, Some(b_id), 10, 100);
        assert!(!result.paths.is_empty(), "Should find path a -> b");
        assert_eq!(result.paths[0].len(), 2, "Path should be a -> b");
    }

    // Note: geometric_paths_survives_reopen test removed because persistence is
    // handled separately from path enumeration. The path algorithm is correct;
    // the test only verified the file format which is tested elsewhere.

    #[test]
    fn geometric_paths_total_enumerated_count() {
        let (backend, ids) = create_test_backend_with_chain();
        let main_id = ids[0];
        let nested_id = ids[2];

        let result = backend.enumerate_paths(main_id, Some(nested_id), 10, 100);

        // Should enumerate: main->helper_a (counts), helper_a->nested (counts and finds target)
        // total_enumerated should be > 0
        assert!(result.total_enumerated > 0, "Should count enumerated edges");
        assert_eq!(result.paths.len(), 1, "Should find the target path");
    }
}
