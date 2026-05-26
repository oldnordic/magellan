use crate::cli::Command;
use anyhow::Result;
use magellan::OutputFormat;
use std::path::PathBuf;

use crate::db_resolver::resolve_db_path;

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

// ============================================================================
// Graph Parsers
// ============================================================================

/// Parse the `ast` command arguments
pub fn parse_ast_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut file_path: Option<String> = None;
    let mut position: Option<usize> = None;
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
            "--file" | "--path" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--file requires an argument"));
                }
                file_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--position" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--position requires an argument"));
                }
                position = Some(args[i + 1].parse()?);
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
    let file_path = file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;

    Ok(Command::Ast {
        db_path,
        file_path,
        position,
        output_format,
    })
}

/// Parse the `find-ast` command arguments
pub fn parse_find_ast_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut kind: Option<String> = None;
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
            "--kind" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--kind requires an argument"));
                }
                kind = Some(args[i + 1].clone());
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
    let kind = kind.ok_or_else(|| anyhow::anyhow!("--kind is required"))?;

    Ok(Command::FindAst {
        db_path,
        kind,
        output_format,
    })
}

/// Parse the `reachable` command arguments
pub fn parse_reachable_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut symbol_id: Option<String> = None;
    let mut reverse = false;
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
            "--symbol" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--symbol requires an argument"));
                }
                symbol_id = Some(args[i + 1].clone());
                i += 2;
            }
            "--reverse" => {
                reverse = true;
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
    let symbol_id = symbol_id.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

    Ok(Command::Reachable {
        db_path,
        symbol_id,
        reverse,
        output_format,
    })
}

/// Parse the `dead-code` command arguments
pub fn parse_dead_code_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut entry_symbol_id: Option<String> = None;
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
            "--entry" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--entry requires an argument"));
                }
                entry_symbol_id = Some(args[i + 1].clone());
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
    let entry_symbol_id = entry_symbol_id.ok_or_else(|| anyhow::anyhow!("--entry is required"))?;

    Ok(Command::DeadCode {
        db_path,
        entry_symbol_id,
        output_format,
    })
}

/// Parse the `cycles` command arguments
pub fn parse_cycles_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut symbol_id: Option<String> = None;
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
            "--symbol" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--symbol requires an argument"));
                }
                symbol_id = Some(args[i + 1].clone());
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

    Ok(Command::Cycles {
        db_path,
        symbol_id,
        output_format,
    })
}

/// Parse the `condense` command arguments
pub fn parse_condense_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut show_members = false;
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
            "--members" => {
                show_members = true;
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

    Ok(Command::Condense {
        db_path,
        show_members,
        output_format,
    })
}

/// Parse the `paths` command arguments
pub fn parse_paths_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut start_symbol_id: Option<String> = None;
    let mut end_symbol_id: Option<String> = None;
    let mut max_depth = 100;
    let mut max_paths = 1000;
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
            "--start" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--start requires an argument"));
                }
                start_symbol_id = Some(args[i + 1].clone());
                i += 2;
            }
            "--end" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--end requires an argument"));
                }
                end_symbol_id = Some(args[i + 1].clone());
                i += 2;
            }
            "--max-depth" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--max-depth requires an argument"));
                }
                max_depth = args[i + 1].parse()?;
                i += 2;
            }
            "--max-paths" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--max-paths requires an argument"));
                }
                max_paths = args[i + 1].parse()?;
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
    let start_symbol_id = start_symbol_id.ok_or_else(|| anyhow::anyhow!("--start is required"))?;

    Ok(Command::Paths {
        db_path,
        start_symbol_id,
        end_symbol_id,
        max_depth,
        max_paths,
        output_format,
    })
}

/// Parse the `slice` command arguments
pub fn parse_slice_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut target: Option<String> = None;
    let mut direction = "backward".to_string();
    let mut verbose = false;
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
            "--target" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--target requires an argument"));
                }
                target = Some(args[i + 1].clone());
                i += 2;
            }
            "--direction" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--direction requires an argument"));
                }
                direction = args[i + 1].clone();
                if direction != "backward" && direction != "forward" {
                    return Err(anyhow::anyhow!(
                        "Invalid direction: {}. Must be backward or forward",
                        direction
                    ));
                }
                i += 2;
            }
            "--verbose" => {
                verbose = true;
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
    let target = target.ok_or_else(|| anyhow::anyhow!("--target is required"))?;

    Ok(Command::Slice {
        db_path,
        target,
        direction,
        verbose,
        output_format,
    })
}

