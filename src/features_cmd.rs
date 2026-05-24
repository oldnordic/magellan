//! Features command implementation for Magellan
//!
//! Reads active Cargo feature flags from the `magellan_meta` database table
//! and prints them in human-readable or JSON format.

use anyhow::{Context, Result};
use magellan::output::{generate_execution_id, output_json, JsonResponse};
use magellan::OutputFormat;
use std::collections::HashSet;
use std::path::PathBuf;

/// Run the `magellan features` command
///
/// # Arguments
/// * `db_path` — Path to the magellan database
/// * `output_format` — Human or JSON output
pub fn run_features(db_path: PathBuf, output_format: OutputFormat) -> Result<()> {
    let conn =
        rusqlite::Connection::open(&db_path).context("Failed to open database connection")?;

    // Query project name and metadata in a single row
    let (project_name, project_metadata_json): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT project_name, project_metadata FROM magellan_meta WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("Failed to query magellan_meta table")?;

    // Extract feature names from metadata JSON
    let features: Vec<String> = if let Some(json) = project_metadata_json {
        #[derive(serde::Deserialize)]
        struct Meta {
            features: Option<std::collections::HashMap<String, serde_json::Value>>,
        }
        let mut feats = Vec::new();
        if let Ok(meta) = serde_json::from_str::<Meta>(&json) {
            if let Some(map) = meta.features {
                let mut set: HashSet<String> = map.into_keys().collect();
                feats.extend(set.drain());
                feats.sort();
            }
        }
        feats
    } else {
        Vec::new()
    };

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            #[derive(serde::Serialize)]
            struct FeaturesResponse {
                project_name: Option<String>,
                features: Vec<String>,
                count: usize,
            }
            let exec_id = generate_execution_id();
            let response = JsonResponse::new(
                FeaturesResponse {
                    project_name,
                    features: features.clone(),
                    count: features.len(),
                },
                &exec_id,
            );
            output_json(&response, output_format)?;
        }
        OutputFormat::Human => {
            println!(
                "Features for project: {}",
                project_name.as_deref().unwrap_or("unknown")
            );
            println!();
            if features.is_empty() {
                println!("No features found in magellan_meta.");
                println!("  Run `magellan watch --root .` to populate project metadata.");
            } else {
                for f in &features {
                    println!("  - {}", f);
                }
                println!();
                println!("Total: {}", features.len());
            }
        }
    }

    Ok(())
}
