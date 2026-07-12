use ahash::{AHashMap, AHashSet};
use sqlitegraph::algo::backend::graph_ops::strongly_connected_components;
use sqlitegraph::errors::SqliteGraphError;
use sqlitegraph::GraphBackend;
use std::collections::{HashSet, VecDeque};

use crate::graph::algorithms_types::{
    InternalPathEnumerationResult, PathEnumerationConfig, SccCollapseResult,
};

/// Backend-agnostic reachable_from implementation
///
/// Uses `fetch_outgoing` from GraphBackend trait instead of requiring SqliteGraph.
pub(crate) fn reachable_from(
    backend: &dyn GraphBackend,
    start: i64,
) -> Result<AHashSet<i64>, SqliteGraphError> {
    let mut visited = AHashSet::new();
    let mut queue = VecDeque::new();

    visited.insert(start);
    queue.push_back(start);

    while let Some(node) = queue.pop_front() {
        for neighbor in backend.fetch_outgoing(node)? {
            if visited.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }

    Ok(visited)
}

/// Backend-agnostic reverse_reachable_from implementation
///
/// Uses `fetch_incoming` from GraphBackend trait instead of requiring SqliteGraph.
pub(crate) fn reverse_reachable_from(
    backend: &dyn GraphBackend,
    start: i64,
) -> Result<AHashSet<i64>, SqliteGraphError> {
    let mut visited = AHashSet::new();
    let mut queue = VecDeque::new();

    visited.insert(start);
    queue.push_back(start);

    while let Some(node) = queue.pop_front() {
        for neighbor in backend.fetch_incoming(node)? {
            if visited.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }

    Ok(visited)
}

/// Backend-agnostic collapse_sccs implementation
///
/// Uses `all_entity_ids` and `fetch_outgoing` from GraphBackend trait.
pub(crate) fn collapse_sccs(
    backend: &dyn GraphBackend,
) -> Result<SccCollapseResult, SqliteGraphError> {
    let scc_result = strongly_connected_components(backend)?;

    if scc_result.components.is_empty() {
        return Ok(SccCollapseResult {
            _node_to_supernode: AHashMap::new(),
            supernode_members: AHashMap::new(),
            supernode_edges: Vec::new(),
            _num_sccs: 0,
        });
    }

    let mut node_to_supernode: AHashMap<i64, i64> = AHashMap::new();
    let mut supernode_members: AHashMap<i64, AHashSet<i64>> = AHashMap::new();

    for component in &scc_result.components {
        let supernode_id = component[0];
        let mut members = AHashSet::new();
        for &node in component {
            node_to_supernode.insert(node, supernode_id);
            members.insert(node);
        }
        supernode_members.insert(supernode_id, members);
    }

    let mut supernode_edges: Vec<(i64, i64)> = Vec::new();
    let mut seen_edges: HashSet<(i64, i64)> = HashSet::new();

    for (&node, &supernode) in &node_to_supernode {
        for neighbor in backend.fetch_outgoing(node)? {
            if let Some(&neighbor_supernode) = node_to_supernode.get(&neighbor) {
                if supernode != neighbor_supernode {
                    let edge = (supernode, neighbor_supernode);
                    if seen_edges.insert(edge) {
                        supernode_edges.push(edge);
                    }
                }
            }
        }
    }

    supernode_edges.sort();

    Ok(SccCollapseResult {
        _node_to_supernode: node_to_supernode,
        supernode_members,
        supernode_edges,
        _num_sccs: scc_result.components.len(),
    })
}

/// Backend-agnostic enumerate_paths implementation
///
/// Uses `fetch_outgoing` from GraphBackend trait.
pub(crate) fn enumerate_paths(
    backend: &dyn GraphBackend,
    entry: i64,
    config: &PathEnumerationConfig,
) -> Result<InternalPathEnumerationResult, SqliteGraphError> {
    let mut paths = Vec::new();
    let mut current_path = Vec::new();
    let mut visit_count: AHashMap<i64, usize> = AHashMap::new();
    let mut total_found = 0usize;
    let mut pruned_by_bounds = 0usize;
    let mut max_depth_reached = 0usize;

    dfs_enumerate(
        backend,
        entry,
        config,
        &mut current_path,
        &mut visit_count,
        &mut paths,
        &mut total_found,
        &mut pruned_by_bounds,
        &mut max_depth_reached,
    )?;

    Ok(InternalPathEnumerationResult {
        paths,
        total_found,
        pruned_by_bounds,
        _max_depth_reached: max_depth_reached,
    })
}

/// DFS helper for path enumeration
#[allow(
    clippy::too_many_arguments,
    reason = "DFS helper carries traversal state accumulators and bound counters"
)]
fn dfs_enumerate(
    backend: &dyn GraphBackend,
    node: i64,
    config: &PathEnumerationConfig,
    current_path: &mut Vec<i64>,
    visit_count: &mut AHashMap<i64, usize>,
    all_paths: &mut Vec<Vec<i64>>,
    total_found: &mut usize,
    pruned_by_bounds: &mut usize,
    max_depth_reached: &mut usize,
) -> Result<(), SqliteGraphError> {
    let count = visit_count.entry(node).or_insert(0);
    *count += 1;

    if *count > config.revisit_cap {
        visit_count.entry(node).and_modify(|e| *e -= 1);
        return Ok(());
    }

    current_path.push(node);
    let current_depth = current_path.len();
    *max_depth_reached = (*max_depth_reached).max(current_depth);

    if current_depth >= config.max_depth {
        *pruned_by_bounds += 1;
        current_path.pop();
        visit_count.entry(node).and_modify(|e| *e -= 1);
        return Ok(());
    }

    if let Some(ref exits) = config.exit_nodes {
        if exits.contains(&node) && current_depth > 1 {
            if all_paths.len() < config.max_paths {
                all_paths.push(current_path.clone());
            }
            *total_found += 1;
            current_path.pop();
            visit_count.entry(node).and_modify(|e| *e -= 1);
            return Ok(());
        }
    }

    let neighbors = backend.fetch_outgoing(node)?;
    let mut had_successors = false;

    for neighbor in neighbors {
        had_successors = true;
        dfs_enumerate(
            backend,
            neighbor,
            config,
            current_path,
            visit_count,
            all_paths,
            total_found,
            pruned_by_bounds,
            max_depth_reached,
        )?;
    }

    if !had_successors && config.exit_nodes.is_none() && current_depth > 1 {
        if all_paths.len() < config.max_paths {
            all_paths.push(current_path.clone());
        }
        *total_found += 1;
    }

    current_path.pop();
    visit_count.entry(node).and_modify(|e| *e -= 1);

    Ok(())
}
