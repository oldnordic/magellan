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
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::context;
use crate::graph::CodeGraph;

/// Application state shared across handlers
pub struct AppState {
    pub db_path: PathBuf,
}

/// Build the axum router with the given state
pub fn create_app(state: Arc<AppState>) -> Router {
    // CORS for local development
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // API routes
        .route("/api/summary", get(handle_summary))
        .route("/api/symbols", get(handle_list_symbols))
        .route("/api/symbol/:name", get(handle_get_symbol))
        .route("/api/file/:path", get(handle_get_file))
        // Static files (serve from ./web-ui directory)
        .nest_service("/", ServeDir::new("web-ui"))
        .with_state(state)
        .layer(cors)
}

/// Start the web server
pub async fn run_web_server(db_path: PathBuf, host: String, port: u16) -> anyhow::Result<()> {
    let state = Arc::new(AppState { db_path: db_path.clone() });
    let app = create_app(state);

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
) -> Result<Json<SummaryResponse>, StatusCode> {
    let mut graph = CodeGraph::open(&state.db_path)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let total_calls = graph.count_calls()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match context::query::get_project_summary(&mut graph) {
        Ok(summary) => Ok(Json(SummaryResponse {
            total_files: summary.total_files,
            total_symbols: summary.total_symbols,
            total_calls,
        })),
        Err(e) => {
            eprintln!("Summary query failed: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

/// GET /api/symbols - List symbols with pagination
async fn handle_list_symbols(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListSymbolsParams>,
) -> Result<Json<context::query::PaginatedResult<context::query::SymbolListItem>>, StatusCode> {
    let mut graph = CodeGraph::open(&state.db_path)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let query = context::query::ListQuery {
        kind: params.kind,
        file_pattern: None,
        page: params.page,
        page_size: Some(params.page_size),
        cursor: params.cursor,
    };
    match context::query::list_symbols(&mut graph, &query) {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            eprintln!("List symbols query failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /api/symbol/:name - Get symbol detail
async fn handle_get_symbol(
    State(state): State<Arc<AppState>>,
    Path(_name): Path<String>,
) -> Result<Json<SymbolItem>, StatusCode> {
    let _graph = CodeGraph::open(&state.db_path)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Err(StatusCode::NOT_FOUND)
}

/// GET /api/file/:path - Get file context
async fn handle_get_file(
    State(state): State<Arc<AppState>>,
    Path(_file_path): Path<String>,
) -> Result<Json<FileResponse>, StatusCode> {
    let _graph = CodeGraph::open(&state.db_path)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Err(StatusCode::NOT_FOUND)
}

// ============================================================================
// Helper Functions
// ============================================================================

// ============================================================================
// Query Parameters
// ============================================================================

#[derive(Debug, Deserialize)]
struct ListSymbolsParams {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    page: Option<usize>,
    #[serde(default = "default_page_size")]
    page_size: usize,
    #[serde(default)]
    cursor: Option<String>,
}

fn default_page_size() -> usize { 50 }

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
struct SymbolItem {
    name: String,
    kind: String,
    file: String,
    line: usize,
}

#[derive(Debug, serde::Serialize)]
struct FileResponse {
    path: String,
    content: String,
    symbols: Vec<SymbolItem>,
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
    <meta charset="utf-8">
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
    <h1>Magellan Code Explorer</h1>

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
        fetch('/api/summary')
            .then(r => r.json())
            .then(data => {
                const stats = document.getElementById('stats');
                stats.textContent = '';
                const s1 = document.createElement('div');
                s1.className = 'stat';
                const v1 = document.createElement('div');
                v1.className = 'stat-value';
                v1.textContent = data.total_files;
                s1.appendChild(v1);
                s1.appendChild(document.createTextNode('Files'));
                stats.appendChild(s1);
                const s2 = document.createElement('div');
                s2.className = 'stat';
                const v2 = document.createElement('div');
                v2.className = 'stat-value';
                v2.textContent = data.total_symbols;
                s2.appendChild(v2);
                s2.appendChild(document.createTextNode('Symbols'));
                stats.appendChild(s2);
                const s3 = document.createElement('div');
                s3.className = 'stat';
                const v3 = document.createElement('div');
                v3.className = 'stat-value';
                v3.textContent = data.total_calls;
                s3.appendChild(v3);
                s3.appendChild(document.createTextNode('Calls'));
                stats.appendChild(s3);
            })
            .catch(e => {
                document.getElementById('stats').textContent = 'Error: ' + e;
            });

        fetch('/api/symbols?page=1&page_size=50')
            .then(r => r.json())
            .then(data => {
                const tbody = document.getElementById('symbols');
                if (data.total_items === 0) {
                    tbody.innerHTML = '<tr><td colspan="3">No symbols found.</td></tr>';
                    return;
                }
                tbody.textContent = '';
                data.items.forEach(sym => {
                    const tr = document.createElement('tr');
                    const td1 = document.createElement('td');
                    const a = document.createElement('a');
                    a.href = '/api/symbol/' + encodeURIComponent(sym.name);
                    a.textContent = sym.name;
                    td1.appendChild(a);
                    tr.appendChild(td1);
                    const td2 = document.createElement('td');
                    td2.textContent = sym.kind;
                    tr.appendChild(td2);
                    const td3 = document.createElement('td');
                    td3.textContent = sym.file + ':' + sym.line;
                    tr.appendChild(td3);
                    tbody.appendChild(tr);
                });
            })
            .catch(e => {
                document.getElementById('symbols').innerHTML = '<tr><td colspan="3">Error: ' + e + '</td></tr>';
            });
    </script>
</body>
</html>"#;
