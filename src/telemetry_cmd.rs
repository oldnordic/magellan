//! Telemetry command implementation
//!
//! Queries performance telemetry events recorded during command execution.

use anyhow::Result;
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;

/// Phase duration response for JSON output
#[derive(Serialize)]
struct PhaseDuration {
    phase: String,
    duration_ms: i64,
    duration_ns: i64,
}

/// Telemetry event response for JSON output
#[derive(Serialize)]
struct TelemetryEventJson {
    execution_id: String,
    event_type: String,
    event_name: String,
    timestamp_ns: i64,
    duration_ns: Option<i64>,
    value: Option<f64>,
    metadata: Option<serde_json::Value>,
}

/// Run the telemetry command
///
/// Queries telemetry events from the database.
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `recent` - Show recent events
/// * `phases` - Show phase durations for a specific execution ID
/// * `limit` - Maximum number of events to show
/// * `output_format` - Output format (Human, Json, Pretty)
///
/// # Returns
/// Result indicating success or failure
pub fn run_telemetry(
    db_path: PathBuf,
    recent: bool,
    phases: Option<String>,
    limit: usize,
    output_format: OutputFormat,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;

    if let Some(exec_id) = phases {
        // Show phase durations for a specific execution
        let durations = graph.telemetry().get_phase_durations(&exec_id)?;

        match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let phases_json: Vec<_> = durations
                    .iter()
                    .map(|(phase_name, duration_ns)| PhaseDuration {
                        phase: phase_name.clone(),
                        duration_ms: duration_ns / 1_000_000,
                        duration_ns: *duration_ns,
                    })
                    .collect();

                let response = JsonResponse::new(
                    json!({
                        "execution_id": exec_id,
                        "phases": phases_json,
                        "total_phases": durations.len(),
                    }),
                    &exec_id,
                );
                output_json(&response, output_format)?;
            }
            OutputFormat::Human => {
                println!("Phase durations for execution: {}", exec_id);
                println!("{:<30} {:>12}", "Phase", "Duration (ms)");
                println!("{}", "-".repeat(45));
                for (phase_name, duration_ns) in &durations {
                    let duration_ms = *duration_ns as f64 / 1_000_000.0;
                    println!("{:<30} {:>12.2}", phase_name, duration_ms);
                }
                println!("\nTotal phases: {}", durations.len());
            }
        }
    } else if recent {
        // Show recent events
        let events = graph.telemetry().get_recent_events(limit)?;

        match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let events_json: Vec<_> = events
                    .iter()
                    .map(|e| TelemetryEventJson {
                        execution_id: e.execution_id.clone(),
                        event_type: format!("{:?}", e.event_type),
                        event_name: e.event_name.clone(),
                        timestamp_ns: e.timestamp_ns,
                        duration_ns: e.duration_ns,
                        value: e.value,
                        metadata: e.metadata.clone(),
                    })
                    .collect();

                let exec_id = format!("telemetry_{}", magellan::output::generate_execution_id());
                let response = JsonResponse::new(
                    json!({
                        "events": events_json,
                        "count": events.len(),
                        "limit": limit,
                    }),
                    &exec_id,
                );
                output_json(&response, output_format)?;
            }
            OutputFormat::Human => {
                println!("Recent telemetry events (limit: {})", limit);
                println!(
                    "{:<36} {:<12} {:<20} {:>16} {:>12}",
                    "Execution ID", "Type", "Name", "Timestamp", "Value"
                );
                println!("{}", "-".repeat(100));
                for e in &events {
                    let type_str = format!("{:?}", e.event_type);
                    let value_str = if let Some(v) = e.value {
                        format!("{:.2}", v)
                    } else {
                        "-".to_string()
                    };
                    println!(
                        "{:<36} {:<12} {:<20} {:>16} {:>12}",
                        e.execution_id, type_str, e.event_name, e.timestamp_ns, value_str,
                    );
                }
                println!("\nShowing {} events", events.len());
            }
        }
    } else {
        // Default: show usage
        println!("Usage: magellan telemetry [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --recent           Show recent telemetry events");
        println!("  --phases <EXEC_ID> Show phase durations for an execution");
        println!("  --limit <N>        Limit number of results (default: 20)");
        println!("  --db <PATH>        Database path");
        println!("  --output <FORMAT>  Output format: human, json, pretty");
        println!();
        println!("Examples:");
        println!("  magellan telemetry --recent");
        println!("  magellan telemetry --phases exec_abc123 --limit 50");
    }

    Ok(())
}
