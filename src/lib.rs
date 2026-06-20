#![allow(unused_imports)]
//! Magellan: A dumb, deterministic codebase mapping tool
//!
//! Magellan observes files, extracts symbols and references, and persists facts to sqlitegraph.
//!
//! # Position Conventions
//!
//! Magellan uses tree-sitter position conventions for all symbol and reference data:
//! - **Line positions**: 1-indexed (line 1 is the first line)
//! - **Column positions**: 0-indexed (column 0 is the first character)
//! - **Byte offsets**: 0-indexed from file start
//!
//! See [MANUAL.md](../MANUAL.md#3-position-conventions) for detailed documentation.
//!
//! # Feature Flags
//!
//! ## Backend Selection (Choose One)
//!
//! Magellan supports two storage backends via sqlitegraph:
//!
//! ### SQLite Backend (Default)
//! - **`sqlite-backend`**: Stable SQLite-based storage
//!   - Widely compatible, well-tested
//!   - Use for maximum compatibility
//!
//! ## Optional Features
//!
//! - **`llvm-cfg`**: LLVM IR-based CFG extraction for C/C++ (requires clang)
//! - **`bytecode-cfg`**: Java bytecode-based CFG extraction (requires Java bytecode library)
//!
//! # Graph Memory
//!
//! Magellan can index external documents (wiki pages, markdown) as source documents
//! and extract candidate facts (subject-predicate-object triples) from them.
//!
//! - **Source inventory**: `source-inventory` CLI command scans directories and
//!   stores document metadata (path, kind, hash, tags, wikilinks) in the
//!   `source_documents` table (schema v13+).
//! - **Candidate facts**: `candidate-fact` CLI command submits, lists, and
//!   validates facts extracted from source documents, stored in the
//!   `candidate_facts` table (schema v14+). Facts have statuses: `pending`,
//!   `accepted`, or `rejected`.
//!
//! See [MANUAL.md](../MANUAL.md) for full CLI reference.

pub mod backend_router;
pub mod capabilities;
pub mod common;
pub mod config;
pub mod context;
pub mod diagnostics;
pub mod error_codes;
pub mod framework;
pub mod generation;
pub mod graph;
pub mod indexer;
pub mod ingest;
pub mod lsif;
pub mod lsp;

pub mod ingest_coverage;
pub mod ingest_coverage_cmd;
pub mod migrate_backend_cmd;
pub mod migrate_cmd;

// Re-export backend detection for CLI commands
pub use backend_router::{MagellanBackend, UnifiedSymbolInfo};
pub use capabilities::all_capabilities;
pub use migrate_backend_cmd::{detect_backend_format, BackendFormat};
pub mod manifest;
pub mod output;
pub mod project_config;
pub mod references;
pub mod temporal;
pub mod validation;
pub mod verify;
pub mod watcher;

pub use common::{
    detect_language_from_path, detect_project_root, extract_context_safe,
    extract_symbol_content_safe, format_symbol_kind, normalize_repo_relative_path,
    parse_symbol_kind, resolve_path,
};
pub use diagnostics::{DiagnosticStage, SkipReason, WatchDiagnostic};
pub use framework::{FrameworkSymbol, MagellanFramework, ProjectHandle};
pub use generation::{ChunkStore, CodeChunk};
pub use graph::candidate_fact::{
    ensure_schema as ensure_candidate_fact_schema, find_by_id as find_candidate_fact_by_id,
    insert as insert_candidate_fact, list_by_status as list_candidate_facts_by_status,
    review_queue as candidate_fact_review_queue, update_status as update_candidate_fact_status,
    validate_ontology, CandidateFact, CandidateProperties, CandidateStatus, ConflictSet,
    ConflictType, ResolutionStatus, ValidationError, ValidationResult,
};
pub use graph::filter::FileFilter;
pub use graph::query::{cross_file_references_to, SymbolQueryResult};
pub use graph::scan::ScanResult;
pub use graph::source_inventory::{
    compute_hash, ensure_schema, extract_frontmatter, extract_metadata, extract_tags,
    extract_title, extract_wikilinks, find_stale, insert_or_update, list_by_kind,
    parse_frontmatter, scan_directory, scan_file, ExtractedMetadata, SourceDocument,
};
pub use graph::telemetry::{TelemetryEvent, TelemetryEventType, TelemetryOps};
pub use graph::test_helpers::{delete_file_facts_with_injection, FailPoint};
pub use graph::CrossFileRef;
pub use graph::{extract_ast_nodes, is_structural_kind, AstNode};
pub use graph::{
    CodeGraph, CondensationGraph, CondensationResult, Cycle, CycleKind, CycleReport, DeadSymbol,
    DeleteResult, ExecutionPath, ExportConfig, ExportFormat, MultiDbContext, PathEnumerationResult,
    PathStatistics, ProgramSlice, ReconcileOutcome, ScanProgress, SliceDirection, SliceResult,
    SliceStatistics, Supernode, SymbolInfo, MAGELLAN_SCHEMA_VERSION,
};
pub use indexer::{run_indexer, run_indexer_n, run_watch_pipeline, WatchPipelineConfig};
pub use ingest::detect::{detect_language, Language};
pub use ingest::pool::with_parser as parse_with_language;
pub use ingest::{ImplRelation, Parser, SymbolFact, SymbolKind};
pub use output::command::{MigrateResponse, ReferenceMatch, Span, SymbolMatch};
pub use output::{generate_execution_id, output_json, JsonResponse, OutputFormat};
pub use references::{CallFact, ReferenceFact};
pub use temporal::{SnapshotFileInput, SnapshotIngestStats, SnapshotSpec};
pub use validation::{
    canonicalize_path, normalize_path, validate_path_within_root, PathValidationError,
};
pub use verify::{verify_graph, VerifyReport};
pub use watcher::{EventType, FileEvent, FileSystemWatcher, WatcherBatch, WatcherConfig};
