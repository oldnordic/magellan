//! Admin socket: JSON-RPC request/response handler for daemon control

use anyhow::{Context, Result};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, Mutex};

use super::registry::Registry;

mod evolve_handlers;
mod query_handlers;

type WatcherMap = Arc<Mutex<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>;

pub struct AdminSocket;

impl AdminSocket {
    /// Handle a single client connection (one request per line)
    /// Handle a single client connection (Phase 6: supports runtime watcher spawn)
    pub async fn handle_client(
        stream: UnixStream,
        registry: Arc<Mutex<Registry>>,
        meta_db: Arc<Mutex<super::meta_db::MetaDb>>,
        batch_tx: mpsc::Sender<super::types::TaggedBatch>,
        watcher_map: Option<WatcherMap>,
        _shutdown_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<()> {
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half).lines();

        while let Some(line) = reader.next_line().await? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let response = match Self::dispatch(
                line,
                registry.clone(),
                meta_db.clone(),
                batch_tx.clone(),
                watcher_map.clone(),
                _shutdown_rx.clone(),
            )
            .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let mut meta = meta_db.lock().await;
                    let ev = super::meta_db::DaemonEvent {
                        id: None,
                        event_type: "admin_err".to_string(),
                        project_name: None,
                        file_path: None,
                        details: Some(serde_json::json!({ "error": e.to_string() })),
                        created_at: now_secs(),
                        execution_id: None,
                    };
                    let _ = meta.log_event(&ev);
                    json!({
                        "id": null,
                        "error": { "code": -32603, "message": format!("Internal error: {}", e) }
                    })
                }
            };

            let resp_line = serde_json::to_string(&response)? + "\n";
            write_half.write_all(resp_line.as_bytes()).await?;
        }

        Ok(())
    }

    async fn dispatch(
        line: &str,
        registry: Arc<Mutex<Registry>>,
        meta_db: Arc<Mutex<super::meta_db::MetaDb>>,
        batch_tx: mpsc::Sender<super::types::TaggedBatch>,
        watcher_map: Option<WatcherMap>,
        _shutdown_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<serde_json::Value> {
        let req: super::types::ServiceRequest =
            serde_json::from_str(line).context("Invalid JSON-RPC request")?;

        let id = req.id.clone();
        let method = req.method.clone();
        let params = req.params;

        tracing::info!(method = %method, "Admin request received");

        // Fast path: ping must not wait on meta_db lock or disk I/O
        if method == "ping" {
            return Ok(
                super::types::ServiceResponse::ok(id, serde_json::json!({"pong": true})).into_val(),
            );
        }

        {
            let mut meta = meta_db.lock().await;
            let mut ev = super::meta_db::DaemonEvent {
                id: None,
                event_type: "admin_request".to_string(),
                project_name: None,
                file_path: None,
                details: Some(serde_json::json!({ "method": &method })),
                created_at: {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64
                },
                execution_id: None,
            };
            if matches!(
                method.as_str(),
                "register" | "unregister" | "pause" | "resume"
            ) {
                ev.project_name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            let _ = meta.log_event(&ev);
        }

        match method.as_str() {
            "list" => {
                let reg = registry.lock().await;
                let names: Vec<String> = reg.enabled_names();
                Ok(super::types::ServiceResponse::ok(id, json!({ "projects": names })).into_val())
            }

            "status" => {
                let reg = registry.lock().await;
                let all = reg
                    .list()
                    .iter()
                    .map(|p| {
                        json!({
                            "name": &p.name,
                            "root": &p.root,
                            "db": &p.db,
                            "enabled": p.enabled,
                            "source": &p.source,
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(super::types::ServiceResponse::ok(id, json!({ "projects": all })).into_val())
            }

            "register" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unnamed")
                    .to_string();
                let root = params
                    .get("root")
                    .and_then(|v| v.as_str())
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let source = params
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("manual")
                    .to_string();
                let include: Vec<String> = params
                    .get("include")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let exclude: Vec<String> = params
                    .get("exclude")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let db = super::registry::Registry::canonical_db_path(&name);

                let entry = super::types::ProjectEntry::new(name.clone(), root.clone(), db, source)
                    .with_include(include.clone())
                    .with_exclude(exclude.clone());
                let mut reg = registry.lock().await;
                reg.register(entry)?;
                // Phase 6: spawn watcher if map / shutdown available and not already running
                if let Some(wm) = watcher_map.clone() {
                    let wm_guard = wm.lock().await;
                    if !wm_guard.contains_key(&name) {
                        drop(wm_guard);
                        let tx = batch_tx.clone();
                        let (local_tx, local_rx) = tokio::sync::watch::channel(false);
                        let name_w = name.clone();
                        let inc = include.clone();
                        let exc = exclude.clone();
                        tokio::spawn(async move {
                            super::watcher_task(root, name_w, local_rx, tx, inc, exc).await;
                        });
                        let mut wm_guard = wm.lock().await;
                        wm_guard.insert(name.clone(), local_tx);
                    }
                }
                Ok(super::types::ServiceResponse::ok(id, json!({ "registered": name })).into_val())
            }

            "unregister" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let mut reg = registry.lock().await;
                let removed = reg.unregister(name)?;
                Ok(super::types::ServiceResponse::ok(id, json!({ "removed": removed })).into_val())
            }

            "pause" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let mut reg = registry.lock().await;
                let ok = reg.pause(name)?;
                Ok(super::types::ServiceResponse::ok(id, json!({ "paused": ok })).into_val())
            }

            "resume" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let (root_opt, include_opt, exclude_opt, enabled) = {
                    let mut reg = registry.lock().await;
                    let ok = reg.resume(name)?;
                    let entry = reg.find(name);
                    let root = entry.map(|e| e.root.clone());
                    let include = entry.map(|e| e.include.clone()).unwrap_or_default();
                    let exclude = entry.map(|e| e.exclude.clone()).unwrap_or_default();
                    (root, include, exclude, ok)
                };
                // Phase 6: spawn watcher on resume if map / shutdown available
                if let Some(root) = root_opt {
                    if let Some(wm) = watcher_map.clone() {
                        let wm_guard = wm.lock().await;
                        if !wm_guard.contains_key(name) {
                            drop(wm_guard);
                            let tx = batch_tx.clone();
                            let (local_tx, local_rx) = tokio::sync::watch::channel(false);
                            let name_str = name.to_string();
                            let inc = include_opt;
                            let exc = exclude_opt;
                            tokio::spawn(async move {
                                super::watcher_task(root, name_str, local_rx, tx, inc, exc).await;
                            });
                            let mut wm_guard = wm.lock().await;
                            wm_guard.insert(name.to_string(), local_tx);
                        }
                    }
                }
                Ok(super::types::ServiceResponse::ok(id, json!({ "resumed": enabled })).into_val())
            }

            "watch" => {
                let tag = params
                    .get("tag")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default")
                    .to_string();
                let paths = params
                    .get("paths")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(std::path::PathBuf::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let batch = super::types::TaggedBatch {
                    project_name: tag,
                    paths,
                };
                // Queue to dispatcher channel
                if let Err(e) = batch_tx.send(batch.clone()).await {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32002,
                        format!("Dispatch queue closed: {}", e),
                    )
                    .into_val());
                }
                Ok(super::types::ServiceResponse::ok(
                    id,
                    json!({ "queued": batch.project_name, "files": batch.paths.len() }),
                )
                .into_val())
            }

            "stop" => {
                // Signal daemon shutdown via the shared shutdown channel
                // The caller receives acknowledgment before the daemon exits
                // Phase 1: propagate stop via request-injection or global signal
                Ok(super::types::ServiceResponse::ok(id, json!({ "stopping": true })).into_val())
            }

            "stats" => {
                let meta = meta_db.lock().await;
                match meta.list_projects() {
                    Ok(projects) => {
                        let arr: Vec<serde_json::Value> = projects
                            .iter()
                            .map(|p| {
                                json!({
                                    "name": p.name,
                                    "root": p.root,
                                    "db_path": p.db_path,
                                    "enabled": p.enabled,
                                    "last_reindexed": p.last_reindexed,
                                    "file_count": p.file_count,
                                    "symbol_count": p.symbol_count,
                                })
                            })
                            .collect();
                        Ok(
                            super::types::ServiceResponse::ok(id, json!({ "projects": arr }))
                                .into_val(),
                        )
                    }
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32003,
                        format!("Meta-db query error: {}", e),
                    )
                    .into_val()),
                }
            }

            "query.find" => query_handlers::handle_find(id, params, meta_db.clone()).await,

            "query.context" => query_handlers::handle_context(id, params, meta_db.clone()).await,

            "query.compare" => query_handlers::handle_compare(id, params, meta_db.clone()).await,

            "query.suggest" => query_handlers::handle_suggest(id, params, meta_db.clone()).await,

            "query.build-index" => query_handlers::handle_build_index(id, meta_db.clone()).await,

            "evolve.analyze" => evolve_handlers::handle_analyze(id, params, meta_db.clone()).await,

            "evolve.retrieve" => {
                evolve_handlers::handle_retrieve(id, params, meta_db.clone()).await
            }

            "evolve.propose" => evolve_handlers::handle_propose(id, params, registry.clone()).await,

            "evolve.candidates" => {
                evolve_handlers::handle_candidates(id, params, registry.clone()).await
            }

            "evolve.promote" => evolve_handlers::handle_promote(id, params, registry.clone()).await,

            "evolve.reject" => evolve_handlers::handle_reject(id, params, registry.clone()).await,

            "evolve.verify" => evolve_handlers::handle_verify(id, params, registry.clone()).await,

            "events" => {
                let project: Option<String> = params
                    .get("project")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let event_type: Option<String> = params
                    .get("event_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let since_hours: Option<i64> =
                    params.get("since_hours").and_then(|v| v.as_u64()).map(|h| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64;
                        now - (h as i64 * 3600)
                    });
                let limit: usize = params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|u| u as usize)
                    .unwrap_or(50);

                let meta = meta_db.lock().await;
                let filter = super::meta_db::EventFilter {
                    project,
                    event_type,
                    since: since_hours,
                    until: None,
                    limit,
                };
                match meta.list_events(&filter) {
                    Ok(events) => {
                        let arr: Vec<serde_json::Value> = events
                            .iter()
                            .map(|e| {
                                json!({
                                    "id": e.id,
                                    "event_type": e.event_type,
                                    "project_name": e.project_name,
                                    "file_path": e.file_path,
                                    "details": e.details,
                                    "created_at": e.created_at,
                                    "execution_id": e.execution_id,
                                })
                            })
                            .collect();
                        Ok(
                            super::types::ServiceResponse::ok(id, json!({ "events": arr }))
                                .into_val(),
                        )
                    }
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32003,
                        format!("Events query error: {}", e),
                    )
                    .into_val()),
                }
            }

            _ => Ok(super::types::ServiceResponse::not_implemented(id, method).into_val()),
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
