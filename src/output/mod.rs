//! JSON output module for CLI commands
//!
//! Provides schema-versioned, span-aware response types for all query commands.

pub mod command;

pub use command::{
    generate_execution_id, output_json, ErrorResponse, FilesResponse,
    FindResponse, JsonResponse, OutputFormat, QueryResponse, RefsResponse, ReferenceMatch, Span,
    StatusResponse, SymbolMatch,
    ValidationResponse, ValidationError, ValidationWarning,
};
