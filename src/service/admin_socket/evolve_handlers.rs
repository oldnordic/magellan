use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::service::meta_db::MetaDb;
use crate::service::registry::Registry;
use crate::service::types::ServiceResponse;

type MetaDbHandle = Arc<Mutex<MetaDb>>;
type RegistryHandle = Arc<Mutex<Registry>>;

fn parse_optional_string(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

fn parse_required_string(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn parse_optional_usize(params: &Value, key: &str) -> Option<usize> {
    params.get(key).and_then(|v| v.as_u64()).map(|v| v as usize)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn find_project_db_path(registry: &RegistryHandle, project: &str) -> Option<PathBuf> {
    let reg = registry.lock().await;
    reg.find(project).map(|entry| entry.db.clone())
}

async fn find_project_db_and_root(
    registry: &RegistryHandle,
    project: &str,
) -> Option<(PathBuf, PathBuf)> {
    let reg = registry.lock().await;
    reg.find(project)
        .map(|entry| (entry.db.clone(), entry.root.clone()))
}

pub async fn handle_analyze(id: String, params: Value, meta_db: MetaDbHandle) -> Result<Value> {
    let project_filter = parse_optional_string(&params, "project");
    let limit = parse_optional_usize(&params, "limit");

    let meta_db_clone = Arc::clone(&meta_db);
    let result = tokio::task::spawn_blocking(move || {
        let meta = meta_db_clone.blocking_lock();
        meta.analyze_hotspots(project_filter.as_deref(), limit)
    })
    .await;

    Ok(match result {
        Ok(Ok(candidates)) => {
            let items: Vec<Value> = candidates
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
            ServiceResponse::ok(id, json!({ "candidates": items })).into_val()
        }
        Ok(Err(e)) => ServiceResponse::err(id, -32004, format!("Analyze error: {}", e)).into_val(),
        Err(e) => {
            ServiceResponse::err(id, -32603, format!("Blocking task panic: {}", e)).into_val()
        }
    })
}

pub async fn handle_retrieve(id: String, params: Value, meta_db: MetaDbHandle) -> Result<Value> {
    let project = parse_required_string(&params, "project");
    let symbol = parse_required_string(&params, "symbol");
    let to_project = parse_optional_string(&params, "to_project");
    let limit = parse_optional_usize(&params, "limit");

    let refs = {
        let meta = meta_db.lock().await;
        meta.query_cross_refs_for_symbol(&project, &symbol)
            .unwrap_or_default()
    };

    let mut analogues: Vec<Value> = refs
        .into_iter()
        .filter(|r| {
            to_project
                .as_deref()
                .is_none_or(|target| r.project_b == target)
        })
        .map(|r| {
            json!({
                "project": r.project_b,
                "symbol": r.symbol_b,
                "file": r.file_b,
                "similarity_score": r.similarity_score,
            })
        })
        .collect();

    if let Some(limit) = limit {
        analogues.truncate(limit);
    }

    Ok(ServiceResponse::ok(
        id,
        json!({ "project": project, "symbol": symbol, "analogues": analogues }),
    )
    .into_val())
}

pub async fn handle_propose(id: String, params: Value, registry: RegistryHandle) -> Result<Value> {
    let project = parse_required_string(&params, "project");
    let symbol = parse_required_string(&params, "symbol");
    let candidate_id = parse_required_string(&params, "candidate_id");
    let patch_diff = parse_required_string(&params, "patch_diff");
    let analogue = params.get("analogue").cloned();

    if project.is_empty() || symbol.is_empty() {
        return Ok(ServiceResponse::err(
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

    let Some(db_path) = find_project_db_path(&registry, &project).await else {
        return Ok(ServiceResponse::err(
            id,
            -32005,
            format!("Project '{}' not found in registry", project),
        )
        .into_val());
    };

    let properties = json!({"patch_diff": patch_diff, "analogue": analogue});
    if let Err(e) = crate::service::candidates::insert_candidate_fact(
        &db_path,
        &candidate_id,
        "Symbol",
        &symbol,
        "proposes-improvement",
        &properties.to_string(),
        "pending",
    ) {
        return Ok(
            ServiceResponse::err(id, -32603, format!("Failed to persist candidate: {}", e))
                .into_val(),
        );
    }

    Ok(ServiceResponse::ok(
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

pub async fn handle_candidates(
    id: String,
    params: Value,
    registry: RegistryHandle,
) -> Result<Value> {
    let project = parse_required_string(&params, "project");
    let status_filter = parse_optional_string(&params, "status");
    let limit = parse_optional_usize(&params, "limit");

    if project.is_empty() {
        return Ok(
            ServiceResponse::err(id, -32602, "Missing 'project' param".to_string()).into_val(),
        );
    }

    let Some(db_path) = find_project_db_path(&registry, &project).await else {
        return Ok(ServiceResponse::err(
            id,
            -32005,
            format!("Project '{}' not found in registry", project),
        )
        .into_val());
    };

    Ok(
        match crate::service::candidates::list_candidates(&db_path, status_filter.as_deref(), limit)
        {
            Ok(recs) => {
                let items: Vec<Value> = recs
                    .into_iter()
                    .map(|record| {
                        json!({
                            "candidate_id": record.candidate_id,
                            "status": record.status,
                            "properties": record.properties_json,
                            "created_at": record.created_at,
                        })
                    })
                    .collect();
                ServiceResponse::ok(id, json!({ "project": project, "candidates": items }))
                    .into_val()
            }
            Err(e) => ServiceResponse::err(id, -32603, format!("Failed to list candidates: {}", e))
                .into_val(),
        },
    )
}

pub async fn handle_promote(id: String, params: Value, registry: RegistryHandle) -> Result<Value> {
    let project = parse_required_string(&params, "project");
    let candidate_id = parse_required_string(&params, "candidate_id");

    if project.is_empty() || candidate_id.is_empty() {
        return Ok(ServiceResponse::err(
            id,
            -32602,
            "Missing 'project' or 'candidate_id' param".to_string(),
        )
        .into_val());
    }

    let Some(db_path) = find_project_db_path(&registry, &project).await else {
        return Ok(ServiceResponse::err(
            id,
            -32005,
            format!("Project '{}' not found in registry", project),
        )
        .into_val());
    };

    Ok(
        match crate::service::candidates::update_candidate_status(
            &db_path,
            &candidate_id,
            "promoted",
            None,
        ) {
            Ok(0) => ServiceResponse::err(
                id,
                -32006,
                format!("Candidate '{}' not found", candidate_id),
            )
            .into_val(),
            Ok(_) => ServiceResponse::ok(
                id,
                json!({"candidate_id": candidate_id, "status": "promoted"}),
            )
            .into_val(),
            Err(e) => {
                ServiceResponse::err(id, -32603, format!("Failed to promote candidate: {}", e))
                    .into_val()
            }
        },
    )
}

pub async fn handle_reject(id: String, params: Value, registry: RegistryHandle) -> Result<Value> {
    let project = parse_required_string(&params, "project");
    let candidate_id = parse_required_string(&params, "candidate_id");
    let reason = parse_optional_string(&params, "rejection_reason");

    if project.is_empty() || candidate_id.is_empty() {
        return Ok(ServiceResponse::err(
            id,
            -32602,
            "Missing 'project' or 'candidate_id' param".to_string(),
        )
        .into_val());
    }

    let Some(db_path) = find_project_db_path(&registry, &project).await else {
        return Ok(ServiceResponse::err(
            id,
            -32005,
            format!("Project '{}' not found in registry", project),
        )
        .into_val());
    };

    Ok(
        match crate::service::candidates::update_candidate_status(
            &db_path,
            &candidate_id,
            "rejected",
            reason.as_deref(),
        ) {
            Ok(0) => ServiceResponse::err(
                id,
                -32006,
                format!("Candidate '{}' not found", candidate_id),
            )
            .into_val(),
            Ok(_) => ServiceResponse::ok(
                id,
                json!({"candidate_id": candidate_id, "status": "rejected"}),
            )
            .into_val(),
            Err(e) => {
                ServiceResponse::err(id, -32603, format!("Failed to reject candidate: {}", e))
                    .into_val()
            }
        },
    )
}

pub async fn handle_verify(id: String, params: Value, registry: RegistryHandle) -> Result<Value> {
    let project = parse_required_string(&params, "project");
    let candidate_id = parse_required_string(&params, "candidate_id");

    if project.is_empty() || candidate_id.is_empty() {
        return Ok(ServiceResponse::err(
            id,
            -32602,
            "Missing 'project' or 'candidate_id' param".to_string(),
        )
        .into_val());
    }

    let Some((db_path, project_root)) = find_project_db_and_root(&registry, &project).await else {
        return Ok(ServiceResponse::err(
            id,
            -32005,
            format!("Project '{}' not found in registry", project),
        )
        .into_val());
    };

    let record = match crate::service::candidates::get_candidate_by_id(&db_path, &candidate_id) {
        Ok(Some(record)) => record,
        Ok(None) => {
            return Ok(ServiceResponse::err(
                id,
                -32006,
                format!("Candidate '{}' not found", candidate_id),
            )
            .into_val());
        }
        Err(e) => {
            return Ok(ServiceResponse::err(id, -32603, format!("DB error: {}", e)).into_val());
        }
    };

    let patch_diff = serde_json::from_str::<Value>(&record.properties_json)
        .ok()
        .and_then(|value| {
            value
                .get("patch_diff")
                .and_then(|patch| patch.as_str())
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_default();

    if patch_diff.is_empty() {
        return Ok(
            ServiceResponse::err(id, -32602, "Candidate has no patch_diff".to_string()).into_val(),
        );
    }

    let result = tokio::task::spawn_blocking(move || {
        crate::service::verify::verify_candidate(&project_root, &patch_diff)
    })
    .await;

    Ok(match result {
        Ok(Ok(verification)) => {
            let status = if verification.passed {
                "verified"
            } else {
                "rejected"
            };
            let _ = crate::service::candidates::update_candidate_status(
                &db_path,
                &candidate_id,
                status,
                None,
            );
            ServiceResponse::ok(
                id,
                json!({
                    "candidate_id": candidate_id,
                    "status": status,
                    "passed": verification.passed,
                    "exit_code": verification.exit_code,
                    "stdout": verification.stdout,
                    "stderr": verification.stderr,
                }),
            )
            .into_val()
        }
        Ok(Err(e)) => ServiceResponse::err(id, -32603, format!("Verify error: {}", e)).into_val(),
        Err(e) => {
            ServiceResponse::err(id, -32603, format!("Blocking task panic: {}", e)).into_val()
        }
    })
}
