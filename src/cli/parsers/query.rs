use crate::cli::Command;
use anyhow::{Context, Result};
use magellan::graph::query::CollisionField;
use magellan::OutputFormat;
use std::path::PathBuf;

use crate::cli::parsers::*;
use crate::db_resolver::resolve_db_path;
use crate::service::registry::Registry;

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

// ============================================================================
// Query Parsers
// ============================================================================

/// Parse the `find` command arguments
pub fn parse_find_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut root: Option<PathBuf> = None;
    let mut path: Option<PathBuf> = None;
    let mut glob_pattern: Option<String> = None;
    let mut symbol_id: Option<String> = None;
    let mut ambiguous_name: Option<String> = None;
    let mut first = false;
    let mut all = false;
    let mut project: Option<String> = None;
    let mut output_format = OutputFormat::Human;
    let mut with_context = false;
    let mut with_callers = false;
    let mut with_callees = false;
    let mut with_semantics = false;
    let mut with_checksums = false;
    let mut context_lines: usize = 3;

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
            "--name" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--name requires an argument"));
                }
                name = Some(args[i + 1].clone());
                i += 2;
            }
            "--root" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--root requires an argument"));
                }
                root = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--path" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--path requires an argument"));
                }
                path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--glob" | "--list-glob" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--glob requires an argument"));
                }
                glob_pattern = Some(args[i + 1].clone());
                i += 2;
            }
            "--symbol-id" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--symbol-id requires an argument"));
                }
                symbol_id = Some(args[i + 1].clone());
                i += 2;
            }
            "--ambiguous" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--ambiguous requires an argument"));
                }
                ambiguous_name = Some(args[i + 1].clone());
                i += 2;
            }
            "--first" => {
                first = true;
                i += 1;
            }
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
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = match args[i + 1].as_str() {
                    "human" => OutputFormat::Human,
                    "json" => OutputFormat::Json,
                    "pretty" => OutputFormat::Pretty,
                    _ => return Err(anyhow::anyhow!("Invalid output format: {}", args[i + 1])),
                };
                i += 2;
            }
            "--with-context" => {
                with_context = true;
                i += 1;
            }
            "--with-callers" => {
                with_callers = true;
                i += 1;
            }
            "--with-callees" => {
                with_callees = true;
                i += 1;
            }
            "--with-semantics" => {
                with_semantics = true;
                i += 1;
            }
            "--with-checksums" => {
                with_checksums = true;
                i += 1;
            }
            "--context-lines" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--context-lines requires an argument"));
                }
                context_lines = args[i + 1].parse().map_err(|_| {
                    anyhow::anyhow!("Invalid context lines: {}. Must be a number", args[i + 1])
                })?;
                // Cap context lines at 100 maximum
                if context_lines > 100 {
                    context_lines = 100;
                }
                i += 2;
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

    Ok(Command::Find {
        db_path,
        name,
        root,
        path,
        glob_pattern,
        symbol_id,
        ambiguous_name,
        first,
        output_format,
        with_context,
        with_callers,
        with_callees,
        with_semantics,
        with_checksums,
        context_lines,
        all,
    })
}

// ============================================================================
// Main Argument Parser
// ============================================================================

/// Parse the `get` command arguments
pub fn parse_get_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut file_path: Option<String> = None;
    let mut symbol_name: Option<String> = None;
    let mut output_format = OutputFormat::Human;
    let mut with_context = false;
    let mut with_semantics = false;
    let mut with_checksums = false;
    let mut context_lines = 3;

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
                file_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--symbol" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--symbol requires an argument"));
                }
                symbol_name = Some(args[i + 1].clone());
                i += 2;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = match args[i + 1].as_str() {
                    "human" => OutputFormat::Human,
                    "json" => OutputFormat::Json,
                    "pretty" => OutputFormat::Pretty,
                    _ => return Err(anyhow::anyhow!("Invalid output format: {}", args[i + 1])),
                };
                i += 2;
            }
            "--with-context" => {
                with_context = true;
                i += 1;
            }
            "--with-semantics" => {
                with_semantics = true;
                i += 1;
            }
            "--with-checksums" => {
                with_checksums = true;
                i += 1;
            }
            "--context-lines" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--context-lines requires an argument"));
                }
                context_lines = args[i + 1].parse().map_err(|_| {
                    anyhow::anyhow!("Invalid context lines: {}. Must be a number", args[i + 1])
                })?;
                if context_lines > 100 {
                    context_lines = 100;
                }
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = resolve_db_path(db_path)?;
    let file_path = file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;
    let symbol_name = symbol_name.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

    Ok(Command::Get {
        db_path,
        file_path,
        symbol_name,
        output_format,
        with_context,
        with_semantics,
        with_checksums,
        context_lines,
    })
}

