//! Service command handler: CLI control interface for the daemon

use anyhow::{Context, Result};
use serde_json::json;
use std::path::PathBuf;

use crate::OutputFormat;

/// Subcommands for `magellan service <subcommand>`
#[derive(Debug)]
pub enum ServiceAction {
    Start,
    Stop,
    List,
    Register {
        root: PathBuf,
        name: Option<String>,
        include: Vec<String>,
        exclude: Vec<String>,
    },
    Unregister {
        name: String,
    },
    Pause {
        name: String,
    },
    Resume {
        name: String,
    },
    Status,
    Stats,
    Events {
        project: Option<String>,
        event_type: Option<String>,
        since_hours: Option<u64>,
        limit: usize,
        json_output: bool,
    },
}

/// Run a service action (CLI-side; talks to daemon via unix socket)
pub async fn run(action: ServiceAction, _output_format: OutputFormat) -> Result<()> {
    match action {
        ServiceAction::Start => {
            // Phase 0: spawn daemon as subprocess, then return
            let exe = std::env::current_exe().context("Cannot locate magellan binary")?;
            let mut child = tokio::process::Command::new(exe)
                .arg("service-daemon")
                .arg("--background")
                .spawn()
                .context("Failed to start daemon process")?;

            // Wait briefly for socket to appear
            let socket = PathBuf::from(crate::service::socket_path());
            for _ in 0..50 {
                if socket.exists() {
                    println!("Daemon started. Socket: {}", socket.display());
                    return Ok(());
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }

            // Socket didn't appear — report status
            let status = child.try_wait()?;
            match status {
                Some(s) if s.success() => println!("Daemon exited unexpectedly (success)"),
                Some(s) => println!("Daemon exited with code: {:?}", s.code()),
                None => println!(
                    "Daemon starting (PID: {:?}) Socket not yet ready.",
                    child.id()
                ),
            }
            Ok(())
        }

        ServiceAction::Stop => {
            let req = json!({
                "id": "stop-1",
                "method": "stop"
            });
            let resp = crate::service::send_request(req).await?;
            if let Some(err) = resp.get("error") {
                println!("Daemon error: {}", err);
            } else {
                println!("Daemon stop signaled.");
            }
            Ok(())
        }

        ServiceAction::List => {
            let req = json!({
                "id": "list-1",
                "method": "list"
            });
            let resp = crate::service::send_request(req).await?;
            if let Some(result) = resp.get("result") {
                if let Some(projects) = result.get("projects") {
                    println!("Projects: {}", serde_json::to_string_pretty(projects)?);
                } else {
                    println!("{}", serde_json::to_string_pretty(result)?);
                }
            } else if let Some(err) = resp.get("error") {
                println!("Error: {}", err);
            }
            Ok(())
        }

        ServiceAction::Register {
            root,
            name,
            include,
            exclude,
        } => {
            // If no name given, derive from root
            let name = name.unwrap_or_else(|| {
                crate::service::registry::Registry::disambiguate_name(
                    &[],
                    root.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("project"),
                )
            });
            let mut req = json!({
                "id": "reg-1",
                "method": "register",
                "name": name,
                "root": root.to_string_lossy(),
                "source": "manual"
            });
            if !include.is_empty() {
                req["include"] = json!(include);
            }
            if !exclude.is_empty() {
                req["exclude"] = json!(exclude);
            }
            let resp = crate::service::send_request(req).await?;
            if let Some(result) = resp.get("result") {
                println!("Registered: {}", result);
            } else if let Some(err) = resp.get("error") {
                println!("Error: {}", err);
            }
            Ok(())
        }

        ServiceAction::Unregister { name } => {
            let req = json!({
                "id": "unreg-1",
                "method": "unregister",
                "name": name
            });
            let resp = crate::service::send_request(req).await?;
            if let Some(result) = resp.get("result") {
                println!("Unregistered: {}", result);
            } else if let Some(err) = resp.get("error") {
                println!("Error: {}", err);
            }
            Ok(())
        }

        ServiceAction::Pause { name } => {
            let req = json!({
                "id": "pause-1",
                "method": "pause",
                "name": name
            });
            let resp = crate::service::send_request(req).await?;
            if let Some(result) = resp.get("result") {
                println!("Paused: {}", result);
            } else if let Some(err) = resp.get("error") {
                println!("Error: {}", err);
            }
            Ok(())
        }

        ServiceAction::Resume { name } => {
            let req = json!({
                "id": "resume-1",
                "method": "resume",
                "name": name
            });
            let resp = crate::service::send_request(req).await?;
            if let Some(result) = resp.get("result") {
                println!("Resumed: {}", result);
            } else if let Some(err) = resp.get("error") {
                println!("Error: {}", err);
            }
            Ok(())
        }

        ServiceAction::Status => {
            let req = json!({
                "id": "status-1",
                "method": "status"
            });
            let resp = crate::service::send_request(req).await?;
            if let Some(result) = resp.get("result") {
                println!("{}", serde_json::to_string_pretty(result)?);
            } else if let Some(err) = resp.get("error") {
                println!("Error: {}", err);
            }
            Ok(())
        }

        ServiceAction::Stats => {
            let req = json!({
                "id": "stats-1",
                "method": "stats"
            });
            let resp = crate::service::send_request(req).await?;
            if let Some(result) = resp.get("result") {
                println!("{}", serde_json::to_string_pretty(result)?);
            } else if let Some(err) = resp.get("error") {
                println!("Error: {}", err);
            }
            Ok(())
        }

        ServiceAction::Events {
            project,
            event_type,
            since_hours,
            limit,
            json_output,
        } => {
            let req = json!({
                "id": "events-1",
                "method": "events",
                "project": project,
                "event_type": event_type,
                "since_hours": since_hours,
                "limit": limit,
            });
            let resp = crate::service::send_request(req).await?;
            if let Some(err) = resp.get("error") {
                println!("Error: {}", err);
                return Ok(());
            }
            let events = resp
                .get("result")
                .and_then(|r| r.get("events"))
                .and_then(|v| v.as_array());
            let Some(events) = events else {
                println!("No events found.");
                return Ok(());
            };
            if json_output {
                println!("{}", serde_json::to_string_pretty(events)?);
            } else {
                if events.is_empty() {
                    println!("No events found.");
                    return Ok(());
                }
                println!("ID     TYPE                 PROJECT              FILE                                     TIME");
                for ev in events {
                    println!(
                        "{:<6} {:<20} {:<20} {:<40} {}",
                        ev.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                        ev.get("event_type").and_then(|v| v.as_str()).unwrap_or("-"),
                        ev.get("project_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("-"),
                        ev.get("file_path").and_then(|v| v.as_str()).unwrap_or("-"),
                        ev.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0),
                    );
                }
            }
            Ok(())
        }
    }
}
