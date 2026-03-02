//! LSIF (Language Server Index Format) export for cross-repository navigation
//!
//! LSIF is a standard format for indexing source code that enables:
//! - Cross-repository symbol resolution
//! - Dependency tracking
//! - Code intelligence across project boundaries
//!
//! See: https://lsif.dev/

pub mod export;
pub mod import;
pub mod schema;

pub use export::export_lsif;
pub use import::import_lsif;
pub use schema::{LsifGraph, Vertex, Edge, PackageData};
