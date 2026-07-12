use serde::{Deserialize, Serialize};

use super::{ReferenceMatch, Span, SymbolMatch};

/// Response for query command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    /// Symbols found in the queried file
    pub symbols: Vec<SymbolMatch>,
    /// File path that was queried
    pub file_path: String,
    /// Kind filter that was applied (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_filter: Option<String>,
}

/// Response for find command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindResponse {
    /// Matching symbols found
    pub matches: Vec<SymbolMatch>,
    /// Name that was queried
    pub query_name: String,
    /// File filter that was applied (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_filter: Option<String>,
}

/// Response for refs command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefsResponse {
    /// References found
    pub references: Vec<ReferenceMatch>,
    /// Symbol name that was queried
    pub symbol_name: String,
    /// File path containing the symbol
    pub file_path: String,
    /// Direction ("in" for callers, "out" for callees)
    pub direction: String,
}

/// Caller information with project attribution for cross-project queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCallerInfo {
    /// Source project name (derived from DB filename)
    pub project: String,
    /// Name of the calling function
    pub name: String,
    /// File containing the call
    pub file_path: String,
    /// Line where call occurs
    pub line: usize,
    /// Column where call occurs
    pub column: usize,
    /// Hop depth for recursive traversal (None = direct)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
}

/// Callee information with project attribution for cross-project queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCalleeInfo {
    /// Source project name (derived from DB filename)
    pub project: String,
    /// Name of the called function
    pub name: String,
    /// File containing the callee definition
    pub file_path: String,
    /// Line number where the call occurs
    pub line: u32,
    /// Hop depth for recursive traversal (None = direct)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
}

/// Symbol match with project attribution for multi-DB context queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSymbolMatch {
    /// Source project name (derived from DB filename)
    pub project: String,
    /// Stable match ID
    pub match_id: String,
    /// Symbol span (location in source code)
    pub span: Span,
    /// Symbol name
    pub name: String,
    /// Symbol kind (normalized)
    pub kind: String,
    /// Containing symbol (if nested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Stable symbol ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    /// Functions that call this symbol (cross-project)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callers: Option<Vec<ProjectCallerInfo>>,
    /// Functions that this symbol calls (cross-project)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callees: Option<Vec<ProjectCalleeInfo>>,
    /// Source code snippet (when --with-source is used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Response for context command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResponse {
    /// Name that was queried
    pub query: String,
    /// Projects searched
    pub projects: Vec<String>,
    /// Matching symbols found across all projects
    pub matches: Vec<ProjectSymbolMatch>,
}

/// Collision candidate details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionCandidate {
    pub entity_id: i64,
    pub symbol_id: Option<String>,
    pub canonical_fqn: Option<String>,
    pub display_fqn: Option<String>,
    pub name: Option<String>,
    pub file_path: Option<String>,
}

/// Collision group response entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionGroup {
    pub field: String,
    pub value: String,
    pub count: usize,
    pub candidates: Vec<CollisionCandidate>,
}

/// Response for collisions command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionsResponse {
    pub field: String,
    pub groups: Vec<CollisionGroup>,
}

/// Response for files command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesResponse {
    /// All indexed files (sorted deterministically)
    pub files: Vec<String>,
    /// Symbol count per file (optional, when --symbols flag is used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_counts: Option<std::collections::HashMap<String, usize>>,
}

/// Response for status command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    /// Number of indexed files
    pub files: usize,
    /// Number of indexed symbols
    pub symbols: usize,
    /// Number of indexed references
    pub references: usize,
    /// Number of indexed calls
    pub calls: usize,
    /// Number of code chunks
    pub code_chunks: usize,
    /// Coverage data (always present for stable JSON shape)
    pub coverage: CoverageInfo,
}

/// Coverage information with stable JSON shape.
///
/// All fields are always serialized so consumers can rely on a fixed schema.
/// Use `available` to distinguish "no coverage data" from "zero coverage".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageInfo {
    /// Whether any coverage data has been ingested
    pub available: bool,
    /// Number of covered CFG blocks
    pub covered_blocks: usize,
    /// Number of covered CFG edges
    pub covered_edges: usize,
    /// Source kind (e.g. "lcov"), when available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Source revision (e.g. git commit hash), when available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Ingestion timestamp (RFC 3339), when available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingested_at: Option<String>,
}

/// Response for validation command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResponse {
    /// Whether validation passed
    pub passed: bool,
    /// Number of errors found
    pub error_count: usize,
    /// Detailed error information
    pub errors: Vec<ValidationError>,
    /// Number of warnings found
    pub warning_count: usize,
    /// Detailed warning information
    pub warnings: Vec<ValidationWarning>,
}

/// Response for migrate command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateResponse {
    /// Whether migration succeeded
    pub success: bool,
    /// Path to backup file (if created)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    /// Previous schema version
    pub old_version: i64,
    /// New schema version
    pub new_version: i64,
    /// Human-readable message
    pub message: String,
}

/// A validation error with structured data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Machine-readable error code (SCREAMING_SNAKE_CASE)
    pub code: String,
    /// Human-readable error description
    pub message: String,
    /// Related stable symbol_id if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Additional structured data
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

/// A validation warning with structured data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    /// Machine-readable warning code (SCREAMING_SNAKE_CASE)
    pub code: String,
    /// Human-readable warning description
    pub message: String,
    /// Related stable symbol_id if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Additional structured data
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

// Conversion from internal validation report to public response type
impl From<crate::graph::validation::ValidationReport> for ValidationResponse {
    fn from(report: crate::graph::validation::ValidationReport) -> Self {
        ValidationResponse {
            passed: report.passed,
            error_count: report.errors.len(),
            errors: report
                .errors
                .into_iter()
                .map(|error| ValidationError {
                    code: error.code,
                    message: error.message,
                    entity_id: error.entity_id,
                    details: error.details,
                })
                .collect(),
            warning_count: report.warnings.len(),
            warnings: report
                .warnings
                .into_iter()
                .map(|warning| ValidationWarning {
                    code: warning.code,
                    message: warning.message,
                    entity_id: warning.entity_id,
                    details: warning.details,
                })
                .collect(),
        }
    }
}

/// Response for program slicing command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceResponse {
    /// Target symbol being sliced
    pub target: SymbolMatch,
    /// Slice direction: "backward" or "forward"
    pub direction: String,
    /// Symbols included in the slice
    pub included_symbols: Vec<SymbolMatch>,
    /// Statistics about the slice
    pub statistics: SliceStats,
}

/// Statistics for program slice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceStats {
    /// Total number of symbols in the slice
    pub total_symbols: usize,
    /// Number of data dependencies (0 for call-graph fallback)
    pub data_dependencies: usize,
    /// Number of control dependencies
    pub control_dependencies: usize,
}
