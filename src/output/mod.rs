//! JSON output module for CLI commands
//!
//! Provides schema-versioned, span-aware response types for all query commands.

pub mod command;
pub mod rich;

pub use command::{
    generate_execution_id, output_json, CollisionCandidate, CollisionGroup, CollisionsResponse,
    ErrorResponse, FilesResponse, FindResponse, JsonResponse, OutputFormat, QueryResponse,
    ReferenceMatch, RefsResponse, Span, StatusResponse, SymbolMatch, ValidationError,
    ValidationResponse, ValidationWarning,
};
