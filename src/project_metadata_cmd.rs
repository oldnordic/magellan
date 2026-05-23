//! Project metadata command implementation for Magellan
//!
//! Provides the ability to query and navigate Cargo.toml project metadata.

use anyhow::{Context, Result};
use magellan::output::{output_json, JsonResponse};
use magellan::OutputFormat;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct CargoManifestMetadata {
    package_name: Option<String>,
    features: HashMap<String, Vec<String>>,
    dependencies: Vec<String>,
    targets: Vec<String>,
}

/// Run project-metadata command
pub fn run_project_metadata(
    db_path: PathBuf,
    query: Option<String>,
    output_format: OutputFormat,
) -> Result<()> {
    if !db_path.exists() {
        anyhow::bail!("Database not found: {}", db_path.display());
    }

    let conn =
        rusqlite::Connection::open(&db_path).context("Failed to open database connection")?;

    // Query project name and metadata
    let (project_name, project_metadata_json): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT project_name, project_metadata FROM magellan_meta WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("Failed to query magellan_meta table")?;

    let parsed_metadata: Option<CargoManifestMetadata> = project_metadata_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok());

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            #[derive(Debug, Serialize)]
            struct MetadataResponse {
                project_name: Option<String>,
                metadata: Option<CargoManifestMetadata>,
            }
            let response = MetadataResponse {
                project_name,
                metadata: parsed_metadata,
            };
            let exec_id = magellan::output::generate_execution_id();
            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
            println!("Project Metadata Navigation");
            println!("===========================");
            println!();

            if let Some(name) = &project_name {
                println!("Project Name: {}", name);
            } else {
                println!("Project Name: (not set/detected)");
            }
            println!();

            if let Some(meta) = parsed_metadata {
                let filter = query.as_deref().unwrap_or("all");
                match filter {
                    "features" => {
                        println!("Project Features:");
                        if meta.features.is_empty() {
                            println!("  No custom features defined.");
                        } else {
                            for (feature, deps) in &meta.features {
                                println!("  - {}: {:?}", feature, deps);
                            }
                        }
                    }
                    "dependencies" => {
                        println!("Project Dependencies:");
                        if meta.dependencies.is_empty() {
                            println!("  No dependencies detected.");
                        } else {
                            for dep in &meta.dependencies {
                                println!("  - {}", dep);
                            }
                        }
                    }
                    "targets" => {
                        println!("Project Targets:");
                        if meta.targets.is_empty() {
                            println!("  No compilation targets detected.");
                        } else {
                            for target in &meta.targets {
                                println!("  - {}", target);
                            }
                        }
                    }
                    _ => {
                        // Print everything
                        println!("Project Targets:");
                        if meta.targets.is_empty() {
                            println!("  No targets detected.");
                        } else {
                            for target in &meta.targets {
                                println!("  - {}", target);
                            }
                        }
                        println!();

                        println!("Project Features:");
                        if meta.features.is_empty() {
                            println!("  No custom features defined.");
                        } else {
                            for (feature, deps) in &meta.features {
                                println!("  - {}: {:?}", feature, deps);
                            }
                        }
                        println!();

                        println!("Project Dependencies:");
                        if meta.dependencies.is_empty() {
                            println!("  No dependencies detected.");
                        } else {
                            for dep in &meta.dependencies {
                                println!("  - {}", dep);
                            }
                        }
                    }
                }
            } else {
                println!("No project metadata is stored in the database yet.");
                println!("Run 'magellan watch --root <DIR> --db <DB>' once to index project files and store metadata.");
            }
        }
    }

    Ok(())
}
