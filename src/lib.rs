//! Magellan: A dumb, deterministic codebase mapping tool
//!
//! Magellan observes files, extracts symbols and references, and persists facts to sqlitegraph.

pub mod diagnostics;
pub mod generation;
pub mod get_cmd;
pub mod graph;
pub mod indexer;
pub mod ingest;
pub mod references;
pub mod verify;
pub mod watcher;

pub use generation::{ChunkStore, CodeChunk};
pub use graph::query::SymbolQueryResult;
pub use graph::{CodeGraph, ReconcileOutcome, ScanProgress, MAGELLAN_SCHEMA_VERSION};
pub use indexer::{run_indexer, run_indexer_n, run_watch_pipeline, WatchPipelineConfig};
pub use ingest::detect::{detect_language, Language};
pub use ingest::{Parser, SymbolFact, SymbolKind};
pub use references::{CallFact, ReferenceFact};
pub use verify::{verify_graph, VerifyReport};
pub use watcher::{EventType, FileEvent, FileSystemWatcher, WatcherBatch, WatcherConfig};
