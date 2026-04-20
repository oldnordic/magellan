//! Web UI server for Magellan
//!
//! Provides a simple web interface for exploring code graphs.
//! Uses axum (like codemcp) for the HTTP server.

use axum::{
    extract::{Query, State},
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
        .route("/api/symbol", get(handle_get_symbol))
        .route("/api/file", get(handle_get_file))
        .route("/api/callers", get(handle_get_callers))
        .route("/api/callees", get(handle_get_callees))
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
    println!("   - GET /api/symbol?name=<name>");
    println!("   - GET /api/file?path=...");
    println!("   - GET /api/callers?name=<name>&file=<path>");
    println!("   - GET /api/callees?name=<name>&file=<path>");
    println!("   - Static files served from ./web-ui/");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct CallersQuery {
    name: String,
    #[serde(default)]
    file: Option<String>,
}

async fn handle_get_callers(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallersQuery>,
) -> Result<Json<Vec<context::query::SymbolListItem>>, StatusCode> {
    let mut graph = CodeGraph::open(&state.db_path)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    match context::query::get_callers(&mut graph, &params.name, params.file.as_deref()) {
        Ok(callers) => Ok(Json(callers)),
        Err(e) => {
            eprintln!("Callers query failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_get_callees(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallersQuery>,
) -> Result<Json<Vec<context::query::SymbolListItem>>, StatusCode> {
    let mut graph = CodeGraph::open(&state.db_path)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    match context::query::get_callees(&mut graph, &params.name, params.file.as_deref()) {
        Ok(callees) => Ok(Json(callees)),
        Err(e) => {
            eprintln!("Callees query failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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

#[derive(Debug, Deserialize)]
struct SymbolQuery {
    name: String,
    #[serde(default)]
    file: Option<String>,
}

/// GET /api/symbol - Get symbol detail
async fn handle_get_symbol(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SymbolQuery>,
) -> Result<Json<context::query::SymbolDetail>, StatusCode> {
    let mut graph = CodeGraph::open(&state.db_path)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    match context::query::get_symbol_detail(&mut graph, &params.name, params.file.as_deref()) {
        Ok(detail) => Ok(Json(detail)),
        Err(e) => {
            let err_str = format!("{}", e);
            if err_str.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                eprintln!("Symbol detail query failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct FileQuery {
    path: String,
}

/// GET /api/file?path=... - Get file context
async fn handle_get_file(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileQuery>,
) -> Result<Json<context::query::FileContext>, StatusCode> {
    let mut graph = CodeGraph::open(&state.db_path)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    match context::query::get_file_context(&mut graph, &params.path) {
        Ok(context) => Ok(Json(context)),
        Err(e) => {
            let err_str = format!("{}", e);
            if err_str.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                eprintln!("File context query failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
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
        body { font-family: system-ui, -apple-system, sans-serif; margin: 2rem; background: #f8f9fa; color: #212529; }
        .card { background: #fff; border: 1px solid #dee2e6; padding: 1rem; margin: 1rem 0; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.04); }
        .stat { display: inline-block; margin-right: 2rem; }
        .stat-value { font-size: 2rem; font-weight: bold; color: #007bff; }
        table { width: 100%; border-collapse: collapse; }
        th, td { padding: 0.5rem; text-align: left; border-bottom: 1px solid #eee; }
        a { color: #007bff; text-decoration: none; }
        a:hover { text-decoration: underline; }
        .error { color: #dc3545; }
        #search { width: 100%; padding: 0.5rem; margin-bottom: 1rem; border: 1px solid #ced4da; border-radius: 4px; box-sizing: border-box; }
        .pagination { margin-top: 1rem; text-align: center; }
        .pagination button { margin: 0 0.25rem; padding: 0.35rem 0.7rem; border: 1px solid #007bff; background: #fff; color: #007bff; border-radius: 4px; cursor: pointer; }
        .pagination button:hover { background: #007bff; color: #fff; }
        .pagination button:disabled { opacity: 0.5; cursor: not-allowed; background: #fff; color: #007bff; }
        .pagination span { margin: 0 0.5rem; }
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
        <input type="text" id="search" placeholder="Search symbols...">
        <table>
            <thead>
                <tr>
                    <th>Name</th>
                    <th>Kind</th>
                    <th>File</th>
                    <th>Line</th>
                </tr>
            </thead>
            <tbody id="symbols"></tbody>
        </table>
        <div id="pagination" class="pagination"></div>
    </div>

    <script>
        (function() {
            function setError(element, message) {
                element.textContent = '';
                var err = document.createElement('div');
                err.className = 'error';
                err.textContent = message;
                element.appendChild(err);
            }

            function renderStats(data) {
                var stats = document.getElementById('stats');
                stats.textContent = '';
                var labels = ['Files', 'Symbols', 'Calls'];
                var keys = ['total_files', 'total_symbols', 'total_calls'];
                for (var i = 0; i < keys.length; i++) {
                    var s = document.createElement('div');
                    s.className = 'stat';
                    var v = document.createElement('div');
                    v.className = 'stat-value';
                    v.textContent = data[keys[i]];
                    s.appendChild(v);
                    s.appendChild(document.createTextNode(labels[i]));
                    stats.appendChild(s);
                }
            }

            function renderSymbols(data) {
                var tbody = document.getElementById('symbols');
                tbody.textContent = '';
                if (!data.items || data.items.length === 0) {
                    var tr = document.createElement('tr');
                    var td = document.createElement('td');
                    td.colSpan = 4;
                    td.textContent = 'No symbols found.';
                    tr.appendChild(td);
                    tbody.appendChild(tr);
                    return;
                }
                for (var i = 0; i < data.items.length; i++) {
                    var sym = data.items[i];
                    var tr = document.createElement('tr');

                    var tdName = document.createElement('td');
                    var a = document.createElement('a');
                    a.href = '/api/symbol?name=' + encodeURIComponent(sym.name);
                    a.textContent = sym.name;
                    tdName.appendChild(a);
                    tr.appendChild(tdName);

                    var tdKind = document.createElement('td');
                    tdKind.textContent = sym.kind || '';
                    tr.appendChild(tdKind);

                    var tdFile = document.createElement('td');
                    tdFile.textContent = sym.file || '';
                    tr.appendChild(tdFile);

                    var tdLine = document.createElement('td');
                    tdLine.textContent = sym.line != null ? String(sym.line) : '';
                    tr.appendChild(tdLine);

                    tbody.appendChild(tr);
                }
            }

            function renderPagination(data, query) {
                var pag = document.getElementById('pagination');
                pag.textContent = '';
                if (data.total_pages <= 1) return;
                var current = data.page || 1;
                var prev = document.createElement('button');
                prev.textContent = 'Prev';
                prev.disabled = current <= 1;
                prev.onclick = function() { loadSymbols(current - 1, query); };
                pag.appendChild(prev);

                var info = document.createElement('span');
                info.textContent = 'Page ' + current + ' of ' + data.total_pages;
                pag.appendChild(info);

                var next = document.createElement('button');
                next.textContent = 'Next';
                next.disabled = current >= data.total_pages;
                next.onclick = function() { loadSymbols(current + 1, query); };
                pag.appendChild(next);
            }

            function loadSymbols(page, query) {
                page = page || 1;
                var url = '/api/symbols?page=' + encodeURIComponent(page) + '&page_size=50';
                if (query) {
                    url += '&kind=' + encodeURIComponent(query);
                }
                var tbody = document.getElementById('symbols');
                tbody.textContent = 'Loading...';
                fetch(url)
                    .then(function(r) {
                        if (!r.ok) throw new Error('HTTP ' + r.status);
                        return r.json();
                    })
                    .then(function(data) {
                        renderSymbols(data);
                        renderPagination(data, query);
                    })
                    .catch(function(e) {
                        setError(tbody, 'Error loading symbols: ' + e.message);
                    });
            }

            fetch('/api/summary')
                .then(function(r) {
                    if (!r.ok) throw new Error('HTTP ' + r.status);
                    return r.json();
                })
                .then(function(data) {
                    renderStats(data);
                })
                .catch(function(e) {
                    setError(document.getElementById('stats'), 'Error loading summary: ' + e.message);
                });

            loadSymbols(1);

            var searchInput = document.getElementById('search');
            var debounceTimer;
            searchInput.addEventListener('input', function() {
                clearTimeout(debounceTimer);
                debounceTimer = setTimeout(function() {
                    loadSymbols(1, searchInput.value.trim());
                }, 300);
            });
            searchInput.addEventListener('keydown', function(e) {
                if (e.key === 'Enter') {
                    clearTimeout(debounceTimer);
                    loadSymbols(1, searchInput.value.trim());
                }
            });
        })();
    </script>
</body>
</html>"#;
