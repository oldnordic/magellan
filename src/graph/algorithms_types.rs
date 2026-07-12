use std::collections::HashMap;

use ahash::{AHashMap, AHashSet};

/// Result of SCC collapse operation
#[derive(Debug, Clone)]
pub(crate) struct SccCollapseResult {
    /// Maps each original node ID to its SCC supernode ID
    pub(crate) _node_to_supernode: AHashMap<i64, i64>,
    /// Maps each supernode ID to the set of original node IDs in that SCC
    pub(crate) supernode_members: AHashMap<i64, AHashSet<i64>>,
    /// Edges between supernodes in the condensed DAG
    pub(crate) supernode_edges: Vec<(i64, i64)>,
    /// Total number of SCCs found
    pub(crate) _num_sccs: usize,
}

/// Path enumeration result for backend-agnostic implementation
#[derive(Debug, Clone)]
pub(crate) struct InternalPathEnumerationResult {
    /// All found paths (each path is a sequence of node IDs)
    pub(crate) paths: Vec<Vec<i64>>,
    /// Total number of paths found (before max_paths limit)
    pub(crate) total_found: usize,
    /// Number of paths pruned by bounds (max_depth, max_paths)
    pub(crate) pruned_by_bounds: usize,
    /// Maximum depth reached during enumeration
    pub(crate) _max_depth_reached: usize,
}

/// Configuration for path enumeration
#[derive(Debug, Clone)]
pub(crate) struct PathEnumerationConfig {
    /// Maximum depth to explore
    pub(crate) max_depth: usize,
    /// Maximum number of paths to return
    pub(crate) max_paths: usize,
    /// Maximum times to revisit a node (prevents infinite loops)
    pub(crate) revisit_cap: usize,
    /// Optional set of nodes that terminate path exploration
    pub(crate) exit_nodes: Option<AHashSet<i64>>,
    /// Optional set of nodes that represent errors
    pub(crate) _error_nodes: Option<AHashSet<i64>>,
}

impl Default for PathEnumerationConfig {
    fn default() -> Self {
        Self {
            max_depth: 100,
            max_paths: 1000,
            revisit_cap: 100,
            exit_nodes: None,
            _error_nodes: None,
        }
    }
}

/// Symbol information for algorithm results
///
/// Contains the key metadata needed to identify and locate a symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    /// Stable symbol ID (32-char BLAKE3 hash)
    pub symbol_id: Option<String>,
    /// Fully-qualified name
    pub fqn: Option<String>,
    /// File path containing the symbol
    pub file_path: String,
    /// Symbol kind (Function, Method, Class, etc.)
    pub kind: String,
}

/// Dead symbol information
///
/// Extends [`SymbolInfo`] with a reason why the symbol is considered dead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadSymbol {
    /// Base symbol information
    pub symbol: SymbolInfo,
    /// Reason why this symbol is unreachable/dead
    pub reason: String,
}

/// Cycle kind classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleKind {
    /// Multiple symbols calling each other (SCC with >1 member)
    MutualRecursion,
    /// Single symbol that calls itself (direct self-loop)
    SelfLoop,
}

/// Cycle information for detected cycles
///
/// Represents a strongly connected component (SCC) with more than one member,
/// indicating mutual recursion or a cycle in the call graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle {
    /// All symbols that participate in this cycle
    pub members: Vec<SymbolInfo>,
    /// Classification of the cycle type
    pub kind: CycleKind,
}

/// Cycle detection report
///
/// Result of running [`CodeGraph::detect_cycles()`], containing all cycles
/// found in the call graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleReport {
    /// All detected cycles
    pub cycles: Vec<Cycle>,
    /// Total number of cycles found
    pub total_count: usize,
}

/// Supernode in a condensation graph
///
/// Represents an SCC collapsed into a single node for DAG analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supernode {
    /// Supernode ID (stable identifier for this SCC)
    pub id: i64,
    /// All symbols that are members of this SCC/supernode
    pub members: Vec<SymbolInfo>,
}

/// Condensation graph (DAG after SCC collapse)
///
/// Represents the call graph after collapsing all SCCs into supernodes.
/// The condensation graph is always a DAG (no cycles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CondensationGraph {
    /// All supernodes in the condensed graph
    pub supernodes: Vec<Supernode>,
    /// Edges between supernodes (from_supernode_id, to_supernode_id)
    pub edges: Vec<(i64, i64)>,
}

/// Condensation result with symbol-to-supernode mapping
///
/// Result of running [`CodeGraph::condense_call_graph()`], providing
/// both the condensed DAG and the mapping from original symbols to supernodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CondensationResult {
    /// The condensed DAG
    pub graph: CondensationGraph,
    /// Maps symbol_id to the supernode ID containing that symbol
    pub original_to_supernode: HashMap<String, i64>,
}

/// Direction of program slicing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceDirection {
    /// Backward slice: what affects this symbol (reverse reachability)
    Backward,
    /// Forward slice: what this symbol affects (forward reachability)
    Forward,
}

/// Program slice result
///
/// Contains the slice results and statistics for a program slicing operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramSlice {
    /// Target symbol for the slice
    pub target: SymbolInfo,
    /// Direction of the slice
    pub direction: SliceDirection,
    /// Symbols included in the slice
    pub included_symbols: Vec<SymbolInfo>,
    /// Number of symbols in the slice
    pub symbol_count: usize,
}

/// Program slice result with statistics
///
/// Wraps a [`ProgramSlice`] with additional statistics about the slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceResult {
    /// The slice itself
    pub slice: ProgramSlice,
    /// Statistics about the slice
    pub statistics: SliceStatistics,
}

/// Statistics for a program slice
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceStatistics {
    /// Total number of symbols in the slice
    pub total_symbols: usize,
    /// Number of data dependencies
    /// Note: Set to 0 for call-graph fallback (not computed without full CFG)
    pub data_dependencies: usize,
    /// Number of control dependencies
    /// For call-graph fallback, this equals total_symbols (callers/callees)
    pub control_dependencies: usize,
}

/// Execution path in the call graph
///
/// Represents a single path through the call graph from a starting symbol
/// to an ending symbol, with metadata about the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPath {
    /// Symbols along the path in order from start to end
    pub symbols: Vec<SymbolInfo>,
    /// Number of symbols in the path
    pub length: usize,
}

/// Path enumeration result
///
/// Contains all discovered execution paths and statistics about the enumeration.
#[derive(Debug, Clone)]
pub struct PathEnumerationResult {
    /// All discovered paths
    pub paths: Vec<ExecutionPath>,
    /// Total number of paths enumerated
    pub total_enumerated: usize,
    /// Whether enumeration was cut off due to bounds
    pub bounded_hit: bool,
    /// Statistics about the discovered paths
    pub statistics: PathStatistics,
}

/// Statistics for path enumeration
#[derive(Debug, Clone)]
pub struct PathStatistics {
    /// Average path length
    pub avg_length: f64,
    /// Minimum path length
    pub min_length: usize,
    /// Maximum path length
    pub max_length: usize,
    /// Number of unique symbols across all paths
    pub unique_symbols: usize,
}
