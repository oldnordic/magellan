use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::service::meta_db::MetaDb;
use crate::service::types::ServiceResponse;

type MetaDbHandle = Arc<Mutex<MetaDb>>;

fn parse_bool(params: &Value, key: &str) -> bool {
    params.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn parse_optional_usize(params: &Value, key: &str) -> Option<usize> {
    params.get(key).and_then(|v| v.as_u64()).map(|v| v as usize)
}

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

async fn enabled_db_paths(meta_db: &MetaDbHandle) -> Vec<PathBuf> {
    let meta = meta_db.lock().await;
    meta.list_projects()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.enabled)
        .map(|p| PathBuf::from(&p.db_path))
        .collect()
}

pub async fn handle_find(id: String, params: Value, meta_db: MetaDbHandle) -> Result<Value> {
    let name = parse_required_string(&params, "name");
    let file = parse_optional_string(&params, "file");
    let depth = parse_optional_usize(&params, "depth");
    let callers = parse_bool(&params, "callers");
    let callees = parse_bool(&params, "callees");
    let db_paths = enabled_db_paths(&meta_db).await;

    let json_matches = {
        let name_for_query = name.clone();
        tokio::task::spawn_blocking(move || {
            let mut ctx = match magellan::MultiDbContext::from_paths(&db_paths) {
                Ok(c) => c,
                Err(e) => return Err(anyhow::anyhow!("multi_db open error: {}", e)),
            };
            let results =
                ctx.search_symbol(&name_for_query, file.as_deref(), depth, callers, callees);
            let arr: Vec<Value> = results
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

    Ok(match json_matches {
        Ok(Ok(arr)) => ServiceResponse::ok(id, json!({ "query": name, "matches": arr })).into_val(),
        Ok(Err(e)) => ServiceResponse::err(id, -32003, format!("Query error: {}", e)).into_val(),
        Err(e) => {
            ServiceResponse::err(id, -32603, format!("Blocking task panic: {}", e)).into_val()
        }
    })
}

pub async fn handle_context(id: String, params: Value, meta_db: MetaDbHandle) -> Result<Value> {
    let name = parse_required_string(&params, "name");
    let file = parse_optional_string(&params, "file");
    let depth = parse_optional_usize(&params, "depth");
    let callers = parse_bool(&params, "callers");
    let callees = parse_bool(&params, "callees");
    let db_paths = enabled_db_paths(&meta_db).await;

    let name_for_query = name.clone();
    let json_matches = tokio::task::spawn_blocking(move || {
        let mut ctx = match magellan::MultiDbContext::from_paths(&db_paths) {
            Ok(c) => c,
            Err(e) => return Err(anyhow::anyhow!("multi_db open error: {}", e)),
        };
        let results = ctx.search_symbol(&name_for_query, file.as_deref(), depth, callers, callees);
        let arr: Vec<Value> = results
            .iter()
            .map(|m| {
                let caller_arr: Value = m
                    .callers
                    .as_ref()
                    .map(|cs| {
                        Value::Array(
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
                    .unwrap_or(Value::Null);
                let callee_arr: Value = m
                    .callees
                    .as_ref()
                    .map(|cs| {
                        Value::Array(
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
                    .unwrap_or(Value::Null);
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

    Ok(match json_matches {
        Ok(Ok(arr)) => ServiceResponse::ok(id, json!({ "query": name, "matches": arr })).into_val(),
        Ok(Err(e)) => ServiceResponse::err(id, -32003, format!("Query error: {}", e)).into_val(),
        Err(e) => {
            ServiceResponse::err(id, -32603, format!("Blocking task panic: {}", e)).into_val()
        }
    })
}

pub async fn handle_compare(id: String, params: Value, meta_db: MetaDbHandle) -> Result<Value> {
    let name = parse_required_string(&params, "name");
    let project_names: Vec<String> = params
        .get("projects")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let (db_entries, score_map) = {
        let meta = meta_db.lock().await;
        let entries = meta
            .list_projects()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.enabled && project_names.contains(&p.name))
            .map(|p| (p.name.clone(), PathBuf::from(&p.db_path)))
            .collect::<Vec<_>>();
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
        let mut arr: Vec<Value> = Vec::new();
        for (project, db_path) in &db_entries {
            let mut graph = match magellan::CodeGraph::open(db_path) {
                Ok(g) => g,
                Err(_) => continue,
            };
            let detail =
                match magellan::context::get_symbol_detail(&mut graph, &name_for_query, None) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
            let best_score: Option<f64> = db_entries
                .iter()
                .filter(|(other, _)| other != project)
                .filter_map(|(other, _)| score_map.get(&(project.clone(), other.clone())).copied())
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
                entry["similarity_score"] = json!(score);
            }
            arr.push(entry);
        }
        Ok::<Vec<Value>, anyhow::Error>(arr)
    })
    .await;

    Ok(match json_comparisons {
        Ok(Ok(arr)) => {
            ServiceResponse::ok(id, json!({ "query": name, "comparisons": arr })).into_val()
        }
        Ok(Err(e)) => ServiceResponse::err(id, -32003, format!("Query error: {}", e)).into_val(),
        Err(e) => {
            ServiceResponse::err(id, -32603, format!("Blocking task panic: {}", e)).into_val()
        }
    })
}

pub async fn handle_suggest(id: String, params: Value, meta_db: MetaDbHandle) -> Result<Value> {
    let from_project = parse_required_string(&params, "from_project");
    let name = parse_required_string(&params, "name");
    let to_project = parse_optional_string(&params, "to_project");

    let refs = {
        let meta = meta_db.lock().await;
        meta.query_cross_refs_for_symbol(&from_project, &name)
            .unwrap_or_default()
    };

    let suggestions: Vec<Value> = refs
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

    Ok(ServiceResponse::ok(
        id,
        json!({ "from_project": from_project, "name": name, "suggestions": suggestions }),
    )
    .into_val())
}

pub async fn handle_build_index(id: String, meta_db: MetaDbHandle) -> Result<Value> {
    let db_entries: Vec<(String, PathBuf)> = {
        let meta = meta_db.lock().await;
        meta.list_projects()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.enabled)
            .map(|p| (p.name.clone(), PathBuf::from(&p.db_path)))
            .collect()
    };

    let meta_db_clone = Arc::clone(&meta_db);
    let result = tokio::task::spawn_blocking(move || {
        let mut meta = meta_db_clone.blocking_lock();
        crate::service::structural::build_cross_refs(&mut meta, &db_entries, 0.70)
    })
    .await;

    Ok(match result {
        Ok(Ok(count)) => ServiceResponse::ok(id, json!({ "pairs_inserted": count })).into_val(),
        Ok(Err(e)) => {
            ServiceResponse::err(id, -32003, format!("Build index error: {}", e)).into_val()
        }
        Err(e) => {
            ServiceResponse::err(id, -32603, format!("Blocking task panic: {}", e)).into_val()
        }
    })
}
