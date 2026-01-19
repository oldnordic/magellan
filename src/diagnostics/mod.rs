//! Structured diagnostics for Magellan watch/index operations.
//!
//! Provides deterministic, sortable diagnostic types for skip reasons and errors.
//! Phase 2: Type definitions + deterministic ordering + stderr output.
//! Phase 3: JSON output with schema_version.

pub mod watch_diagnostics;

// Re-export main diagnostic types
pub use watch_diagnostics::{DiagnosticStage, SkipReason, WatchDiagnostic};
