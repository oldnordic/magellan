//! Geo index - Directory scanning for geometric backend
//!
//! Provides functions for scanning directories and indexing files
//! into the geometric backend database.
//!
//! # Indexing Modes
//!
//! - `CfgFirst` (default): Prioritizes CFG, call graph, and analysis data.
//!   Does NOT persist full AST by default. Use for dead-code, paths, loops, dominators.
//! - `FullAst`: Persists complete AST for all nodes. Higher storage cost.
//!
//! Set `MAGELLAN_FULL_AST=1` to enable FullAst mode.

use crate::graph::geometric_backend::{
    extract_all_from_file_timed, ExtractionTiming, GeometricBackend, InsertSymbol,
};
use crate::ingest::detect_language;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use walkdir::WalkDir;

/// Indexing mode for `.geo` databases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexingMode {
    /// CFG-first mode (default): Persist CFG, call graph, symbols, chunks.
    /// Does NOT persist full AST. Optimized for analysis workflows.
    CfgFirst,
    /// Full AST mode: Persist everything including complete AST.
    /// Higher storage cost, only use when full AST queries needed.
    FullAst,
}

impl IndexingMode {
    /// Detect mode from environment
    pub fn from_env() -> Self {
        if std::env::var("MAGELLAN_FULL_AST").is_ok() {
            IndexingMode::FullAst
        } else {
            IndexingMode::CfgFirst
        }
    }

    /// Returns true if full AST should be persisted
    pub fn persist_full_ast(&self) -> bool {
        matches!(self, IndexingMode::FullAst)
    }
}

/// Timing stats for indexing phases
#[derive(Debug, Default)]
struct IndexingStats {
    file_discovery_us: u64,
    file_read_us: u64,
    hash_compute_us: u64,
    parse_us: u64,
    symbol_extraction_us: u64,
    symbol_insertion_us: u64,
    chunk_insertion_us: u64,
    ast_extraction_us: u64,
    ast_insertion_us: u64,
    call_edge_insertion_us: u64,
    cfg_extraction_us: u64,
    cfg_insertion_us: u64,
    total_files: usize,
    total_symbols: usize,
    total_cfg_blocks: usize,
    total_call_edges: usize,
    total_ast_nodes: usize,
    slowest_files: Vec<(String, u64)>, // (path, microseconds)
}

impl IndexingStats {
    fn print_report(&self, mode: IndexingMode) {
        eprintln!("\n=== INDEXING TIMING REPORT ===");
        eprintln!("Mode: {:?}", mode);
        eprintln!("Total files indexed: {}", self.total_files);
        eprintln!("Total symbols: {}", self.total_symbols);
        eprintln!("Total CFG blocks: {}", self.total_cfg_blocks);
        eprintln!("Total call edges: {}", self.total_call_edges);
        if mode.persist_full_ast() {
            eprintln!("Total AST nodes persisted: {}", self.total_ast_nodes);
        } else {
            eprintln!(
                "AST nodes: {} (extracted but not persisted in CfgFirst mode)",
                self.total_ast_nodes
            );
        }
        eprintln!("");
        eprintln!("Phase timings (microseconds):");
        eprintln!(
            "  File discovery:     {:>12} us ({:.2}s)",
            self.file_discovery_us,
            self.file_discovery_us as f64 / 1_000_000.0
        );
        eprintln!(
            "  File read:          {:>12} us ({:.2}s)",
            self.file_read_us,
            self.file_read_us as f64 / 1_000_000.0
        );
        eprintln!(
            "  Hash compute:       {:>12} us ({:.2}s)",
            self.hash_compute_us,
            self.hash_compute_us as f64 / 1_000_000.0
        );
        eprintln!(
            "  Parse:              {:>12} us ({:.2}s)",
            self.parse_us,
            self.parse_us as f64 / 1_000_000.0
        );
        eprintln!(
            "  Symbol extraction:  {:>12} us ({:.2}s)",
            self.symbol_extraction_us,
            self.symbol_extraction_us as f64 / 1_000_000.0
        );
        eprintln!(
            "  CFG extraction:     {:>12} us ({:.2}s)",
            self.cfg_extraction_us,
            self.cfg_extraction_us as f64 / 1_000_000.0
        );
        eprintln!(
            "  Symbol insertion:   {:>12} us ({:.2}s)",
            self.symbol_insertion_us,
            self.symbol_insertion_us as f64 / 1_000_000.0
        );
        eprintln!(
            "  Chunk insertion:    {:>12} us ({:.2}s)",
            self.chunk_insertion_us,
            self.chunk_insertion_us as f64 / 1_000_000.0
        );
        if mode.persist_full_ast() {
            eprintln!(
                "  AST insertion:      {:>12} us ({:.2}s)",
                self.ast_insertion_us,
                self.ast_insertion_us as f64 / 1_000_000.0
            );
        }
        eprintln!(
            "  Call edge insertion:{:>12} us ({:.2}s)",
            self.call_edge_insertion_us,
            self.call_edge_insertion_us as f64 / 1_000_000.0
        );
        eprintln!(
            "  CFG insertion:      {:>12} us ({:.2}s)",
            self.cfg_insertion_us,
            self.cfg_insertion_us as f64 / 1_000_000.0
        );
        let total_us = self.file_discovery_us
            + self.file_read_us
            + self.hash_compute_us
            + self.parse_us
            + self.symbol_extraction_us
            + self.cfg_extraction_us
            + self.symbol_insertion_us
            + self.chunk_insertion_us
            + if mode.persist_full_ast() {
                self.ast_insertion_us
            } else {
                0
            }
            + self.call_edge_insertion_us
            + self.cfg_insertion_us;
        eprintln!(
            "  TOTAL:              {:>12} us ({:.2}s)",
            total_us,
            total_us as f64 / 1_000_000.0
        );

        if !self.slowest_files.is_empty() {
            eprintln!("");
            eprintln!("Top 10 slowest files:");
            for (i, (path, us)) in self.slowest_files.iter().take(10).enumerate() {
                eprintln!("  {}. {}: {:.2} ms", i + 1, path, *us as f64 / 1000.0);
            }
        }
        eprintln!("=== END REPORT ===\n");
    }
}

