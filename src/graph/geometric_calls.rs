//! Call graph analysis for geometric backend
//!
//! Provides types and functions for analyzing call graphs stored
//! in the geometric backend, including strongly connected components,
//! cycle detection, and graph condensation.

use std::collections::HashMap;

/// Result of strongly connected component analysis
#[derive(Debug, Clone)]
pub struct SccResult {
    /// Each component is a set of symbol IDs that are mutually reachable
    pub components: Vec<Vec<u64>>,
    /// Maps each symbol ID to its component index
    pub node_to_component: HashMap<u64, usize>,
}

/// Condensed DAG where each SCC is collapsed into a supernode
#[derive(Debug, Clone)]
pub struct CondensationDag {
    /// List of supernodes, each containing the original symbol IDs
    pub supernodes: Vec<Vec<u64>>,
    /// Maps each original symbol ID to its supernode index
    pub node_to_supernode: HashMap<u64, usize>,
    /// Edges between supernodes (from_index, to_index)
    pub edges: Vec<(usize, usize)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scc_result_empty() {
        let result = SccResult {
            components: Vec::new(),
            node_to_component: HashMap::new(),
        };
        assert!(result.components.is_empty());
        assert!(result.node_to_component.is_empty());
    }

    #[test]
    fn test_condensation_dag_empty() {
        let dag = CondensationDag {
            supernodes: Vec::new(),
            node_to_supernode: HashMap::new(),
            edges: Vec::new(),
        };
        assert!(dag.supernodes.is_empty());
        assert!(dag.edges.is_empty());
    }
}
