//! Magellan: A dumb, deterministic codebase mapping tool
//!
//! Magellan observes files, extracts symbols and references, and persists facts to sqlitegraph.

pub mod graph;
pub mod indexer;
pub mod ingest;
pub mod references;
pub mod watcher;

pub use graph::CodeGraph;
pub use indexer::{run_indexer, run_indexer_n};
pub use ingest::{Parser, SymbolFact, SymbolKind};
pub use references::ReferenceFact;
pub use watcher::{EventType, FileEvent, FileSystemWatcher, WatcherConfig};
