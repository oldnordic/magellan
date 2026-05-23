//! Admin socket: JSON-RPC request/response handler for daemon control

use anyhow::{Context, Result};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, Mutex};

use super::registry::Registry;

pub struct AdminSocket;

impl AdminSocket {
    /// Handle a single client connection (one request per line)
    pub async fn handle_client(
        stream: UnixStream,
        registry: Arc<Mutex<Registry>>,
        meta_db: Arc<Mutex<super::meta_db::MetaDb>>,
        batch_tx: mpsc::Sender<super::types::TaggedBatch>,
    ) -> Result<()> {
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half).lines();

        while let Some(line) = reader.next_line().await? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let response =
                match Self::dispatch(line, registry.clone(), meta_db.clone(), batch_tx.clone())
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
    ) -> Result<serde_json::Value> {
        let req: super::types::ServiceRequest =
            serde_json::from_str(line).context("Invalid JSON-RPC request")?;

        let id = req.id.clone();
        let method = req.method.clone();
        let params = req.params; // moves params out

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

                let entry = super::types::ProjectEntry::new(name.clone(), root, db, source);
                let mut reg = registry.lock().await;
                reg.register(entry)?;
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
                let mut reg = registry.lock().await;
                let ok = reg.resume(name)?;
                Ok(super::types::ServiceResponse::ok(id, json!({ "resumed": ok })).into_val())
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

            _ => Ok(super::types::ServiceResponse::not_implemented(id, method).into_val()),
        }
    }
}
