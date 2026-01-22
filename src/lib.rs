//! Magellan: A dumb, deterministic codebase mapping tool
//!
//! Magellan observes files, extracts symbols and references, and persists facts to sqlitegraph.

pub mod diagnostics;
pub mod error_codes;
pub mod generation;
pub mod get_cmd;
pub mod graph;
pub mod indexer;
pub mod ingest;
pub mod output;
pub mod references;
pub mod validation;
pub mod verify;
pub mod watcher;

pub use diagnostics::{DiagnosticStage, SkipReason, WatchDiagnostic};
pub use generation::{ChunkStore, CodeChunk};
pub use graph::filter::FileFilter;
pub use graph::query::SymbolQueryResult;
pub use graph::{CodeGraph, DeleteResult, ExportConfig, ExportFormat, ReconcileOutcome, ScanProgress, MAGELLAN_SCHEMA_VERSION};
pub use graph::test_helpers::{delete_file_facts_with_injection, FailPoint};
pub use graph::scan::ScanResult;
pub use indexer::{run_indexer, run_indexer_n, run_watch_pipeline, WatchPipelineConfig};
pub use ingest::detect::{detect_language, Language};
pub use ingest::{Parser, SymbolFact, SymbolKind};
pub use references::{CallFact, ReferenceFact};
pub use verify::{verify_graph, VerifyReport};
pub use validation::{PathValidationError, canonicalize_path, validate_path_within_root};
pub use watcher::{EventType, FileEvent, FileSystemWatcher, WatcherBatch, WatcherConfig};
pub use output::{JsonResponse, OutputFormat, generate_execution_id, output_json};
pub use output::command::{Span, SymbolMatch, ReferenceMatch};
