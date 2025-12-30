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
pub use ingest::{Parser, SymbolFact, SymbolKind};
pub use ingest::detect::{Language, detect_language};
pub use references::{ReferenceFact, CallFact};
pub use verify::{VerifyReport, verify_graph};
pub use watcher::{EventType, FileEvent, FileSystemWatcher, WatcherConfig};
