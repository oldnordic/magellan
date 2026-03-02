//! Web UI server for Magellan
//!
//! Provides a simple web interface for exploring code graphs.
//! Uses axum (like codemcp) for the HTTP server.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::graph::CodeGraph;

/// Application state shared across handlers
pub struct AppState {
    pub db_path: PathBuf,
    pub graph: Arc<Mutex<CodeGraph>>,
}

/// Start the web server
pub async fn run_web_server(db_path: PathBuf, host: String, port: u16) -> anyhow::Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let state = Arc::new(AppState {
        db_path,
        graph: Arc::new(Mutex::new(graph)),
    });

    // CORS for local development
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router with API routes and static file serving
    let app = Router::new()
        // API routes
        .route("/api/summary", get(handle_summary))
        .route("/api/symbols", get(handle_list_symbols))
        .route("/api/symbol/:name", get(handle_get_symbol))
        .route("/api/file/:path", get(handle_get_file))
        // Static files (serve from ./web-ui directory)
        .nest_service("/", ServeDir::new("web-ui"))
        .with_state(state)
        .layer(cors);

    let addr = format!("{}:{}", host, port);
    println!("🌐 Magellan Web UI starting at http://{}", addr);
    println!("   API endpoints:");
    println!("   - GET /api/summary");
    println!("   - GET /api/symbols");
    println!("   - GET /api/symbol/:name");
    println!("   - GET /api/file/:path");
    println!("   - Static files served from ./web-ui/");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ============================================================================
// API Handlers
// ============================================================================

/// GET /api/summary - Project overview
async fn handle_summary(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let graph = state.graph.lock().await;
    
    // Get basic stats from the graph
    let files = match graph.count_files() {
        Ok(c) => c,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    
    let symbols = match graph.count_symbols() {
        Ok(c) => c,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    
    let calls = match graph.count_calls() {
        Ok(c) => c,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    
    let summary = SummaryResponse {
        total_files: files,
        total_symbols: symbols,
        total_calls: calls,
    };
    
    Json(summary).into_response()
}

/// GET /api/symbols - List symbols with pagination
async fn handle_list_symbols(
    State(_state): State<Arc<AppState>>,
    Query(params): Query<SymbolListParams>,
) -> impl IntoResponse {
    // For now, return empty list - full implementation would query the graph
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50);
    
    let result = ListResponse::<SymbolItem> {
        page,
        total_pages: 0,
        page_size,
        total_items: 0,
        next_cursor: None,
        prev_cursor: None,
        items: vec![],
    };
    
    Json(result).into_response()
}

/// GET /api/symbol/:name - Get symbol detail
async fn handle_get_symbol(
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
) -> impl IntoResponse {
    // Placeholder - would query graph for symbol details
    error_response(StatusCode::NOT_FOUND, "Symbol not found")
}

/// GET /api/file/:path - Get file context
async fn handle_get_file(
    State(_state): State<Arc<AppState>>,
    Path(_file_path): Path<String>,
) -> impl IntoResponse {
    // Placeholder - would query graph for file context
    error_response(StatusCode::NOT_FOUND, "File not found")
}

// ============================================================================
// Helper Functions
// ============================================================================

fn error_response(status: StatusCode, message: &str) -> (StatusCode, &'static str) {
    // Leak the string to get a static reference (acceptable for error responses)
    (status, Box::leak(message.to_string().into_boxed_str()))
}

// ============================================================================
// Query Parameters
// ============================================================================

#[derive(Debug, Deserialize)]
struct SymbolListParams {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    page: Option<usize>,
    #[serde(default)]
    page_size: Option<usize>,
    #[serde(default)]
    cursor: Option<String>,
}

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, serde::Serialize)]
struct SummaryResponse {
    total_files: usize,
    total_symbols: usize,
    total_calls: usize,
}

#[derive(Debug, serde::Serialize)]
struct ListResponse<T> {
    page: usize,
    total_pages: usize,
    page_size: usize,
    total_items: usize,
    next_cursor: Option<String>,
    prev_cursor: Option<String>,
    items: Vec<T>,
}

#[derive(Debug, serde::Serialize)]
struct SymbolItem {
    name: String,
    kind: String,
    file: String,
    line: usize,
}

// ============================================================================
// Embedded HTML UI
// ============================================================================

/// Get the index HTML page
pub fn get_index_html() -> Html<&'static str> {
    Html(INDEX_HTML)
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>Magellan Code Explorer</title>
    <style>
        body { font-family: system-ui; margin: 2rem; }
        .card { border: 1px solid #ccc; padding: 1rem; margin: 1rem 0; border-radius: 8px; }
        .stat { display: inline-block; margin-right: 2rem; }
        .stat-value { font-size: 2rem; font-weight: bold; color: #007bff; }
        table { width: 100%; border-collapse: collapse; }
        th, td { padding: 0.5rem; text-align: left; border-bottom: 1px solid #eee; }
        a { color: #007bff; text-decoration: none; }
        a:hover { text-decoration: underline; }
    </style>
</head>
<body>
    <h1>🔍 Magellan Code Explorer</h1>
    
    <div id="summary" class="card">
        <h2>Project Summary</h2>
        <div id="stats">Loading...</div>
    </div>
    
    <div class="card">
        <h2>Symbols</h2>
        <input type="text" id="search" placeholder="Search symbols..." style="width: 100%; padding: 0.5rem; margin-bottom: 1rem;">
        <table>
            <thead>
                <tr>
                    <th>Name</th>
                    <th>Kind</th>
                    <th>File</th>
                </tr>
            </thead>
            <tbody id="symbols"><tr><td colspan="3">Loading...</td></tr></tbody>
        </table>
    </div>
    
    <script>
        // Load summary
        fetch('/api/summary')
            .then(r => r.json())
            .then(data => {
                document.getElementById('stats').innerHTML = `
                    <div class="stat"><div class="stat-value">${data.total_files}</div>Files</div>
                    <div class="stat"><div class="stat-value">${data.total_symbols}</div>Symbols</div>
                    <div class="stat"><div class="stat-value">${data.total_calls}</div>Calls</div>
                `;
            })
            .catch(e => {
                document.getElementById('stats').innerHTML = 'Error loading summary: ' + e;
            });
        
        // Load symbols
        fetch('/api/symbols?page=1&page_size=50')
            .then(r => r.json())
            .then(data => {
                const tbody = document.getElementById('symbols');
                if (data.total_items === 0) {
                    tbody.innerHTML = '<tr><td colspan="3">No symbols found. Use magellan context list for now.</td></tr>';
                    return;
                }
                tbody.innerHTML = '';
                data.items.forEach(sym => {
                    const tr = document.createElement('tr');
                    tr.innerHTML = `
                        <td><a href="/api/symbol/${sym.name}">${sym.name}</a></td>
                        <td>${sym.kind}</td>
                        <td>${sym.file}:${sym.line}</td>
                    `;
                    tbody.appendChild(tr);
                });
            })
            .catch(e => {
                document.getElementById('symbols').innerHTML = '<tr><td colspan="3">Error: ' + e + '</td></tr>';
            });
    </script>
</body>
</html>"#;
