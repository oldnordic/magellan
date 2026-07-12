use anyhow::Result;
use std::path::Path;

use crate::references::{CallFact, ReferenceFact};

use super::{
    calls, cfg_edges_extract, ops, query, CfgBlock, CodeGraph, DeleteResult, DirectCallIcfgEdge,
};

impl CodeGraph {
    /// Index a file into the graph (idempotent)
    pub fn index_file(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        ops::index_file(self, path, source)
    }

    pub fn register_snapshot(&self, spec: &crate::temporal::SnapshotSpec) -> Result<i64> {
        let mut conn = self.side_conn.lock();
        crate::temporal::register_snapshot(&mut conn, spec)
    }

    pub fn ingest_snapshot_sources(
        &self,
        snapshot_id: i64,
        repo_root: &Path,
        files: &[crate::temporal::SnapshotFileInput],
    ) -> Result<crate::temporal::SnapshotIngestStats> {
        crate::temporal::ingest_snapshot_sources(self, snapshot_id, repo_root, files)
    }

    /// Delete a file and all derived data from the graph
    pub fn delete_file(&mut self, path: &str) -> Result<DeleteResult> {
        ops::delete_file(self, path)
    }

    /// Delete ALL facts derived from a file path.
    pub fn delete_file_facts(&mut self, path: &str) -> Result<DeleteResult> {
        ops::delete_file_facts(self, path)
    }

    /// Query all symbols defined in a file
    pub fn symbols_in_file(&mut self, path: &str) -> Result<Vec<crate::ingest::SymbolFact>> {
        query::symbols_in_file(self, path)
    }

    /// Query symbols defined in a file, optionally filtered by kind
    pub fn symbols_in_file_with_kind(
        &mut self,
        path: &str,
        kind: Option<crate::ingest::SymbolKind>,
    ) -> Result<Vec<crate::ingest::SymbolFact>> {
        query::symbols_in_file_with_kind(self, path, kind)
    }

    /// Query symbol facts along with their node IDs for deterministic ordering/output.
    pub fn symbol_nodes_in_file(
        &mut self,
        path: &str,
    ) -> Result<Vec<(i64, crate::ingest::SymbolFact)>> {
        query::symbol_nodes_in_file(self, path)
    }

    /// Query the node ID of a specific symbol by file path and symbol name
    pub fn symbol_id_by_name(&mut self, path: &str, name: &str) -> Result<Option<i64>> {
        query::symbol_id_by_name(self, path, name)
    }

    /// Query the persisted stable symbol ID of a specific symbol by file path and symbol name.
    pub fn stable_symbol_id_by_name(&mut self, path: &str, name: &str) -> Result<Option<String>> {
        query::stable_symbol_id_by_name(self, path, name)
    }

    /// Index references for a file into the graph
    pub fn index_references(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        query::index_references(self, path, source)
    }

    /// Query all references to a specific symbol
    pub fn references_to_symbol(&mut self, symbol_id: i64) -> Result<Vec<ReferenceFact>> {
        query::references_to_symbol(self, symbol_id)
    }

    /// Lookup symbol extent (byte + line span) for a specific symbol name in a file.
    pub fn symbol_extents(
        &mut self,
        path: &str,
        name: &str,
    ) -> Result<Vec<(i64, crate::ingest::SymbolFact)>> {
        query::symbol_extents(self, path, name)
    }

    /// Index calls for a file into the graph
    pub fn index_calls(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        calls::index_calls(self, path, source)
    }

    /// Query all calls FROM a specific symbol (forward call graph)
    pub fn calls_from_symbol(&mut self, path: &str, name: &str) -> Result<Vec<CallFact>> {
        calls::calls_from_symbol(self, path, name)
    }

    /// Query all calls TO a specific symbol (reverse call graph)
    pub fn callers_of_symbol(&mut self, path: &str, name: &str) -> Result<Vec<CallFact>> {
        calls::callers_of_symbol(self, path, name)
    }

    /// Stitch direct call sites from a function CFG to callee entry CFG blocks.
    pub fn direct_call_icfg_edges(
        &mut self,
        path: &str,
        name: &str,
    ) -> Result<Vec<DirectCallIcfgEdge>> {
        let Some(caller_symbol_id) = self.symbol_id_by_name(path, name)? else {
            return Ok(Vec::new());
        };

        let caller_blocks = self.cfg_ops.get_cfg_for_function(caller_symbol_id)?;
        let caller_edges = self.cfg_ops.get_cfg_edges_for_function(caller_symbol_id)?;
        let resolved_calls = self.calls.resolved_calls_from_symbol(caller_symbol_id)?;

        let mut stitched = Vec::new();
        for (call, callee_symbol_id) in resolved_calls {
            let Some(caller_block_idx) = find_call_block_index(&caller_blocks, &call) else {
                continue;
            };
            let caller_resume_block_idx =
                find_resume_block_index(&caller_edges, caller_block_idx, caller_blocks.len());

            let callee_blocks = self.cfg_ops.get_cfg_for_function(callee_symbol_id)?;
            let Some(callee_entry_block_idx) =
                callee_blocks.iter().position(|block| block.kind == "entry")
            else {
                continue;
            };
            let callee_return_block_indices = find_return_block_indices(&callee_blocks);

            stitched.push(DirectCallIcfgEdge {
                call,
                caller_symbol_id,
                callee_symbol_id,
                caller_block_idx,
                callee_entry_block_idx,
                caller_resume_block_idx,
                callee_return_block_indices,
            });
        }

        Ok(stitched)
    }
}

fn find_call_block_index(blocks: &[CfgBlock], call: &CallFact) -> Option<usize> {
    let call_start = call.byte_start as u64;
    let call_end = call.byte_end as u64;

    blocks
        .iter()
        .enumerate()
        .find(|(_, block)| {
            block.kind == "call" && block.byte_start <= call_start && call_end <= block.byte_end
        })
        .map(|(idx, _)| idx)
        .or_else(|| {
            blocks
                .iter()
                .enumerate()
                .find(|(_, block)| block.byte_start <= call_start && call_end <= block.byte_end)
                .map(|(idx, _)| idx)
        })
}

fn find_resume_block_index(
    edges: &[cfg_edges_extract::CfgEdge],
    caller_block_idx: usize,
    block_count: usize,
) -> Option<usize> {
    let mut candidates: Vec<usize> = edges
        .iter()
        .filter(|edge| {
            edge.source_idx == caller_block_idx
                && edge.edge_type == cfg_edges_extract::CfgEdgeType::Fallthrough
                && edge.target_idx < block_count
                && edge.target_idx != caller_block_idx
        })
        .map(|edge| edge.target_idx)
        .collect();
    candidates.sort_unstable();
    candidates.dedup();
    candidates.into_iter().next()
}

fn find_return_block_indices(blocks: &[CfgBlock]) -> Vec<usize> {
    blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.kind == "return" || block.terminator == "return")
        .map(|(idx, _)| idx)
        .collect()
}