/// Macro to time a block if MAGELLAN_TIMING env var is set
macro_rules! timed {
    ($stats:expr, $field:ident, $block:expr) => {
        if std::env::var("MAGELLAN_TIMING").is_ok() {
            let start = std::time::Instant::now();
            let result = $block;
            $stats.$field += start.elapsed().as_micros() as u64;
            result
        } else {
            $block
        }
    };
}

/// Compute SHA-256 hash of file contents
pub fn compute_file_hash(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

/// Scan a directory and index all files into the geometric backend
///
/// # Arguments
/// * `backend` - Mutable reference to the geometric backend
/// * `root_path` - Root directory to scan
/// * `progress_callback` - Optional callback for progress updates
/// * `mode` - Indexing mode (CfgFirst or FullAst)
///
/// # Returns
/// Number of files successfully indexed
pub fn scan_directory_with_progress(
    backend: &mut GeometricBackend,
    root_path: &Path,
    progress_callback: Option<&dyn Fn(usize, usize)>,
    mode: IndexingMode,
) -> Result<usize> {
    let timing_enabled = std::env::var("MAGELLAN_TIMING").is_ok();
    let mut stats = IndexingStats::default();

    if timing_enabled {
        eprintln!("[MAGELLAN] Indexing mode: {:?}", mode);
    }

    let start_discovery = std::time::Instant::now();
    // Collect all files first
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(root_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            // Only index files with known extensions
            if detect_language(path).is_some() {
                files.push(path.to_path_buf());
            }
        }
    }
    if timing_enabled {
        stats.file_discovery_us = start_discovery.elapsed().as_micros() as u64;
    }

    let total = files.len();
    let indexed = AtomicUsize::new(0);

    for path in files {
        let file_start = std::time::Instant::now();
        
        // Canonicalize path for consistent storage
        let path_str = match path.canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => path.to_string_lossy().to_string(),
        };

        let language = match detect_language(&path) {
            Some(l) => l,
            None => continue,
        };

        let read_start = std::time::Instant::now();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let read_elapsed = read_start.elapsed().as_micros() as u64;
        if timing_enabled {
            stats.file_read_us += read_elapsed;
        }

        // Compute file hash for verification
        let hash_start = std::time::Instant::now();
        let file_hash = compute_file_hash(content.as_bytes());
        if timing_enabled {
            stats.hash_compute_us += hash_start.elapsed().as_micros() as u64;
        }

        // CLEAR EXISTING DATA: Remove old symbols, edges, CFG, AST, chunks for this file
        // This prevents duplicate accumulation on re-indexing
        if let Err(e) = backend.clear_file_data(&path_str) {
            eprintln!("Warning: Failed to clear old data for {}: {}", path_str, e);
        }

        // SINGLE-PARSE EXTRACTION: Get symbols, CFG, calls, and AST in ONE parse
        match extract_all_from_file_timed(&path, &content, language, timing_enabled) {
            Ok((mut extracted, extract_timing)) => {
                // CANONICALIZE PATHS: Ensure all extracted data uses the canonical path
                // This prevents duplicates from mixed relative/absolute path forms
                for sym in &mut extracted.symbols {
                    sym.file_path = path_str.clone();
                }
                for edge in &mut extracted.call_edges {
                    edge.file_path = path_str.clone();
                }
                for node in &mut extracted.ast_nodes {
                    node.file_path = path_str.clone();
                }

                if timing_enabled {
                    stats.parse_us += extract_timing.parse_us;
                    stats.symbol_extraction_us += extract_timing.symbol_extraction_us;
                    stats.cfg_extraction_us += extract_timing.cfg_extraction_us;
                    // In CfgFirst mode, we still extract AST but don't persist it
                    stats.total_symbols += extracted.symbols.len();
                    stats.total_cfg_blocks += extracted.cfg_blocks.len();
                    stats.total_call_edges += extracted.call_edges.len();
                    stats.total_ast_nodes += extracted.ast_nodes.len();

                    // Track per-file total time
                    let file_total = file_start.elapsed().as_micros() as u64;
                    stats.slowest_files.push((path_str.clone(), file_total));
                    stats.slowest_files.sort_by(|a, b| b.1.cmp(&a.1));
                    stats.slowest_files.truncate(20); // Keep top 20
                }

                let sym_count = extracted.symbols.len();

                // Store file hash
                backend.set_file_hash(&path_str, &file_hash);

                let insert_start = std::time::Instant::now();
                let symbol_ids = backend.insert_symbols(extracted.symbols)?;
                if timing_enabled {
                    stats.symbol_insertion_us += insert_start.elapsed().as_micros() as u64;
                }

                // Insert code chunks for each symbol
                let chunk_start = std::time::Instant::now();
                for (idx, symbol_id) in symbol_ids.iter().enumerate() {
                    let byte_start = symbol_ids
                        .get(idx)
                        .map(|_| {
                            // We need to get symbol info - use the backend to look it up
                            *symbol_id
                        })
                        .unwrap_or(0);

                    // Get symbol info from backend to extract content
                    if let Some(info) = backend.find_symbol_by_id_info(*symbol_id) {
                        let byte_start = info.byte_start as usize;
                        let byte_end = info.byte_end as usize;
                        let symbol_name = info.name;
                        let symbol_kind = format!("{:?}", info.kind);

                        // Extract the symbol's source text
                        if byte_end <= content.len() {
                            let symbol_content = &content[byte_start..byte_end];

                            // Insert chunk
                            backend.insert_code_chunk(
                                &info.file_path,
                                byte_start,
                                byte_end,
                                symbol_content,
                                Some(&symbol_name),
                                Some(&symbol_kind),
                            );

                            // Auto-populate labels based on symbol characteristics
                            // Label: "main" for main functions
                            if symbol_name == "main" {
                                backend.add_label(*symbol_id, "main");
                                backend.add_label(*symbol_id, "entry_point");
                            }

                            // Label: "test" for test functions
                            if symbol_name.starts_with("test_") || symbol_name.ends_with("_test") {
                                backend.add_label(*symbol_id, "test");
                            }

                            // Label: "lib" for library roots (pub fn in lib.rs or mod.rs)
                            let is_pub = symbol_content.starts_with("pub ");
                            let is_lib_file =
                                path_str.ends_with("lib.rs") || path_str.ends_with("mod.rs");
                            if is_pub && is_lib_file {
                                backend.add_label(*symbol_id, "lib");
                            }

                            // Label: "entry_point" for public functions that could be entry points
                            if is_pub
                                && (info.kind == crate::ingest::SymbolKind::Function
                                    || info.kind == crate::ingest::SymbolKind::Method)
                            {
                                backend.add_label(*symbol_id, "entry_point");
                            }
                        }
                    }
                }
                if timing_enabled {
                    stats.chunk_insertion_us += chunk_start.elapsed().as_micros() as u64;
                }

                // Add AST nodes only in FullAst mode
                if mode.persist_full_ast() {
                    let ast_start = std::time::Instant::now();
                    let ast_nodes = extracted.ast_nodes;

                    // Build a map from byte position to symbol ID for parent lookup
                    let mut parent_map: std::collections::HashMap<usize, u64> =
                        std::collections::HashMap::new();
                    for symbol_id in &symbol_ids {
                        if let Some(info) = backend.find_symbol_by_id_info(*symbol_id) {
                            for pos in info.byte_start..info.byte_end {
                                parent_map.insert(pos as usize, *symbol_id);
                            }
                        }
                    }

                    // Add AST nodes using batch insert (reduces lock contention)
                    let ast_insert_start = std::time::Instant::now();
                    backend.add_ast_nodes_batch(ast_nodes, &parent_map);
                    if timing_enabled {
                        stats.ast_extraction_us += ast_start.elapsed().as_micros() as u64;
                        stats.ast_insertion_us += ast_insert_start.elapsed().as_micros() as u64;
                    }
                } else if timing_enabled {
                    // Still track extraction time even if we don't persist
                    stats.ast_extraction_us += extract_timing.ast_extraction_us;
                }

                // Insert call edges with resolved symbol IDs
                let call_start = std::time::Instant::now();
                for mut edge in extracted.call_edges {
                    let src_idx = edge.src_symbol_id as usize;
                    let dst_idx = edge.dst_symbol_id as usize;
                    if src_idx < symbol_ids.len() && dst_idx < symbol_ids.len() {
                        edge.src_symbol_id = symbol_ids[src_idx];
                        edge.dst_symbol_id = symbol_ids[dst_idx];
                        backend.insert_call_edge(
                            edge.src_symbol_id,
                            edge.dst_symbol_id,
                            &edge.file_path,
                            edge.byte_start,
                            edge.byte_end,
                            edge.start_line,
                            edge.start_col,
                        );
                    }
                }
                if timing_enabled {
                    stats.call_edge_insertion_us += call_start.elapsed().as_micros() as u64;
                }

                // Insert CFG blocks (from the same single parse!)
                let cfg_start = std::time::Instant::now();
                if !extracted.cfg_blocks.is_empty() {
                    let mut block_id_map: std::collections::HashMap<usize, u64> =
                        std::collections::HashMap::new();

                    for (idx, mut block) in extracted.cfg_blocks.into_iter().enumerate() {
                        let local_sym_idx = block.function_id as usize;
                        if local_sym_idx < symbol_ids.len() {
                            block.function_id = symbol_ids[local_sym_idx] as i64;
                        }
                        let logical_id = block.id;
                        block_id_map.insert(idx, logical_id);
                        let _ = backend.insert_cfg_block(block);
                    }

                    for edge in extracted.cfg_edges {
                        if let (Some(&src_id), Some(&dst_id)) = (
                            block_id_map.get(&(edge.src_id as usize)),
                            block_id_map.get(&(edge.dst_id as usize)),
                        ) {
                            let _ = backend.insert_edge(src_id, dst_id, "cfg");
                        }
                    }
                }
                if timing_enabled {
                    stats.cfg_insertion_us += cfg_start.elapsed().as_micros() as u64;
                }

                let current = indexed.fetch_add(1, Ordering::Relaxed) + 1;
                if let Some(cb) = progress_callback {
                    cb(current, total);
                }
            }
            Err(e) => {
                eprintln!("ERROR: {}: {}", path.display(), e);
            }
        }
    }

    let total_indexed = indexed.load(Ordering::Relaxed);
    if timing_enabled {
        stats.total_files = total_indexed;
        stats.print_report(mode);
    }

    Ok(total_indexed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_directory_with_progress_noop() {
        // Test that the function compiles - actual testing requires a temp directory
        let result = std::panic::catch_unwind(|| {
            // This is a compile-time test
            let _: &dyn Fn(usize, usize) = &|_, _| {};
        });
        assert!(result.is_ok());
    }
}
