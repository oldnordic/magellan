//! Collisions command implementation
//!
//! Enumerates ambiguous symbols that share the same FQN or display FQN.

use anyhow::Result;
use magellan::graph::query::{collision_groups, CollisionField};
use magellan::output::{
    generate_execution_id, output_json, CollisionCandidate, CollisionGroup, CollisionsResponse,
    JsonResponse, OutputFormat,
};
use magellan::CodeGraph;
use std::path::PathBuf;

/// Run the collisions command
///
/// Lists collision groups for a selected field (fqn, display_fqn, canonical_fqn).
pub fn run_collisions(
    db_path: PathBuf,
    field: CollisionField,
    limit: usize,
    output_format: OutputFormat,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();

    let mut args = vec!["collisions".to_string()];
    args.push("--db".to_string());
    args.push(db_path.to_string_lossy().to_string());
    args.push("--field".to_string());
    args.push(field.as_str().to_string());
    args.push("--limit".to_string());
    args.push(limit.to_string());

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path.to_string_lossy(),
    )?;

    let groups = collision_groups(&mut graph, field, limit)?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = CollisionsResponse {
                field: field.as_str().to_string(),
                groups: groups
                    .into_iter()
                    .map(|group| CollisionGroup {
                        field: group.field,
                        value: group.value,
                        count: group.count,
                        candidates: group
                            .candidates
                            .into_iter()
                            .map(|candidate| CollisionCandidate {
                                entity_id: candidate.entity_id,
                                symbol_id: candidate.symbol_id,
                                canonical_fqn: candidate.canonical_fqn,
                                display_fqn: candidate.display_fqn,
                                name: candidate.name,
                                file_path: candidate.file_path,
                            })
                            .collect(),
                    })
                    .collect(),
            };

            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
            if groups.is_empty() {
                println!("No collisions found for {}", field.as_str());
            } else {
                println!("Collisions by {}:", field.as_str());
                for group in groups {
                    println!();
                    println!("{} ({})", group.value, group.count);
                    for (idx, candidate) in group.candidates.iter().enumerate() {
                        let symbol_id = candidate.symbol_id.as_deref().unwrap_or("<none>");
                        let file_path = candidate.file_path.as_deref().unwrap_or("?");
                        let canonical = candidate.canonical_fqn.as_deref().unwrap_or("<none>");

                        println!("  [{}] {} {}", idx + 1, symbol_id, file_path);
                        println!("       {}", canonical);
                    }
                }
            }
        }
    }

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;

    Ok(())
}
