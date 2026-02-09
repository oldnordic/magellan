//! Status command implementation for Magellan
//!
//! Provides status query functionality and execution tracking.

use anyhow::Result;
use magellan::output::{generate_execution_id, output_json, JsonResponse, StatusResponse};
use magellan::{CodeGraph, OutputFormat};
use std::path::PathBuf;

/// Tracks execution metadata for logging and debugging
pub struct ExecutionTracker {
    exec_id: String,
    tool_version: String,
    args: Vec<String>,
    root: Option<String>,
    db_path: String,
    outcome: String,
    error_message: Option<String>,
    files_indexed: usize,
    symbols_indexed: usize,
    references_indexed: usize,
}

impl ExecutionTracker {
    pub fn new(args: Vec<String>, root: Option<String>, db_path: String) -> Self {
        Self {
            exec_id: generate_execution_id(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            args,
            root,
            db_path,
            outcome: "success".to_string(),
            error_message: None,
            files_indexed: 0,
            symbols_indexed: 0,
            references_indexed: 0,
        }
    }

    pub fn start(&self, graph: &CodeGraph) -> Result<()> {
        graph.execution_log().start_execution(
            &self.exec_id,
            &self.tool_version,
            &self.args,
            self.root.as_deref(),
            &self.db_path,
        )?;
        Ok(())
    }

    pub fn finish(&self, graph: &CodeGraph) -> Result<()> {
        graph.execution_log().finish_execution(
            &self.exec_id,
            &self.outcome,
            self.error_message.as_deref(),
            self.files_indexed,
            self.symbols_indexed,
            self.references_indexed,
        )
    }

    /// Set execution outcome to error with message
    ///
    /// Currently unused but provided for API completeness and future error handling.
    #[expect(dead_code)]
    pub fn set_error(&mut self, msg: String) {
        self.outcome = "error".to_string();
        self.error_message = Some(msg);
    }

    /// Set indexing counts for execution tracking
    ///
    /// Currently unused but provided for API completeness and future tracking.
    #[expect(dead_code)]
    pub fn set_counts(&mut self, files: usize, symbols: usize, references: usize) {
        self.files_indexed = files;
        self.symbols_indexed = symbols;
        self.references_indexed = references;
    }

    pub fn exec_id(&self) -> &str {
        &self.exec_id
    }
}

/// Run status query command
///
/// Usage: magellan status --db <FILE>
pub fn run_status(db_path: PathBuf, output_format: OutputFormat) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let tracker = ExecutionTracker::new(
        vec!["status".to_string()],
        None,
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;

    let file_count = graph.count_files()?;
    let symbol_count = graph.count_symbols()?;
    let reference_count = graph.count_references()?;
    let call_count = graph.count_calls()?;
    let chunk_count = graph.count_chunks()?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = StatusResponse {
                files: file_count,
                symbols: symbol_count,
                references: reference_count,
                calls: call_count,
                code_chunks: chunk_count,
            };
            let exec_id = tracker.exec_id().to_string();
            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
            println!("files: {}", file_count);
            println!("symbols: {}", symbol_count);
            println!("references: {}", reference_count);
            println!("calls: {}", call_count);
            println!("code_chunks: {}", chunk_count);
        }
    }

    tracker.finish(&graph)?;
    Ok(())
}
