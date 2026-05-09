//! Candidate fact command implementation
//!
//! Submits, validates, lists, and manages candidate facts for graph memory.

use anyhow::{Context, Result};
use magellan::graph::candidate_fact::{self, CandidateFact, CandidateStatus};
use magellan::output::{generate_execution_id, output_json, JsonResponse, OutputFormat};
use rusqlite::Connection;
use std::path::PathBuf;

/// Run the candidate fact command
pub fn run_candidate_fact(
    db_path: PathBuf,
    action: CandidateFactAction,
    output_format: OutputFormat,
) -> Result<()> {
    let exec_id = generate_execution_id();

    let conn = Connection::open(&db_path)
        .with_context(|| format!("open database: {}", db_path.display()))?;

    // Ensure both schemas exist (candidate_facts depends on source_documents FK)
    magellan::graph::source_inventory::ensure_schema(&conn)
        .context("ensure source inventory schema")?;
    candidate_fact::ensure_schema(&conn).context("ensure candidate fact schema")?;

    match action {
        CandidateFactAction::Submit { fact } => {
            let id = candidate_fact::insert(&conn, &fact)
                .with_context(|| format!("insert candidate fact: {}", fact.candidate_id))?;

            match output_format {
                OutputFormat::Json | OutputFormat::Pretty => {
                    let response = SubmitResponse {
                        candidate_id: fact.candidate_id,
                        inserted_id: id,
                        status: "submitted".to_string(),
                    };
                    let json_response = JsonResponse::new(response, &exec_id);
                    output_json(&json_response, output_format)?;
                }
                OutputFormat::Human => {
                    println!(
                        "Submitted candidate fact {} (db id: {})",
                        fact.candidate_id, id
                    );
                }
            }
        }

        CandidateFactAction::Validate { candidate_id } => {
            let fact = candidate_fact::find_by_id(&conn, &candidate_id)
                .with_context(|| format!("find candidate: {}", candidate_id))?
                .ok_or_else(|| anyhow::anyhow!("Candidate fact not found: {}", candidate_id))?;

            let result = candidate_fact::validate_ontology(&fact);

            // Update status based on validation result
            let new_status = if result.accepted {
                CandidateStatus::Accepted
            } else {
                CandidateStatus::Rejected
            };
            let reason = if result.accepted {
                None
            } else {
                Some(
                    result
                        .errors
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            };

            candidate_fact::update_status(
                &conn,
                &candidate_id,
                new_status.clone(),
                reason.as_deref(),
            )
            .with_context(|| format!("update status for: {}", candidate_id))?;

            match output_format {
                OutputFormat::Json | OutputFormat::Pretty => {
                    let response = ValidateResponse {
                        candidate_id,
                        accepted: result.accepted,
                        errors: result.errors.iter().map(|e| e.to_string()).collect(),
                        warnings: result.warnings,
                        new_status: new_status.as_str().to_string(),
                    };
                    let json_response = JsonResponse::new(response, &exec_id);
                    output_json(&json_response, output_format)?;
                }
                OutputFormat::Human => {
                    if result.accepted {
                        println!("Candidate {}: ACCEPTED", candidate_id);
                    } else {
                        println!("Candidate {}: REJECTED", candidate_id);
                        for err in &result.errors {
                            println!("  - {}", err);
                        }
                    }
                    if !result.warnings.is_empty() {
                        println!("Warnings:");
                        for warn in &result.warnings {
                            println!("  - {}", warn);
                        }
                    }
                }
            }
        }

        CandidateFactAction::List { status, limit } => {
            let facts = candidate_fact::list_by_status(&conn, status, limit)
                .context("list candidate facts")?;

            match output_format {
                OutputFormat::Json | OutputFormat::Pretty => {
                    let response = ListResponse { facts };
                    let json_response = JsonResponse::new(response, &exec_id);
                    output_json(&json_response, output_format)?;
                }
                OutputFormat::Human => {
                    if facts.is_empty() {
                        println!("No candidate facts found.");
                    } else {
                        println!("Candidate facts ({}):", facts.len());
                        for fact in facts {
                            let obj_str = match (&fact.object_type, &fact.object_key) {
                                (Some(t), Some(k)) => format!(" -> {}:{}", t, k),
                                _ => "".to_string(),
                            };
                            println!(
                                "  [{}] {}:{} {} {}{}",
                                fact.status.as_str(),
                                fact.subject_type,
                                fact.subject_key,
                                fact.predicate,
                                obj_str,
                                fact.rejection_reason
                                    .as_ref()
                                    .map(|r| format!(" (reason: {})", r))
                                    .unwrap_or_default()
                            );
                        }
                    }
                }
            }
        }

        CandidateFactAction::ReviewQueue { limit } => {
            let queue = candidate_fact::review_queue(&conn, limit).context("get review queue")?;

            match output_format {
                OutputFormat::Json | OutputFormat::Pretty => {
                    let response = ReviewQueueResponse { queue };
                    let json_response = JsonResponse::new(response, &exec_id);
                    output_json(&json_response, output_format)?;
                }
                OutputFormat::Human => {
                    if queue.is_empty() {
                        println!("Review queue is empty.");
                    } else {
                        println!("Review queue ({} items):", queue.len());
                        for fact in queue {
                            println!(
                                "  [{}] {}: {}:{} {} — {}",
                                fact.status.as_str(),
                                fact.candidate_id,
                                fact.subject_type,
                                fact.subject_key,
                                fact.predicate,
                                fact.rejection_reason
                                    .as_deref()
                                    .unwrap_or("no reason given")
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Actions for the candidate-fact command
#[derive(Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "CLI action enum: size differences expected"
)]
pub enum CandidateFactAction {
    Submit {
        fact: CandidateFact,
    },
    Validate {
        candidate_id: String,
    },
    List {
        status: Option<CandidateStatus>,
        limit: Option<usize>,
    },
    ReviewQueue {
        limit: Option<usize>,
    },
}

#[derive(Debug, serde::Serialize)]
struct SubmitResponse {
    candidate_id: String,
    inserted_id: i64,
    status: String,
}

#[derive(Debug, serde::Serialize)]
struct ValidateResponse {
    candidate_id: String,
    accepted: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
    new_status: String,
}

#[derive(Debug, serde::Serialize)]
struct ListResponse {
    facts: Vec<CandidateFact>,
}

#[derive(Debug, serde::Serialize)]
struct ReviewQueueResponse {
    queue: Vec<CandidateFact>,
}
