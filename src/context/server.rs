//! HTTP Context Server for LLM access
//!
//! Provides a lightweight HTTP API for LLMs to query code context.
//!
//! # Endpoints
//!
//! - `GET /summary` - Project overview (~50 tokens)
//! - `GET /files` - List files (paginated)
//! - `GET /file?path=src/main.rs` - File context (~100 tokens)
//! - `GET /symbols` - List symbols (paginated)
//! - `GET /symbol?name=main` - Symbol detail (~150 tokens)
//! - `GET /callers?name=main` - What calls main()
//! - `GET /callees?name=main` - What main() calls
//!
//! # Usage
//!
//! ```bash
//! # Start server
//! magellan context-server --db code.db --port 8080
//!
//! # Query from LLM
//! curl http://localhost:8080/summary
//! curl http://localhost:8080/symbol?name=main
//! ```

use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;

use crate::graph::CodeGraph;
use super::query::{
    get_project_summary, get_symbol_detail, list_symbols, ListQuery,
};

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Database path
    pub db_path: PathBuf,
    /// Port to listen on
    pub port: u16,
    /// Host to bind to
    pub host: String,
    /// Enable CORS
    pub cors: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("codegraph.db"),
            port: 8080,
            host: "127.0.0.1".to_string(),
            cors: true,
        }
    }
}

/// Run the context server
pub fn run_context_server(config: ServerConfig) -> Result<()> {
    println!("Starting Magellan Context Server");
    println!("  Database: {:?}", config.db_path);
    println!("  Address: http://{}:{}", config.host, config.port);
    println!();
    println!("Endpoints:");
    println!("  GET /summary          - Project overview");
    println!("  GET /files            - List files (paginated)");
    println!("  GET /file?path=...    - File context");
    println!("  GET /symbols          - List symbols (paginated)");
    println!("  GET /symbol?name=...  - Symbol detail");
    println!("  GET /callers?name=... - What calls symbol");
    println!("  GET /callees?name=... - What symbol calls");
    println!();

    // Note: Full HTTP server implementation would use a crate like:
    // - axum (recommended, modern)
    // - warp (lightweight)
    // - actix-web (full-featured)
    //
    // For now, we provide the structure and response types.
    // The actual server implementation is deferred to avoid adding
    // heavy HTTP dependencies to the core library.

    println!("Note: HTTP server implementation requires additional dependencies.");
    println!("To enable, add to Cargo.toml:");
    println!("  [dependencies]");
    println!("  axum = \"0.7\"");
    println!("  tokio = {{ version = \"1\", features = [\"full\"] }}");
    println!();
    println!("Then implement the handlers in src/context/server.rs");

    Ok(())
}

/// API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }
}

/// Summary endpoint response
#[derive(Debug, Serialize)]
pub struct SummaryResponse {
    pub name: String,
    pub version: String,
    pub language: String,
    pub total_files: usize,
    pub total_symbols: usize,
    pub description: String,
}

/// Symbol endpoint response
#[derive(Debug, Serialize)]
pub struct SymbolResponse {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
    pub callers: Vec<String>,
    pub callees: Vec<String>,
}

/// List endpoint response (paginated)
#[derive(Debug, Serialize)]
pub struct ListResponse<T> {
    pub page: usize,
    pub total_pages: usize,
    pub page_size: usize,
    pub total_items: usize,
    pub next_cursor: Option<String>,
    pub items: Vec<T>,
}

// Example handler implementations (would be used with axum/warp/actix)

#[allow(dead_code)]
fn handle_summary(graph: &mut CodeGraph) -> Result<ApiResponse<SummaryResponse>> {
    let summary = get_project_summary(graph)?;
    
    Ok(ApiResponse::ok(SummaryResponse {
        name: summary.name,
        version: summary.version,
        language: summary.language,
        total_files: summary.total_files,
        total_symbols: summary.total_symbols,
        description: summary.description,
    }))
}

#[allow(dead_code)]
fn handle_symbol(
    graph: &mut CodeGraph,
    name: &str,
    file: Option<&str>,
) -> Result<ApiResponse<SymbolResponse>> {
    let detail = get_symbol_detail(graph, name, file)?;
    
    Ok(ApiResponse::ok(SymbolResponse {
        name: detail.name,
        kind: detail.kind,
        file: detail.file,
        line: detail.line,
        callers: detail.callers,
        callees: detail.callees,
    }))
}

#[allow(dead_code)]
fn handle_list_symbols(
    graph: &mut CodeGraph,
    query: &ListQuery,
) -> Result<ApiResponse<ListResponse<super::query::SymbolListItem>>> {
    let result = list_symbols(graph, query)?;
    
    Ok(ApiResponse::ok(ListResponse {
        page: result.page,
        total_pages: result.total_pages,
        page_size: result.page_size,
        total_items: result.total_items,
        next_cursor: result.next_cursor,
        items: result.items,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.host, "127.0.0.1");
        assert!(config.cors);
    }

    #[test]
    fn test_api_response_ok() {
        let response: ApiResponse<String> = ApiResponse::ok("test".to_string());
        assert!(response.success);
        assert_eq!(response.data, Some("test".to_string()));
        assert!(response.error.is_none());
    }

    #[test]
    fn test_api_response_err() {
        let response: ApiResponse<String> = ApiResponse::err("error".to_string());
        assert!(!response.success);
        assert!(response.data.is_none());
        assert_eq!(response.error, Some("error".to_string()));
    }
}
