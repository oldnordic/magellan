//! Integration tests for the unified HTTP server (web-ui feature)

use magellan::web_ui::{create_app, AppState};
use std::sync::Arc;

/// Helper to create an ephemeral test database with a minimal graph.
/// Returns the TempDir (must be kept alive so the DB file isn't deleted)
/// and the path to the database file.
fn setup_test_db() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // Open graph (creates DB + schema) and index a tiny Rust source file directly.
    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let source = r#"fn main() {
    hello();
}

fn hello() {
    println!("hi");
}
"#;
    graph.index_file("src/main.rs", source.as_bytes()).unwrap();

    (dir, db_path)
}

#[tokio::test]
async fn test_api_summary_returns_data() {
    let (_dir, db_path) = setup_test_db();
    let state = Arc::new(AppState { db_path });
    let app = create_app(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/api/summary", addr))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("total_files").is_some());
    assert!(json.get("total_symbols").is_some());
    assert!(json.get("total_calls").is_some());
}

#[tokio::test]
async fn test_api_symbols_paginated() {
    let (_dir, db_path) = setup_test_db();
    let state = Arc::new(AppState { db_path });
    let app = create_app(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/api/symbols?page=1&page_size=10", addr))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("items").is_some());
    assert!(json.get("total_items").is_some());
    assert!(json.get("page").is_some());
}

#[tokio::test]
async fn test_api_symbol_detail() {
    let (_dir, db_path) = setup_test_db();
    let state = Arc::new(AppState { db_path });
    let app = create_app(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "http://{}/api/symbol?name=main&file=src/main.rs",
            addr
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["name"], "main");
}

#[test]
fn test_context_server_command_removed() {
    // The old `context-server` subcommand was removed; `magellan` should exit with an error.
    let output = std::process::Command::new("cargo")
        .args(["run", "--", "context-server"])
        .current_dir(std::env::current_dir().unwrap())
        .output()
        .expect("failed to execute cargo run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{} {}", stdout, stderr);

    assert!(
        !output.status.success(),
        "expected magellan context-server to fail, but it succeeded"
    );
    assert!(
        combined.contains("Error") || combined.contains("unrecognized"),
        "expected error message about unknown subcommand, got: {}",
        combined
    );
}