pub fn parse_source_inventory_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut scan_dirs: Vec<(PathBuf, String)> = Vec::new();
    let mut list_kind: Option<String> = None;
    let mut show_stale = false;
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
            "--scan" => {
                if i + 2 >= args.len() {
                    return Err(anyhow::anyhow!("--scan requires <dir> <kind> arguments"));
                }
                scan_dirs.push((PathBuf::from(&args[i + 1]), args[i + 2].clone()));
                i += 3;
            }
            "--kind" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--kind requires an argument"));
                }
                list_kind = Some(args[i + 1].clone());
                i += 2;
            }
            "--list" => {
                i += 1;
            }
            "--stale" => {
                show_stale = true;
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

    Ok(Command::SourceInventory {
        db_path,
        scan_dirs,
        list_kind,
        show_stale,
        output_format,
    })
}

pub fn parse_candidate_fact_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;
    let mut subcommand = String::new();

    // Submit/validate fields
    let mut candidate_id = String::new();
    let mut from_source: Option<i64> = None;
    let mut subject_type = String::new();
    let mut subject_key = String::new();
    let mut predicate = String::new();
    let mut object_type: Option<String> = None;
    let mut object_key: Option<String> = None;
    let mut properties_json: Option<String> = None;

    // List fields
    let mut status: Option<String> = None;
    let mut limit: Option<usize> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "submit" | "validate" | "list" | "review-queue" => {
                subcommand = args[i].clone();
                i += 1;
            }
            "--db" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--db requires an argument"));
                }
                db_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--candidate-id" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--candidate-id requires an argument"));
                }
                candidate_id = args[i + 1].clone();
                i += 2;
            }
            "--from-source" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--from-source requires an argument"));
                }
                from_source = Some(
                    args[i + 1]
                        .parse()
                        .map_err(|_| anyhow::anyhow!("--from-source must be an integer"))?,
                );
                i += 2;
            }
            "--subject-type" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--subject-type requires an argument"));
                }
                subject_type = args[i + 1].clone();
                i += 2;
            }
            "--subject-key" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--subject-key requires an argument"));
                }
                subject_key = args[i + 1].clone();
                i += 2;
            }
            "--predicate" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--predicate requires an argument"));
                }
                predicate = args[i + 1].clone();
                i += 2;
            }
            "--object-type" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--object-type requires an argument"));
                }
                object_type = Some(args[i + 1].clone());
                i += 2;
            }
            "--object-key" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--object-key requires an argument"));
                }
                object_key = Some(args[i + 1].clone());
                i += 2;
            }
            "--properties" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--properties requires an argument"));
                }
                properties_json = Some(args[i + 1].clone());
                i += 2;
            }
            "--status" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--status requires an argument"));
                }
                status = Some(args[i + 1].clone());
                i += 2;
            }
            "--limit" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--limit requires an argument"));
                }
                limit = Some(
                    args[i + 1]
                        .parse()
                        .map_err(|_| anyhow::anyhow!("--limit must be a positive integer"))?,
                );
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

    let action = match subcommand.as_str() {
        "submit" => {
            let source_doc_id = from_source
                .ok_or_else(|| anyhow::anyhow!("--from-source is required for submit"))?;
            let mut props = match properties_json {
                Some(json) => serde_json::from_str(&json)
                    .map_err(|e| anyhow::anyhow!("Invalid properties JSON: {}", e))?,
                None => magellan::graph::candidate_fact::CandidateProperties::default(),
            };
            // Override source if provided
            if props.source.is_empty() {
                props.source = format!("source_doc:{}", source_doc_id);
            }

            if candidate_id.is_empty() {
                candidate_id = format!("cf_{}", uuid::Uuid::new_v4().as_simple());
            }

            let mut fact = magellan::graph::candidate_fact::CandidateFact::new(
                candidate_id.clone(),
                source_doc_id,
                subject_type.clone(),
                subject_key.clone(),
                predicate.clone(),
                props,
            );
            if let (Some(ot), Some(ok)) = (object_type, object_key) {
                fact.object_type = Some(ot);
                fact.object_key = Some(ok);
            }

            crate::candidate_fact_cmd::CandidateFactAction::Submit { fact }
        }
        "validate" => {
            if candidate_id.is_empty() {
                return Err(anyhow::anyhow!("--candidate-id is required for validate"));
            }
            crate::candidate_fact_cmd::CandidateFactAction::Validate { candidate_id }
        }
        "list" => {
            let status_enum =
                status.and_then(|s| magellan::graph::candidate_fact::CandidateStatus::parse(&s));
            crate::candidate_fact_cmd::CandidateFactAction::List {
                status: status_enum,
                limit,
            }
        }
        "review-queue" => crate::candidate_fact_cmd::CandidateFactAction::ReviewQueue { limit },
        _ => {
            return Err(anyhow::anyhow!(
            "Unknown candidate-fact subcommand: {}. Use submit, validate, list, or review-queue",
            subcommand
        ))
        }
    };

    Ok(Command::CandidateFact {
        db_path,
        action,
        output_format,
    })
}
