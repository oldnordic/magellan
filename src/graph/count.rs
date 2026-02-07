//! Count operations for CodeGraph
//!
//! Provides methods to count files, symbols, references, and calls.

use anyhow::Result;
use sqlitegraph::SnapshotId;

use super::CodeGraph;

/// Count total number of files in the graph
pub fn count_files(graph: &CodeGraph) -> Result<usize> {
    let snapshot = SnapshotId::current();
    Ok(graph
        .files
        .backend
        .entity_ids()?
        .into_iter()
        .filter(|id| {
            graph
                .files
                .backend
                .get_node(snapshot, *id)
                .map(|n| n.kind == "File")
                .unwrap_or(false)
        })
        .count())
}

/// Count total number of symbols in the graph
pub fn count_symbols(graph: &CodeGraph) -> Result<usize> {
    let snapshot = SnapshotId::current();
    Ok(graph
        .symbols
        .backend
        .entity_ids()?
        .into_iter()
        .filter(|id| {
            graph
                .symbols
                .backend
                .get_node(snapshot, *id)
                .map(|n| n.kind == "Symbol")
                .unwrap_or(false)
        })
        .count())
}

/// Count total number of references in the graph
pub fn count_references(graph: &CodeGraph) -> Result<usize> {
    let snapshot = SnapshotId::current();
    Ok(graph
        .references
        .backend
        .entity_ids()?
        .into_iter()
        .filter(|id| {
            graph
                .references
                .backend
                .get_node(snapshot, *id)
                .map(|n| n.kind == "Reference")
                .unwrap_or(false)
        })
        .count())
}

/// Count total number of calls in the graph
pub fn count_calls(graph: &CodeGraph) -> Result<usize> {
    let snapshot = SnapshotId::current();
    Ok(graph
        .calls
        .backend
        .entity_ids()?
        .into_iter()
        .filter(|id| {
            graph
                .calls
                .backend
                .get_node(snapshot, *id)
                .map(|n| n.kind == "Call")
                .unwrap_or(false)
        })
        .count())
}