/// Parse the `get-file` command arguments
pub fn parse_get_file_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut file_path: Option<String> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => db_path = Some(parse_path_arg(args, &mut i, "--db")?),
            "--file" => file_path = Some(parse_required_arg(args, &mut i, "--file")?),
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = parse_output_format(&args[i + 1])?;
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = resolve_db_path(db_path)?;
    let file_path = file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;

    Ok(Command::GetFile {
        db_path,
        file_path,
        output_format,
    })
}

/// Parse the `refs` command arguments
pub fn parse_refs_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut root: Option<PathBuf> = None;
    let mut path: Option<PathBuf> = None;
    let mut symbol_id: Option<String> = None;
    let mut direction = "in".to_string();
    let mut output_format = OutputFormat::Human;
    let mut with_context = false;
    let mut with_semantics = false;
    let mut with_checksums = false;
    let mut context_lines = 3;
    let mut all = false;

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
            "--name" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--name requires an argument"));
                }
                name = Some(args[i + 1].clone());
                i += 2;
            }
            "--root" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--root requires an argument"));
                }
                root = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--path" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--path requires an argument"));
                }
                path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--symbol-id" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--symbol-id requires an argument"));
                }
                symbol_id = Some(args[i + 1].clone());
                i += 2;
            }
            "--direction" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--direction requires an argument"));
                }
                direction = args[i + 1].clone();
                if direction != "in" && direction != "out" {
                    return Err(anyhow::anyhow!(
                        "Invalid direction: {}. Must be in or out",
                        direction
                    ));
                }
                i += 2;
            }
            "--json" => {
                output_format = OutputFormat::Json;
                i += 1;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = match args[i + 1].as_str() {
                    "human" => OutputFormat::Human,
                    "json" => OutputFormat::Json,
                    "pretty" => OutputFormat::Pretty,
                    _ => return Err(anyhow::anyhow!("Invalid output format: {}", args[i + 1])),
                };
                i += 2;
            }
            "--with-context" => {
                with_context = true;
                i += 1;
            }
            "--with-semantics" => {
                with_semantics = true;
                i += 1;
            }
            "--with-checksums" => {
                with_checksums = true;
                i += 1;
            }
            "--context-lines" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--context-lines requires an argument"));
                }
                context_lines = args[i + 1].parse().map_err(|_| {
                    anyhow::anyhow!("Invalid context lines: {}. Must be a number", args[i + 1])
                })?;
                if context_lines > 100 {
                    context_lines = 100;
                }
                i += 2;
            }
            "--all" => {
                all = true;
                i += 1;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = if !all {
        resolve_db_path(db_path)?
    } else {
        db_path.unwrap_or_default()
    };
    let name = name.ok_or_else(|| anyhow::anyhow!("--name is required"))?;
    // path is now optional - if not provided, will search all symbols matching name

    Ok(Command::Refs {
        db_path,
        name,
        root,
        path,
        symbol_id,
        direction,
        output_format,
        with_context,
        with_semantics,
        with_checksums,
        context_lines,
        all,
    })
}

/// Parse the `verify` command arguments
pub fn parse_verify_args(args: &[String]) -> Result<Command> {
    let mut root_path: Option<PathBuf> = None;
    let mut db_path: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--root requires an argument"));
                }
                root_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--db" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--db requires an argument"));
                }
                db_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!(
                        "--output requires an argument (human|json|pretty)"
                    ));
                }
                output_format = parse_output_format(&args[i + 1])?;
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let root_path = root_path.ok_or_else(|| anyhow::anyhow!("--root is required"))?;
    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Verify {
        root_path,
        db_path,
        output_format,
    })
}

