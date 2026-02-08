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
//! ## Native V2 Backend
//!
//! - **`native-v2`**: Enable Native V2 backend with KV store and WAL transactions
//!   - Uses sqlitegraph's native file-based backend (no SQLite dependency)
//!   - Provides in-memory KV store for O(1) symbol lookups
//!   - Includes WAL (Write-Ahead Log) for crash recovery and transaction isolation
//!   - Production-ready and fully tested
//!
//! - **`native-v2-perf`**: Enable clustered adjacency for 10x traversal performance
//!   - Implies `native-v2`
//!   - Uses `sqlitegraph/v2_experimental` feature for clustered adjacency storage
//!   - Provides ~10x performance improvement for graph traversal algorithms
//!   - **Experimental**: Benchmark before making default (Phase 48)
//!   - Enables A/B performance testing in Phase 48-04 benchmarks
//!
//! ### Feature Selection
//!
//! ```toml
//! # Default: SQLite backend
//! magellan = "2.1.0"
//!
//! # Native V2 backend (production-ready)
//! magellan = { version = "2.1.0", features = ["native-v2"] }
//!
//! # Native V2 + clustered adjacency (experimental)
//! magellan = { version = "2.1.0", features = ["native-v2-perf"] }
//! ```
//!
//! ## Optional Features
//!
//! - **`llvm-cfg`**: LLVM IR-based CFG extraction for C/C++ (requires clang)
//! - **`bytecode-cfg`**: Java bytecode-based CFG extraction (requires Java bytecode library)

pub mod common;
pub mod diagnostics;
pub mod error_codes;
pub mod generation;
pub mod graph;
pub mod indexer;
pub mod ingest;
pub mod migrate_backend_cmd;
pub mod migrate_cmd;

// Re-export backend detection for CLI commands
pub use migrate_backend_cmd::{detect_backend_format, BackendFormat};
pub mod output;
pub mod references;
pub mod validation;
pub mod verify;
pub mod watcher;

// KV index module (only available with native-v2 feature)
#[cfg(feature = "native-v2")]
pub mod kv;

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
