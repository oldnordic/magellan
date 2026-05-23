//! Admin socket: JSON-RPC request/response handler for daemon control

use anyhow::{Context, Result};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use super::registry::Registry;

pub struct AdminSocket;

impl AdminSocket {
    /// Handle a single client connection (one request per line)
    pub async fn handle_client(stream: UnixStream, registry: Arc<Mutex<Registry>>) -> Result<()> {
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half).lines();

        while let Some(line) = reader.next_line().await? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let response = match Self::dispatch(line, registry.clone()).await {
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

    async fn dispatch(line: &str, registry: Arc<Mutex<Registry>>) -> Result<serde_json::Value> {
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
                // Phase 1: queue to dispatcher
                let batch = super::types::TaggedBatch {
                    project_name: tag,
                    paths,
                };
                // For now, acknowledge receipt; actual dispatch in Phase 1 worker_loop
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
            _ => Ok(super::types::ServiceResponse::not_implemented(id, method).into_val()),
        }
    }
}