/// Parse the `refresh` command arguments
pub fn parse_refresh_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut include_untracked = false;
    let mut staged = false;
    let mut unstaged = false;
    let mut force = false;
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
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--include-untracked" => {
                include_untracked = true;
                i += 1;
            }
            "--staged" => {
                staged = true;
                i += 1;
            }
            "--unstaged" => {
                unstaged = true;
                i += 1;
            }
            "--force" => {
                force = true;
                i += 1;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = match args[i + 1].as_str() {
                    "human" => OutputFormat::Human,
                    "json" => OutputFormat::Json,
                    "pretty" => OutputFormat::Pretty,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid output format: {}. Must be human, json, or pretty",
                            args[i + 1]
                        ))
                    }
                };
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = db_path.unwrap_or_else(|| PathBuf::from(".magellan/magellan.db"));

    Ok(Command::Refresh {
        db_path,
        dry_run,
        include_untracked,
        staged,
        unstaged,
        force,
        output_format,
    })
}

/// Parse the `label` command arguments
pub fn parse_label_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut label = Vec::new();
    let mut list = false;
    let mut count = false;
    let mut show_code = false;
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
            "--label" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--label requires an argument"));
                }
                label.push(args[i + 1].clone());
                i += 2;
            }
            "--list" => {
                list = true;
                i += 1;
            }
            "--count" => {
                count = true;
                i += 1;
            }
            "--show-code" => {
                show_code = true;
                i += 1;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = parse_output_format(&args[i + 1])?;
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Label {
        db_path,
        label,
        list,
        count,
        show_code,
        output_format,
    })
}

/// Parse the `collisions` command arguments
pub fn parse_collisions_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut field = CollisionField::Fqn;
    let mut limit = 100;
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
            "--field" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--field requires an argument"));
                }
                field = match args[i + 1].as_str() {
                    "fqn" => CollisionField::Fqn,
                    "display_fqn" => CollisionField::DisplayFqn,
                    "canonical_fqn" => CollisionField::CanonicalFqn,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid field: {}. Must be fqn, display_fqn, or canonical_fqn",
                            args[i + 1]
                        ))
                    }
                };
                i += 2;
            }
            "--limit" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--limit requires an argument"));
                }
                limit = args[i + 1].parse()?;
                i += 2;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = match args[i + 1].as_str() {
                    "human" => OutputFormat::Human,
                    "json" => OutputFormat::Json,
                    "pretty" => OutputFormat::Pretty,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid output format: {}. Must be human, json, or pretty",
                            args[i + 1]
                        ))
                    }
                };
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Collisions {
        db_path,
        field,
        limit,
        output_format,
    })
}

/// Parse the `migrate` command arguments
pub fn parse_migrate_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut no_backup = false;
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
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--no-backup" => {
                no_backup = true;
                i += 1;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = match args[i + 1].as_str() {
                    "human" => OutputFormat::Human,
                    "json" => OutputFormat::Json,
                    "pretty" => OutputFormat::Pretty,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid output format: {}. Must be human, json, or pretty",
                            args[i + 1]
                        ))
                    }
                };
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Migrate {
        db_path,
        dry_run,
        no_backup,
        output_format,
    })
}

/// Parse the `migrate-backend` command arguments
pub fn parse_migrate_backend_args(args: &[String]) -> Result<Command> {
    let mut input_db: Option<PathBuf> = None;
    let mut output_db: Option<PathBuf> = None;
    let mut export_dir: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--input requires an argument"));
                }
                input_db = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_db = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--export-dir" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--export-dir requires an argument"));
                }
                export_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--format" => {
                // Legacy alias for --output
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--format requires an argument"));
                }
                output_format = match args[i + 1].as_str() {
                    "human" => OutputFormat::Human,
                    "json" => OutputFormat::Json,
                    "pretty" => OutputFormat::Pretty,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid output format: {}. Must be human, json, or pretty",
                            args[i + 1]
                        ))
                    }
                };
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let input_db = input_db.ok_or_else(|| anyhow::anyhow!("--input is required"))?;
    let output_db = output_db.ok_or_else(|| anyhow::anyhow!("--output is required"))?;

    Ok(Command::MigrateBackend {
        input_db,
        output_db,
        export_dir,
        dry_run,
        output_format,
    })
}
