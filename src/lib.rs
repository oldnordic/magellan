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

pub mod common;
pub mod diagnostics;
pub mod error_codes;
pub mod generation;
pub mod graph;
pub mod indexer;
pub mod ingest;
pub mod migrate_cmd;
pub mod output;
pub mod references;
pub mod validation;
pub mod verify;
pub mod watcher;

pub use common::{
    detect_language_from_path, extract_context_safe, extract_symbol_content_safe, format_symbol_kind,
    parse_symbol_kind, resolve_path,
};
pub use diagnostics::{DiagnosticStage, SkipReason, WatchDiagnostic};
pub use generation::{ChunkStore, CodeChunk};
pub use graph::filter::FileFilter;
pub use graph::query::SymbolQueryResult;
pub use graph::scan::ScanResult;
pub use graph::test_helpers::{delete_file_facts_with_injection, FailPoint};
pub use graph::{
    CodeGraph, CondensationGraph, CondensationResult, Cycle, CycleKind, CycleReport,
    DeadSymbol, DeleteResult, ExecutionPath, ExportConfig, ExportFormat, PathEnumerationResult,
    PathStatistics, ProgramSlice, ReconcileOutcome, ScanProgress, SliceDirection, SliceResult,
    SliceStatistics, Supernode, SymbolInfo, MAGELLAN_SCHEMA_VERSION,
};
pub use indexer::{run_indexer, run_indexer_n, run_watch_pipeline, WatchPipelineConfig};
pub use ingest::detect::{detect_language, Language};
pub use ingest::{Parser, SymbolFact, SymbolKind};
pub use output::command::{MigrateResponse, ReferenceMatch, Span, SymbolMatch};
pub use output::{generate_execution_id, output_json, JsonResponse, OutputFormat};
pub use references::{CallFact, ReferenceFact};
pub use validation::{
    canonicalize_path, normalize_path, validate_path_within_root, PathValidationError,
};
pub use verify::{verify_graph, VerifyReport};
pub use watcher::{EventType, FileEvent, FileSystemWatcher, WatcherBatch, WatcherConfig};
