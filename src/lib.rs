//! Magellan: A dumb, deterministic codebase mapping tool
//!
//! Magellan observes files, extracts symbols and references, and persists facts to sqlitegraph.

pub mod graph;
pub mod indexer;
pub mod ingest;
pub mod references;
pub mod verify;
pub mod watcher;

pub use graph::{CodeGraph, ScanProgress};
pub use indexer::{run_indexer, run_indexer_n};
pub use ingest::detect::{detect_language, Language};
pub use ingest::{Parser, SymbolFact, SymbolKind};
pub use references::{CallFact, ReferenceFact};
pub use verify::{verify_graph, VerifyReport};
pub use watcher::{EventType, FileEvent, FileSystemWatcher, WatcherConfig};
