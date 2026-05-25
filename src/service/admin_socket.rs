//! Admin socket: JSON-RPC request/response handler for daemon control

use anyhow::{Context, Result};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, Mutex};

use super::registry::Registry;

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
        watcher_map: Option<
            Arc<Mutex<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
        >,
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
        watcher_map: Option<
            Arc<Mutex<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
        >,
        _shutdown_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<serde_json::Value> {
        let req: super::types::ServiceRequest =
            serde_json::from_str(line).context("Invalid JSON-RPC request")?;

        let id = req.id.clone();
        let method = req.method.clone();
        let params = req.params;

        tracing::info!(method = %method, "Admin request received");

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
            "ping" => Ok(super::types::ServiceResponse::ok(id, json!({"pong": true })).into_val()),

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
                let db = super::registry::Registry::canonical_db_path(&name);

                let entry = super::types::ProjectEntry::new(name.clone(), root.clone(), db, source);
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
                        tokio::spawn(async move {
                            super::watcher_task(root, name_w, local_rx, tx).await;
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
                let (root_opt, enabled) = {
                    let mut reg = registry.lock().await;
                    let ok = reg.resume(name)?;
                    let root = reg.find(name).map(|e| e.root.clone());
                    (root, ok)
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
                            tokio::spawn(async move {
                                super::watcher_task(root, name_str, local_rx, tx).await;
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

            "query.find" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let file = params
                    .get("file")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let depth = params
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|d| d as usize);
                let callers = params
                    .get("callers")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let callees = params
                    .get("callees")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Resolve DB paths from meta.db under async lock
                let db_paths: Vec<std::path::PathBuf> = {
                    let meta = meta_db.lock().await;
                    meta.list_projects()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|p| p.enabled)
                        .map(|p| std::path::PathBuf::from(&p.db_path))
                        .collect()
                };

                let json_matches = {
                    let name_for_query = name.clone();
                    tokio::task::spawn_blocking(move || {
                        let mut ctx = match magellan::MultiDbContext::from_paths(&db_paths) {
                            Ok(c) => c,
                            Err(e) => return Err(anyhow::anyhow!("multi_db open error: {}", e)),
                        };
                        let results = ctx.search_symbol(
                            &name_for_query,
                            file.as_deref(),
                            depth,
                            callers,
                            callees,
                        );
                        let arr: Vec<serde_json::Value> = results
                            .iter()
                            .map(|m| {
                                json!({
                                    "project": &m.project,
                                    "name": &m.name,
                                    "kind": &m.kind,
                                    "file_path": &m.span.file_path,
                                    "start_line": m.span.start_line,
                                    "start_col": m.span.start_col,
                                    "end_line": m.span.end_line,
                                    "end_col": m.span.end_col,
                                })
                            })
                            .collect();
                        Ok(arr)
                    })
                    .await
                };

                match json_matches {
                    Ok(Ok(arr)) => Ok(super::types::ServiceResponse::ok(
                        id,
                        json!({ "query": name, "matches": arr }),
                    )
                    .into_val()),
                    Ok(Err(e)) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32003,
                        format!("Query error: {}", e),
                    )
                    .into_val()),
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Blocking task panic: {}", e),
                    )
                    .into_val()),
                }
            }

            "query.context" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let file = params
                    .get("file")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let depth = params
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|d| d as usize);
                let callers = params
                    .get("callers")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let callees = params
                    .get("callees")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let db_paths: Vec<std::path::PathBuf> = {
                    let meta = meta_db.lock().await;
                    meta.list_projects()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|p| p.enabled)
                        .map(|p| std::path::PathBuf::from(&p.db_path))
                        .collect()
                };

                let name_for_query = name.clone();
                let json_matches = tokio::task::spawn_blocking(move || {
                    let mut ctx = match magellan::MultiDbContext::from_paths(&db_paths) {
                        Ok(c) => c,
                        Err(e) => return Err(anyhow::anyhow!("multi_db open error: {}", e)),
                    };
                    let results = ctx.search_symbol(
                        &name_for_query,
                        file.as_deref(),
                        depth,
                        callers,
                        callees,
                    );
                    let arr: Vec<serde_json::Value> = results
                        .iter()
                        .map(|m| {
                            let caller_arr: serde_json::Value = m
                                .callers
                                .as_ref()
                                .map(|cs| {
                                    serde_json::Value::Array(
                                        cs.iter()
                                            .map(|c| {
                                                json!({
                                                    "name": &c.name,
                                                    "file": &c.file_path,
                                                    "line": c.line,
                                                })
                                            })
                                            .collect(),
                                    )
                                })
                                .unwrap_or(serde_json::Value::Null);
                            let callee_arr: serde_json::Value = m
                                .callees
                                .as_ref()
                                .map(|cs| {
                                    serde_json::Value::Array(
                                        cs.iter()
                                            .map(|c| {
                                                json!({
                                                    "name": &c.name,
                                                    "file": &c.file_path,
                                                    "line": c.line,
                                                })
                                            })
                                            .collect(),
                                    )
                                })
                                .unwrap_or(serde_json::Value::Null);
                            json!({
                                "project": &m.project,
                                "name": &m.name,
                                "kind": &m.kind,
                                "file_path": &m.span.file_path,
                                "start_line": m.span.start_line,
                                "callers": caller_arr,
                                "callees": callee_arr,
                            })
                        })
                        .collect();
                    Ok(arr)
                })
                .await;

                match json_matches {
                    Ok(Ok(arr)) => Ok(super::types::ServiceResponse::ok(
                        id,
                        json!({ "query": name, "matches": arr }),
                    )
                    .into_val()),
                    Ok(Err(e)) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32003,
                        format!("Query error: {}", e),
                    )
                    .into_val()),
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Blocking task panic: {}", e),
                    )
                    .into_val()),
                }
            }

            "query.compare" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let project_names: Vec<String> = params
                    .get("projects")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                // Resolve DB paths + pre-fetch cross-ref scores for the symbol
                let (db_entries, score_map) = {
                    let meta = meta_db.lock().await;
                    let entries = meta
                        .list_projects()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|p| p.enabled && project_names.contains(&p.name))
                        .map(|p| (p.name.clone(), std::path::PathBuf::from(&p.db_path)))
                        .collect::<Vec<_>>();
                    // Build (proj_a, proj_b) -> similarity_score lookup from pattern_cross_refs
                    let mut scores = std::collections::HashMap::new();
                    for (proj, _) in &entries {
                        for xref in meta
                            .query_cross_refs_for_symbol(proj, &name)
                            .unwrap_or_default()
                        {
                            scores.insert(
                                (xref.project_a.clone(), xref.project_b.clone()),
                                xref.similarity_score,
                            );
                            scores.insert(
                                (xref.project_b.clone(), xref.project_a.clone()),
                                xref.similarity_score,
                            );
                        }
                    }
                    (entries, scores)
                };

                let name_for_query = name.clone();
                let json_comparisons = tokio::task::spawn_blocking(move || {
                    let mut arr: Vec<serde_json::Value> = Vec::new();
                    for (project, db_path) in &db_entries {
                        let mut graph = match magellan::CodeGraph::open(db_path) {
                            Ok(g) => g,
                            Err(_) => continue,
                        };
                        let detail = match magellan::context::get_symbol_detail(
                            &mut graph,
                            &name_for_query,
                            None,
                        ) {
                            Ok(d) => d,
                            Err(_) => continue,
                        };
                        // Find the best similarity score against any other requested project
                        let best_score: Option<f64> = db_entries
                            .iter()
                            .filter(|(other, _)| other != project)
                            .filter_map(|(other, _)| {
                                score_map.get(&(project.clone(), other.clone())).copied()
                            })
                            .reduce(f64::max);
                        let mut entry = json!({
                            "project": project,
                            "name": &detail.name,
                            "kind": &detail.kind,
                            "file_path": &detail.file,
                            "start_line": detail.line,
                            "callers": detail.callers.iter().map(|c| json!({
                                "name": &c.name, "file": &c.file, "line": c.line,
                            })).collect::<Vec<_>>(),
                            "callees": detail.callees.iter().map(|c| json!({
                                "name": &c.name, "file": &c.file, "line": c.line,
                            })).collect::<Vec<_>>(),
                        });
                        if let Some(score) = best_score {
                            entry["similarity_score"] = serde_json::json!(score);
                        }
                        arr.push(entry);
                    }
                    Ok::<Vec<serde_json::Value>, anyhow::Error>(arr)
                })
                .await;

                match json_comparisons {
                    Ok(Ok(arr)) => Ok(super::types::ServiceResponse::ok(
                        id,
                        json!({ "query": name, "comparisons": arr }),
                    )
                    .into_val()),
                    Ok(Err(e)) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32003,
                        format!("Query error: {}", e),
                    )
                    .into_val()),
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Blocking task panic: {}", e),
                    )
                    .into_val()),
                }
            }

            "query.suggest" => {
                // Params: from_project (required), name (required), to_project (optional filter)
                let from_project = params
                    .get("from_project")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let to_project: Option<String> = params
                    .get("to_project")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let refs = {
                    let meta = meta_db.lock().await;
                    meta.query_cross_refs_for_symbol(&from_project, &name)
                        .unwrap_or_default()
                };

                let suggestions: Vec<serde_json::Value> = refs
                    .into_iter()
                    .filter(|r| to_project.as_deref().is_none_or(|tp| r.project_b == tp))
                    .map(|r| {
                        json!({
                            "project": r.project_b,
                            "symbol": r.symbol_b,
                            "file": r.file_b,
                            "similarity_score": r.similarity_score,
                        })
                    })
                    .collect();

                Ok(super::types::ServiceResponse::ok(
                    id,
                    json!({ "from_project": from_project, "name": name, "suggestions": suggestions }),
                )
                .into_val())
            }

            "query.build-index" => {
                // Collect all enabled project (name, db_path) pairs from meta.db
                let db_entries: Vec<(String, std::path::PathBuf)> = {
                    let meta = meta_db.lock().await;
                    meta.list_projects()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|p| p.enabled)
                        .map(|p| (p.name.clone(), std::path::PathBuf::from(&p.db_path)))
                        .collect()
                };

                let meta_db_clone = Arc::clone(&meta_db);
                let result = tokio::task::spawn_blocking(move || {
                    let mut meta = meta_db_clone.blocking_lock();
                    crate::service::structural::build_cross_refs(&mut meta, &db_entries, 0.70)
                })
                .await;

                match result {
                    Ok(Ok(count)) => Ok(super::types::ServiceResponse::ok(
                        id,
                        json!({ "pairs_inserted": count }),
                    )
                    .into_val()),
                    Ok(Err(e)) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32003,
                        format!("Build index error: {}", e),
                    )
                    .into_val()),
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Blocking task panic: {}", e),
                    )
                    .into_val()),
                }
            }

            "evolve.analyze" => {
                let project_filter: Option<String> = params
                    .get("project")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let limit: Option<usize> = params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|u| u as usize);

                let meta_db_clone = Arc::clone(&meta_db);
                let result = tokio::task::spawn_blocking(move || {
                    let meta = meta_db_clone.blocking_lock();
                    meta.analyze_hotspots(project_filter.as_deref(), limit)
                })
                .await;

                match result {
                    Ok(Ok(candidates)) => {
                        let items: Vec<serde_json::Value> = candidates
                            .into_iter()
                            .map(|c| {
                                json!({
                                    "project": c.project,
                                    "symbol": c.symbol,
                                    "file": c.file,
                                    "rank_score": c.rank_score,
                                    "loc": c.loc,
                                    "fan_in": c.fan_in,
                                    "complexity": c.cyclomatic_complexity,
                                })
                            })
                            .collect();
                        Ok(
                            super::types::ServiceResponse::ok(id, json!({ "candidates": items }))
                                .into_val(),
                        )
                    }
                    Ok(Err(e)) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32004,
                        format!("Analyze error: {}", e),
                    )
                    .into_val()),
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Blocking task panic: {}", e),
                    )
                    .into_val()),
                }
            }

            "evolve.retrieve" => {
                let project = params
                    .get("project")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let symbol = params
                    .get("symbol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let to_project: Option<String> = params
                    .get("to_project")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let limit: Option<usize> = params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|u| u as usize);

                let refs = {
                    let meta = meta_db.lock().await;
                    meta.query_cross_refs_for_symbol(&project, &symbol)
                        .unwrap_or_default()
                };

                let mut analogues: Vec<serde_json::Value> = refs
                    .into_iter()
                    .filter(|r| to_project.as_deref().is_none_or(|tp| r.project_b == tp))
                    .map(|r| {
                        json!({
                            "project": r.project_b,
                            "symbol": r.symbol_b,
                            "file": r.file_b,
                            "similarity_score": r.similarity_score,
                        })
                    })
                    .collect();

                if let Some(l) = limit {
                    analogues.truncate(l);
                }

                Ok(super::types::ServiceResponse::ok(
                    id,
                    json!({ "project": project, "symbol": symbol, "analogues": analogues }),
                )
                .into_val())
            }

            "evolve.propose" => {
                let project = params
                    .get("project")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let symbol = params
                    .get("symbol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let candidate_id = params
                    .get("candidate_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let patch_diff = params
                    .get("patch_diff")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let analogue = params.get("analogue").cloned();

                if project.is_empty() || symbol.is_empty() {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32602,
                        "Missing 'project' or 'symbol' param".to_string(),
                    )
                    .into_val());
                }

                let candidate_id = if candidate_id.is_empty() {
                    format!("{}/{}-{}", project, symbol, now_secs())
                } else {
                    candidate_id
                };

                let db_path = {
                    let reg = registry.lock().await;
                    reg.find(&project).map(|e| e.db.clone())
                };
                let Some(db_path) = db_path else {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32005,
                        format!("Project '{}' not found in registry", project),
                    )
                    .into_val());
                };

                let properties = json!({"patch_diff": patch_diff, "analogue": analogue});
                if let Err(e) = super::candidates::insert_candidate_fact(
                    &db_path,
                    &candidate_id,
                    "Symbol",
                    &symbol,
                    "proposes-improvement",
                    &properties.to_string(),
                    "pending",
                ) {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Failed to persist candidate: {}", e),
                    )
                    .into_val());
                }

                Ok(super::types::ServiceResponse::ok(
                    id,
                    json!({
                        "candidate_id": candidate_id,
                        "status": "pending",
                        "project": project,
                        "symbol": symbol
                    }),
                )
                .into_val())
            }

            "evolve.candidates" => {
                let project = params
                    .get("project")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let status_filter: Option<String> = params
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let limit: Option<usize> = params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|u| u as usize);

                if project.is_empty() {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32602,
                        "Missing 'project' param".to_string(),
                    )
                    .into_val());
                }

                let db_path = {
                    let reg = registry.lock().await;
                    reg.find(&project).map(|e| e.db.clone())
                };
                let Some(db_path) = db_path else {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32005,
                        format!("Project '{}' not found in registry", project),
                    )
                    .into_val());
                };

                match super::candidates::list_candidates(&db_path, status_filter.as_deref(), limit)
                {
                    Ok(recs) => {
                        let items: Vec<serde_json::Value> = recs
                            .into_iter()
                            .map(|r| {
                                json!({
                                    "candidate_id": r.candidate_id,
                                    "status": r.status,
                                    "properties": r.properties_json,
                                    "created_at": r.created_at,
                                })
                            })
                            .collect();
                        Ok(super::types::ServiceResponse::ok(
                            id,
                            json!({ "project": project, "candidates": items }),
                        )
                        .into_val())
                    }
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Failed to list candidates: {}", e),
                    )
                    .into_val()),
                }
            }

            "evolve.promote" => {
                let project = params
                    .get("project")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let candidate_id = params
                    .get("candidate_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if project.is_empty() || candidate_id.is_empty() {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32602,
                        "Missing 'project' or 'candidate_id' param".to_string(),
                    )
                    .into_val());
                }

                let db_path = {
                    let reg = registry.lock().await;
                    reg.find(&project).map(|e| e.db.clone())
                };
                let Some(db_path) = db_path else {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32005,
                        format!("Project '{}' not found in registry", project),
                    )
                    .into_val());
                };

                match super::candidates::update_candidate_status(
                    &db_path,
                    &candidate_id,
                    "promoted",
                    None,
                ) {
                    Ok(0) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32006,
                        format!("Candidate '{}' not found", candidate_id),
                    )
                    .into_val()),
                    Ok(_) => Ok(super::types::ServiceResponse::ok(
                        id,
                        json!({"candidate_id": candidate_id, "status": "promoted"}),
                    )
                    .into_val()),
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Failed to promote candidate: {}", e),
                    )
                    .into_val()),
                }
            }

            "evolve.reject" => {
                let project = params
                    .get("project")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let candidate_id = params
                    .get("candidate_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let reason: Option<String> = params
                    .get("rejection_reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                if project.is_empty() || candidate_id.is_empty() {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32602,
                        "Missing 'project' or 'candidate_id' param".to_string(),
                    )
                    .into_val());
                }

                let db_path = {
                    let reg = registry.lock().await;
                    reg.find(&project).map(|e| e.db.clone())
                };
                let Some(db_path) = db_path else {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32005,
                        format!("Project '{}' not found in registry", project),
                    )
                    .into_val());
                };

                match super::candidates::update_candidate_status(
                    &db_path,
                    &candidate_id,
                    "rejected",
                    reason.as_deref(),
                ) {
                    Ok(0) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32006,
                        format!("Candidate '{}' not found", candidate_id),
                    )
                    .into_val()),
                    Ok(_) => Ok(super::types::ServiceResponse::ok(
                        id,
                        json!({"candidate_id": candidate_id, "status": "rejected"}),
                    )
                    .into_val()),
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Failed to reject candidate: {}", e),
                    )
                    .into_val()),
                }
            }

            "evolve.verify" => {
                let project = params
                    .get("project")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let candidate_id = params
                    .get("candidate_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if project.is_empty() || candidate_id.is_empty() {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32602,
                        "Missing 'project' or 'candidate_id' param".to_string(),
                    )
                    .into_val());
                }

                let pair = {
                    let reg = registry.lock().await;
                    reg.find(&project).map(|e| (e.db.clone(), e.root.clone()))
                };
                let Some((db_path, project_root)) = pair else {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32005,
                        format!("Project '{}' not found in registry", project),
                    )
                    .into_val());
                };

                let rec = match super::candidates::get_candidate_by_id(&db_path, &candidate_id) {
                    Ok(Some(r)) => r,
                    Ok(None) => {
                        return Ok(super::types::ServiceResponse::err(
                            id,
                            -32006,
                            format!("Candidate '{}' not found", candidate_id),
                        )
                        .into_val());
                    }
                    Err(e) => {
                        return Ok(super::types::ServiceResponse::err(
                            id,
                            -32603,
                            format!("DB error: {}", e),
                        )
                        .into_val());
                    }
                };

                // Extract patch_diff from properties_json
                let patch_diff = serde_json::from_str::<serde_json::Value>(&rec.properties_json)
                    .ok()
                    .and_then(|v| {
                        v.get("patch_diff")
                            .and_then(|p| p.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();

                if patch_diff.is_empty() {
                    return Ok(super::types::ServiceResponse::err(
                        id,
                        -32602,
                        "Candidate has no patch_diff".to_string(),
                    )
                    .into_val());
                }

                let result = tokio::task::spawn_blocking(move || {
                    super::verify::verify_candidate(&project_root, &patch_diff)
                })
                .await;

                match result {
                    Ok(Ok(vr)) => {
                        let status = if vr.passed { "verified" } else { "rejected" };
                        let _ = super::candidates::update_candidate_status(
                            &db_path,
                            &candidate_id,
                            status,
                            None,
                        );
                        Ok(super::types::ServiceResponse::ok(
                            id,
                            json!({
                                "candidate_id": candidate_id,
                                "status": status,
                                "passed": vr.passed,
                                "exit_code": vr.exit_code,
                                "stdout": vr.stdout,
                                "stderr": vr.stderr,
                            }),
                        )
                        .into_val())
                    }
                    Ok(Err(e)) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Verify error: {}", e),
                    )
                    .into_val()),
                    Err(e) => Ok(super::types::ServiceResponse::err(
                        id,
                        -32603,
                        format!("Blocking task panic: {}", e),
                    )
                    .into_val()),
                }
            }

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
