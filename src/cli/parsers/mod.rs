//! CLI command parsers grouped by category.
//!
//! All functions are re-exported so existing code using `crate::cli::parsers::*`
//! continues to work without qualification.

pub mod catalog;
pub mod config_project;
pub mod core;
pub mod graph;
pub mod index;
pub mod query;
pub mod score;
pub mod semantic;
pub mod system;

// Re-export all public items from each submodule so callers can use
// `use crate::cli::parsers::*` and get everything.
pub use catalog::*;
pub use config_project::*;
pub use core::*;
pub use graph::*;
pub use index::*;
pub use query::*;
pub use score::*;
pub use semantic::*;
pub use system::*;
