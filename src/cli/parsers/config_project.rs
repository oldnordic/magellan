use crate::cli::parsers::*;
use crate::cli::Command;
use crate::db_resolver::resolve_db_path;
use crate::service::registry::Registry;
use anyhow::{Context, Result};
use magellan::OutputFormat;
use std::path::PathBuf;

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

// ============================================================================
// Config Project Parsers
// ============================================================================

pub fn parse_config_show_args(args: &[String]) -> Result<Command> {
    let mut output_format = OutputFormat::Human;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => i += 1,
        }
    }
    Ok(Command::ConfigShow { output_format })
}

pub fn parse_config_init_args(args: &[String]) -> Result<Command> {
    let mut force = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--force" => {
                force = true;
                i += 1;
            }
            _ => i += 1,
        }
    }
    Ok(Command::ConfigInit { force })
}

pub fn parse_project_init_args(args: &[String]) -> Result<Command> {
    let mut path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--path" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--path requires an argument"));
                }
                path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => i += 1,
        }
    }
    Ok(Command::ProjectInit { path })
}

pub fn parse_delete_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut file_path: Option<PathBuf> = None;
    let mut root: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--db requires an argument"));
                }
                db_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--file" | "--path" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--file requires an argument"));
                }
                file_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--root" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--root requires an argument"));
                }
                root = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => i += 1,
        }
    }
    let db_path = resolve_db_path(db_path)?;
    let file_path = file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;
    Ok(Command::Delete {
        db_path,
        file_path,
        root,
    })
}

/// Parse the `status` command arguments
pub fn parse_status_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;
    let mut all = false;
    let mut project: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => db_path = Some(parse_path_arg(args, &mut i, "--db")?),
            "--all" => {
                all = true;
                i += 1;
            }
            "--project" => {
                project = Some(parse_required_arg(args, &mut i, "--project")?);
            }
            "--json" => {
                output_format = OutputFormat::Json;
                i += 1;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }
    if let Some(ref name) = project {
        let registry =
            Registry::load().context("Failed to load project registry for --project resolution")?;
        let entry = registry
            .find(name)
            .ok_or_else(|| anyhow::anyhow!("project '{}' not found in registry", name))?;
        db_path = Some(entry.db.clone());
    }
    let db_path = resolve_db_path(db_path)?;
    Ok(Command::Status {
        output_format,
        db_path,
        all,
    })
}

/// Parse the `features` command arguments
pub fn parse_features_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => db_path = Some(parse_path_arg(args, &mut i, "--db")?),
            "--json" => {
                output_format = OutputFormat::Json;
                i += 1;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }
    let db_path = resolve_db_path(db_path)?;
    Ok(Command::Features {
        db_path,
        output_format,
    })
}

pub fn parse_project_metadata_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut query: Option<String> = None;
    let mut output_format = OutputFormat::Human;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => db_path = Some(parse_path_arg(args, &mut i, "--db")?),
            "--query" => query = Some(parse_required_arg(args, &mut i, "--query")?),
            "--json" => {
                output_format = OutputFormat::Json;
                i += 1;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }
    let db_path = resolve_db_path(db_path)?;
    Ok(Command::ProjectMetadata {
        db_path,
        query,
        output_format,
    })
}

/// Parse the `doctor` command arguments
pub fn parse_doctor_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut fix = false;
    let mut output_format = OutputFormat::Human;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--db requires an argument"));
                }
                db_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--fix" => {
                fix = true;
                i += 1;
            }
            "--json" => {
                output_format = OutputFormat::Json;
                i += 1;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = parse_output_format(&args[i + 1])?;
                i += 2;
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
            }
        }
    }
    let db_path = resolve_db_path(db_path)?;
    Ok(Command::Doctor {
        db_path,
        fix,
        output_format,
    })
}
