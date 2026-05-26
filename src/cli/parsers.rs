use super::{Command, ContextSubcommand};
use anyhow::{Context, Result};
use magellan::graph::export::ExportFilters;
use magellan::graph::query::CollisionField;
use magellan::{detect_project_root, ExportFormat, OutputFormat, WatcherConfig};
use std::path::PathBuf;

use crate::db_resolver::resolve_db_path;
use crate::service::registry::Registry;

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

/// Helper to parse a required string argument
///
/// Returns the next argument value and increments index by 2,
/// or returns an error if no value is provided.
pub fn parse_required_arg(args: &[String], i: &mut usize, flag: &str) -> Result<String> {
    if *i + 1 >= args.len() {
        return Err(anyhow::anyhow!("{} requires an argument", flag));
    }
    let value = args[*i + 1].clone();
    *i += 2;
    Ok(value)
}

/// Helper to parse output format from string
///
/// Accepts: "human", "json", "pretty"
pub fn parse_output_format(value: &str) -> Result<OutputFormat> {
    match value {
        "human" => Ok(OutputFormat::Human),
        "json" => Ok(OutputFormat::Json),
        "pretty" => Ok(OutputFormat::Pretty),
        _ => Err(anyhow::anyhow!(
            "Invalid output format: {}. Must be human, json, or pretty",
            value
        )),
    }
}

/// Helper to parse a `PathBuf` argument
pub fn parse_path_arg(args: &[String], i: &mut usize, flag: &str) -> Result<PathBuf> {
    let value = parse_required_arg(args, i, flag)?;
    Ok(PathBuf::from(value))
}

/// Parse the `watch` command arguments
///
/// # Arguments
/// * `args` - The command line arguments (starting from index 2, after "watch")
///
/// # Returns
/// The parsed Watch command or an error
pub fn parse_backfill_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;

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
            _ => i += 1,
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Backfill { db_path })
}

pub fn parse_cross_file_refs_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut fqn: Option<String> = None;
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
            "--fqn" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--fqn requires an argument"));
                }
                fqn = Some(args[i + 1].clone());
                i += 2;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => i += 1,
        }
    }

    let db_path = resolve_db_path(db_path)?;
    let fqn = fqn.ok_or_else(|| anyhow::anyhow!("--fqn is required"))?;

    Ok(Command::CrossFileRefs {
        db_path,
        fqn,
        output_format,
    })
}

pub fn parse_registry_scan_args(args: &[String]) -> Result<Command> {
    let mut root: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--root requires an argument"));
                }
                root = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => i += 1,
        }
    }

    let root = root.unwrap_or_else(|| PathBuf::from("."));

    Ok(Command::RegistryScan {
        root,
        output_format,
    })
}

pub fn parse_registry_list_args(args: &[String]) -> Result<Command> {
    let mut root: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--root requires an argument"));
                }
                root = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => i += 1,
        }
    }

    let root = root.unwrap_or_else(|| PathBuf::from("."));

    Ok(Command::RegistryList {
        root,
        output_format,
    })
}

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

pub fn parse_index_args(args: &[String]) -> Result<Command> {
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

    Ok(Command::Index {
        db_path,
        file_path,
        root,
    })
}

pub fn parse_watch_args(args: &[String]) -> Result<Command> {
    let mut root_path: Option<PathBuf> = None;
    let mut db_path: Option<PathBuf> = None;
    let mut debounce_ms: u64 = 500;
    let mut watch_only = false;
    let mut scan_initial = true;
    let mut gitignore_aware = true;
    let mut validate = false;
    let mut validate_only = false;
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
            "--debounce-ms" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--debounce-ms requires an argument"));
                }
                debounce_ms = args[i + 1].parse()?;
                i += 2;
            }
            "--watch-only" => {
                watch_only = true;
                i += 1;
            }
            "--scan-initial" => {
                scan_initial = true;
                i += 1;
            }
            "--gitignore-aware" => {
                gitignore_aware = true;
                i += 1;
            }
            "--no-gitignore" => {
                gitignore_aware = false;
                i += 1;
            }
            "--validate" => {
                validate = true;
                i += 1;
            }
            "--validate-only" => {
                validate_only = true;
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
            _ => {
                return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
            }
        }
    }

    // Auto-detect project root if not specified
    let root_path = match root_path {
        Some(path) => path,
        None => detect_project_root(),
    };

    // Require --db argument (like other commands)
    let db_path = resolve_db_path(db_path)?;

    if watch_only {
        scan_initial = false;
    }

    let config = WatcherConfig {
        root_path: root_path.clone(),
        debounce_ms,
        gitignore_aware,
    };

    Ok(Command::Watch {
        root_path,
        db_path,
        config,
        scan_initial,
        validate,
        validate_only,
        output_format,
    })
}

/// Parse the `export` command arguments
pub fn parse_export_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut format = ExportFormat::Json;
    let mut output: Option<PathBuf> = None;
    let mut include_symbols = true;
    let mut include_references = true;
    let mut include_calls = true;
    let mut minify = false;
    let mut include_collisions = false;
    let mut collisions_field = CollisionField::Fqn;
    let mut filters = ExportFilters::default();

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
            "--format" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--format requires an argument"));
                }
                format = match args[i + 1].as_str() {
                    "json" => ExportFormat::Json,
                    "jsonl" => ExportFormat::JsonL,
                    "csv" => ExportFormat::Csv,
                    "scip" => ExportFormat::Scip,
                    "dot" => ExportFormat::Dot,
                    "lsif" => ExportFormat::Lsif,
                    _ => return Err(anyhow::anyhow!("Invalid format: {}", args[i + 1])),
                };
                i += 2;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--no-symbols" => {
                include_symbols = false;
                i += 1;
            }
            "--no-references" => {
                include_references = false;
                i += 1;
            }
            "--no-calls" => {
                include_calls = false;
                i += 1;
            }
            "--minify" => {
                minify = true;
                i += 1;
            }
            "--include-collisions" => {
                include_collisions = true;
                i += 1;
            }
            "--collisions-field" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--collisions-field requires an argument"));
                }
                collisions_field = match args[i + 1].as_str() {
                    "fqn" => CollisionField::Fqn,
                    "display_fqn" => CollisionField::DisplayFqn,
                    "canonical_fqn" => CollisionField::CanonicalFqn,
                    _ => return Err(anyhow::anyhow!("Invalid collisions field: {}", args[i + 1])),
                };
                i += 2;
            }
            "--filter-file" | "--file" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--filter-file requires an argument"));
                }
                filters.file = Some(args[i + 1].clone());
                i += 2;
            }
            "--filter-kind" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--filter-kind requires an argument"));
                }
                filters.kind = Some(args[i + 1].clone());
                i += 2;
            }
            "--cluster" => {
                filters.cluster = true;
                i += 1;
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
            }
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Export {
        db_path,
        format,
        output,
        include_symbols,
        include_references,
        include_calls,
        minify,
        include_collisions,
        collisions_field,
        filters,
    })
}

/// Parse the `import-lsif` command arguments
pub fn parse_import_lsif_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut lsif_paths: Vec<PathBuf> = Vec::new();

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
            "--input" | "--file" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--input requires an argument"));
                }
                lsif_paths.push(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => {
                // Treat as LSIF file path
                lsif_paths.push(PathBuf::from(&args[i]));
                i += 1;
            }
        }
    }

    let db_path = resolve_db_path(db_path)?;

    if lsif_paths.is_empty() {
        return Err(anyhow::anyhow!("At least one LSIF file must be specified"));
    }

    Ok(Command::ImportLsif {
        db_path,
        lsif_paths,
    })
}

/// Parse the `ingest-coverage` command arguments
///
/// Usage: `magellan ingest-coverage --db <FILE> --lcov <FILE>`
pub fn parse_ingest_coverage_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut lcov_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--db requires a value"));
                }
                db_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--lcov" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--lcov requires a value"));
                }
                lcov_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => i += 1,
        }
    }

    let db_path = resolve_db_path(db_path)?;
    let lcov_path = lcov_path.ok_or_else(|| anyhow::anyhow!("--lcov is required"))?;

    Ok(Command::IngestCoverage { db_path, lcov_path })
}

/// Parse the `enrich` command arguments
pub fn parse_enrich_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut files: Option<Vec<PathBuf>> = None;
    let mut timeout_secs: u64 = 30;

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
                let file = PathBuf::from(&args[i + 1]);
                files.get_or_insert_with(Vec::new).push(file);
                i += 2;
            }
            "--timeout" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--timeout requires an argument"));
                }
                timeout_secs = args[i + 1].parse().map_err(|_| {
                    anyhow::anyhow!("Invalid timeout: {}. Must be a number", args[i + 1])
                })?;
                i += 2;
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
            }
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Enrich {
        db_path,
        files,
        timeout_secs,
    })
}

/// Parse the `context` command arguments
pub fn parse_context_args(args: &[String]) -> Result<Command> {
    if args.is_empty() {
        return Err(anyhow::anyhow!(
            "context subcommand required: build, summary, list, symbol, file, impact, affected"
        ));
    }

    let mut db_paths: Vec<PathBuf> = Vec::new();
    let mut all = false;

    // Pre-scan for global flags (--db, --output, --all) that may appear before subcommand
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--db requires an argument"));
                }
                db_paths.extend(parse_db_paths(&args[i + 1])?);
                i += 2;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                let _ = parse_output_format(&args[i + 1])?;
                i += 2;
            }
            "--all" => {
                all = true;
                i += 1;
            }
            _ => break,
        }
    }

    // Slice args so subcommand is at index 0, flags start at index 1
    let args = &args[i..];
    let subcommand_name = args.first().map_or("", |s| s.as_str());
    let subcommand = match subcommand_name {
        "build" => ContextSubcommand::Build,
        "summary" => ContextSubcommand::Summary,
        "list" => {
            let mut kind: Option<String> = None;
            let mut page: Option<usize> = None;
            let mut page_size: Option<usize> = None;
            let mut cursor: Option<String> = None;
            let mut project: Option<String> = None;
            let mut output_format = OutputFormat::Human;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--db" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--db requires an argument"));
                        }
                        db_paths.extend(parse_db_paths(&args[i + 1])?);
                        i += 2;
                    }
                    "--kind" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--kind requires an argument"));
                        }
                        kind = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--page" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--page requires an argument"));
                        }
                        page = Some(
                            args[i + 1]
                                .parse()
                                .map_err(|_| anyhow::anyhow!("Invalid page number"))?,
                        );
                        i += 2;
                    }
                    "--page-size" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--page-size requires an argument"));
                        }
                        page_size = Some(
                            args[i + 1]
                                .parse()
                                .map_err(|_| anyhow::anyhow!("Invalid page size"))?,
                        );
                        i += 2;
                    }
                    "--cursor" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--cursor requires an argument"));
                        }
                        cursor = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--project" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--project requires an argument"));
                        }
                        project = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = parse_output_format(&args[i + 1])?;
                        i += 2;
                    }
                    "--all" => {
                        all = true;
                        i += 1;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            ContextSubcommand::List {
                kind,
                page,
                page_size,
                cursor,
                project,
                output_format,
            }
        }
        "symbol" => {
            let mut name: Option<String> = None;
            let mut file: Option<String> = None;
            let mut callers = false;
            let mut callees = false;
            let mut output_format = OutputFormat::Human;
            let mut with_source = false;
            let mut depth: Option<usize> = None;
            let mut project: Option<String> = None;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--db" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--db requires an argument"));
                        }
                        db_paths.extend(parse_db_paths(&args[i + 1])?);
                        i += 2;
                    }
                    "--name" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--name requires an argument"));
                        }
                        name = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--file" | "--path" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("{} requires an argument", args[i]));
                        }
                        file = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--callers" => {
                        callers = true;
                        i += 1;
                    }
                    "--callees" => {
                        callees = true;
                        i += 1;
                    }
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = parse_output_format(&args[i + 1])?;
                        i += 2;
                    }
                    "--with-source" => {
                        with_source = true;
                        i += 1;
                    }
                    "--depth" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--depth requires an argument"));
                        }
                        let d: usize = args[i + 1]
                            .parse()
                            .map_err(|_| anyhow::anyhow!("--depth must be a positive integer"))?;
                        depth = Some(d);
                        i += 2;
                    }
                    "--project" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--project requires an argument"));
                        }
                        project = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--all" => {
                        all = true;
                        i += 1;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let name =
                name.ok_or_else(|| anyhow::anyhow!("--name is required for symbol subcommand"))?;
            ContextSubcommand::Symbol {
                name,
                file,
                callers,
                callees,
                output_format,
                with_source,
                depth,
                project,
            }
        }
        "file" => {
            let mut path: Option<String> = None;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--db" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--db requires an argument"));
                        }
                        db_paths.extend(parse_db_paths(&args[i + 1])?);
                        i += 2;
                    }
                    "--path" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--path requires an argument"));
                        }
                        path = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--all" => {
                        all = true;
                        i += 1;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let path =
                path.ok_or_else(|| anyhow::anyhow!("--path is required for file subcommand"))?;
            ContextSubcommand::File { path }
        }
        "impact" => {
            let mut symbol: Option<String> = None;
            let mut file: Option<String> = None;
            let mut depth: usize = 3;
            let mut project: Option<String> = None;
            let mut output_format = OutputFormat::Human;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--db" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--db requires an argument"));
                        }
                        db_paths.extend(parse_db_paths(&args[i + 1])?);
                        i += 2;
                    }
                    "--name" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--name requires an argument"));
                        }
                        symbol = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--file" | "--path" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("{} requires an argument", args[i]));
                        }
                        file = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--depth" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--depth requires an argument"));
                        }
                        depth = args[i + 1]
                            .parse()
                            .map_err(|_| anyhow::anyhow!("--depth must be a positive integer"))?;
                        i += 2;
                    }
                    "--project" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--project requires an argument"));
                        }
                        project = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = parse_output_format(&args[i + 1])?;
                        i += 2;
                    }
                    "--all" => {
                        all = true;
                        i += 1;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let symbol = symbol
                .ok_or_else(|| anyhow::anyhow!("--name is required for impact subcommand"))?;
            ContextSubcommand::Impact {
                symbol,
                file,
                depth,
                project,
                output_format,
            }
        }
        "affected" => {
            let mut symbol: Option<String> = None;
            let mut file: Option<String> = None;
            let mut depth: usize = 3;
            let mut project: Option<String> = None;
            let mut output_format = OutputFormat::Human;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--db" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--db requires an argument"));
                        }
                        db_paths.extend(parse_db_paths(&args[i + 1])?);
                        i += 2;
                    }
                    "--name" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--name requires an argument"));
                        }
                        symbol = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--file" | "--path" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("{} requires an argument", args[i]));
                        }
                        file = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--depth" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--depth requires an argument"));
                        }
                        depth = args[i + 1]
                            .parse()
                            .map_err(|_| anyhow::anyhow!("--depth must be a positive integer"))?;
                        i += 2;
                    }
                    "--project" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--project requires an argument"));
                        }
                        project = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = parse_output_format(&args[i + 1])?;
                        i += 2;
                    }
                    "--all" => {
                        all = true;
                        i += 1;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let symbol = symbol
                .ok_or_else(|| anyhow::anyhow!("--name is required for affected subcommand"))?;
            ContextSubcommand::Affected {
                symbol,
                file,
                depth,
                project,
                output_format,
            }
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown context subcommand: {}. Use: build, summary, list, symbol, file, impact, affected",
                subcommand_name
            ));
        }
    };

    // Parse --db from remaining args if not already parsed
    if db_paths.is_empty() {
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--db" && i + 1 < args.len() {
                db_paths.extend(parse_db_paths(&args[i + 1])?);
                break;
            }
            i += 1;
        }
    }

    if all {
        let registry = Registry::load().with_context(|| "Failed to load project registry")?;
        let enabled: Vec<_> = registry.projects.iter().filter(|p| p.enabled).collect();
        if enabled.is_empty() {
            return Err(anyhow::anyhow!(
                "No enabled projects in registry. Use `magellan registry scan` to discover projects, then `magellan registry enable <name>` to activate."
            ));
        }
        db_paths = enabled.iter().map(|p| p.db.clone()).collect();
    }

    if db_paths.is_empty() {
        db_paths.push(resolve_db_path(None)?);
    }

    Ok(Command::Context {
        subcommand,
        db_paths,
    })
}

/// Parse comma-separated DB paths or discover .db files in a directory
pub fn parse_db_paths(value: &str) -> Result<Vec<PathBuf>> {
    let path = PathBuf::from(value);
    if path.is_dir() {
        // Discover all .db files in the directory
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "db") {
                paths.push(p);
            }
        }
        paths.sort();
        Ok(paths)
    } else if value.contains(',') {
        Ok(value.split(',').map(PathBuf::from).collect())
    } else {
        Ok(vec![path])
    }
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

/// Parse the `web-ui` command arguments
#[cfg(feature = "web-ui")]
pub fn parse_web_ui_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut host = "127.0.0.1".to_string();
    let mut port: u16 = 8080;

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
            "--host" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--host requires an argument"));
                }
                host = args[i + 1].clone();
                i += 2;
            }
            "--port" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--port requires an argument"));
                }
                port = args[i + 1]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid port number"))?;
                i += 2;
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
            }
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::WebUi {
        db_path,
        host,
        port,
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

/// Parse CLI arguments into a Command
///
/// This function handles all CLI argument parsing for Magellan.
/// For the --version and -V flags, it prints the version and exits.
/// For the --help and -h flags, it prints usage and exits.
///
/// The version display is handled via a closure passed in to avoid
/// circular dependencies with the version module.
pub fn parse_args_impl<F>(print_version: F) -> Result<Command>
where
    F: FnOnce(),
{
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        return Err(anyhow::anyhow!("Missing command"));
    }

    let command = &args[1];

    // Handle --version and -V flags
    if command == "--version" || command == "-V" {
        print_version();
        std::process::exit(0);
    }

    // Handle --help and -h flags
    if command == "--help" || command == "-h" {
        super::print_short_usage();
        std::process::exit(0);
    }

    // Handle --help-full and -H flags
    if command == "--help-full" || command == "-H" {
        super::print_full_usage();
        std::process::exit(0);
    }

    match command.as_str() {
        "watch" => parse_watch_args(&args[2..]),
        "backfill" => parse_backfill_args(&args[2..]),
        "cross-file-refs" => parse_cross_file_refs_args(&args[2..]),
        "delete" => parse_delete_args(&args[2..]),
        "export" => parse_export_args(&args[2..]),
        "index" => parse_index_args(&args[2..]),
        "import-lsif" => parse_import_lsif_args(&args[2..]),
        "ingest-coverage" => parse_ingest_coverage_args(&args[2..]),
        "enrich" => parse_enrich_args(&args[2..]),
        "status" => parse_status_args(&args[2..]),
        "project-metadata" => parse_project_metadata_args(&args[2..]),
        "context" => parse_context_args(&args[2..]),
        "doctor" => parse_doctor_args(&args[2..]),
        #[cfg(feature = "web-ui")]
        "web-ui" => parse_web_ui_args(&args[2..]),
        "query" => parse_query_args(&args[2..]),
        "find" => parse_find_args(&args[2..]),
        "refs" => parse_refs_args(&args[2..]),
        "get" => parse_get_args(&args[2..]),
        "get-file" => parse_get_file_args(&args[2..]),
        "files" => parse_files_args(&args[2..]),
        "verify" => parse_verify_args(&args[2..]),
        "refresh" => parse_refresh_args(&args[2..]),
        "label" => parse_label_args(&args[2..]),
        "collisions" => parse_collisions_args(&args[2..]),
        "migrate" => parse_migrate_args(&args[2..]),
        "migrate-backend" => parse_migrate_backend_args(&args[2..]),
        "chunks" => parse_chunks_args(&args[2..]),
        "chunk-by-span" => parse_chunk_by_span_args(&args[2..]),
        "chunk-by-symbol" => parse_chunk_by_symbol_args(&args[2..]),
        "ast" => parse_ast_args(&args[2..]),
        "find-ast" => parse_find_ast_args(&args[2..]),
        "reachable" => parse_reachable_args(&args[2..]),
        "dead-code" => parse_dead_code_args(&args[2..]),
        "cycles" => parse_cycles_args(&args[2..]),
        "registry" => {
            // Registry has subcommands
            if args.len() < 3 {
                return Err(anyhow::anyhow!("registry subcommand required: scan, list"));
            }
            match args[2].as_str() {
                "scan" => parse_registry_scan_args(&args[3..]),
                "list" => parse_registry_list_args(&args[3..]),
                _ => Err(anyhow::anyhow!("Unknown registry subcommand: {}", args[2])),
            }
        }
        "config" => {
            // Config has subcommands
            if args.len() < 3 {
                return Err(anyhow::anyhow!("config subcommand required: show, init"));
            }
            match args[2].as_str() {
                "show" => parse_config_show_args(&args[3..]),
                "init" => parse_config_init_args(&args[3..]),
                _ => Err(anyhow::anyhow!("Unknown config subcommand: {}", args[2])),
            }
        }
        "condense" => parse_condense_args(&args[2..]),
        "init" => parse_project_init_args(&args[2..]),
        "paths" => parse_paths_args(&args[2..]),
        "slice" => parse_slice_args(&args[2..]),
        "source-inventory" => parse_source_inventory_args(&args[2..]),
        "service" => {
            if args.len() < 3 {
                return Err(anyhow::anyhow!("service subcommand required: start, stop, list, register, unregister, pause, resume, status"));
            }
            let mut output_format = OutputFormat::Human;
            let mut name: Option<String> = None;
            let mut root: Option<PathBuf> = None;
            let mut event_type: Option<String> = None;
            let mut event_project: Option<String> = None;
            let mut since_hours: Option<u64> = None;
            let mut event_limit: usize = 50;
            let mut json_output = false;
            let mut i = 0;
            while i < args.len() {
                match args[i].as_str() {
                    "--output" | "-o" => {
                        let value = parse_required_arg(&args[..], &mut i, "--output")?;
                        output_format = parse_output_format(&value)?;
                    }
                    "--name" | "-n" => {
                        name = Some(parse_required_arg(&args[..], &mut i, "--name")?);
                    }
                    "--root" | "-r" => {
                        root = Some(parse_path_arg(&args[..], &mut i, "--root")?);
                    }
                    "--type" | "-t" => {
                        event_type = Some(parse_required_arg(&args[..], &mut i, "--type")?);
                    }
                    "--project" | "-p" => {
                        event_project = Some(parse_required_arg(&args[..], &mut i, "--project")?);
                    }
                    "--since" => {
                        since_hours = Some(
                            parse_required_arg(&args[..], &mut i, "--since")?
                                .parse()
                                .map_err(|_| anyhow::anyhow!("--since must be a number"))?,
                        );
                    }
                    "--limit" | "-l" => {
                        event_limit = parse_required_arg(&args[..], &mut i, "--limit")?
                            .parse()
                            .map_err(|_| anyhow::anyhow!("--limit must be a number"))?;
                    }
                    "--json" | "-j" => {
                        json_output = true;
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
            let action = match args[2].as_str() {
                "start" => crate::service_cmd::ServiceAction::Start,
                "stop" => crate::service_cmd::ServiceAction::Stop,
                "list" => crate::service_cmd::ServiceAction::List,
                "register" => crate::service_cmd::ServiceAction::Register {
                    root: root.unwrap_or_else(|| PathBuf::from(".")),
                    name,
                },
                "unregister" => crate::service_cmd::ServiceAction::Unregister {
                    name: name.unwrap_or_default(),
                },
                "pause" => crate::service_cmd::ServiceAction::Pause {
                    name: name.unwrap_or_default(),
                },
                "resume" => crate::service_cmd::ServiceAction::Resume {
                    name: name.unwrap_or_default(),
                },
                "status" => crate::service_cmd::ServiceAction::Status,
                "stats" => crate::service_cmd::ServiceAction::Stats,
                "events" => crate::service_cmd::ServiceAction::Events {
                    project: event_project,
                    event_type,
                    since_hours,
                    limit: event_limit,
                    json_output,
                },
                _ => return Err(anyhow::anyhow!("Unknown service subcommand: {}", args[2])),
            };
            Ok(Command::Service {
                action,
                output_format,
            })
        }
        "candidate-fact" => parse_candidate_fact_args(&args[2..]),
        "cypher" => parse_cypher_args(&args[2..]),
        "hnsw-create" => parse_hnsw_create_args(&args[2..]),
        "hnsw-query" => parse_hnsw_query_args(&args[2..]),
        "ask" => parse_ask_args(&args[2..]),
        "navigate" => parse_navigate_args(&args[2..]),
        "features" => parse_features_args(&args[2..]),
        "service-daemon" => Ok(Command::ServiceDaemon),
        _ => Err(anyhow::anyhow!("Unknown command: {}", command)),
    }
}

/// Parse the `files` command arguments
pub fn parse_files_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;
    let mut with_symbols = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => db_path = Some(parse_path_arg(args, &mut i, "--db")?),
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            "--symbols" => {
                with_symbols = true;
                i += 1;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Files {
        db_path,
        output_format,
        with_symbols,
    })
}

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

/// Parse the `query` command arguments
pub fn parse_query_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut file_path: Option<PathBuf> = None;
    let mut root: Option<PathBuf> = None;
    let mut kind: Option<String> = None;
    let mut explain = false;
    let mut symbol: Option<String> = None;
    let mut show_extent = false;
    let mut output_format = OutputFormat::Human;
    let mut with_context = false;
    let mut with_callers = false;
    let mut with_callees = false;
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
            "--kind" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--kind requires an argument"));
                }
                kind = Some(args[i + 1].clone());
                i += 2;
            }
            "--explain" => {
                explain = true;
                i += 1;
            }
            "--symbol" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--symbol requires an argument"));
                }
                symbol = Some(args[i + 1].clone());
                i += 2;
            }
            "--show-extent" => {
                show_extent = true;
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

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Query {
        db_path,
        file_path,
        root,
        kind,
        explain,
        symbol,
        show_extent,
        output_format,
        with_context,
        with_callers,
        with_callees,
        with_semantics,
        with_checksums,
        context_lines,
    })
}

/// Parse the `chunks` command arguments
pub fn parse_chunks_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;
    let mut limit: Option<usize> = None;
    let mut file_filter: Option<String> = None;
    let mut kind_filter: Option<String> = None;

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
            "--limit" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--limit requires an argument"));
                }
                limit = Some(args[i + 1].parse()?);
                i += 2;
            }
            "--file" | "--path" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--file requires an argument"));
                }
                file_filter = Some(args[i + 1].clone());
                i += 2;
            }
            "--kind" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--kind requires an argument"));
                }
                kind_filter = Some(args[i + 1].clone());
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Chunks {
        db_path,
        output_format,
        limit,
        file_filter,
        kind_filter,
    })
}

/// Parse the `chunk-by-span` command arguments
pub fn parse_chunk_by_span_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut file_path: Option<String> = None;
    let mut byte_start: Option<usize> = None;
    let mut byte_end: Option<usize> = None;
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
            "--start" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--start requires an argument"));
                }
                byte_start = Some(args[i + 1].parse()?);
                i += 2;
            }
            "--end" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--end requires an argument"));
                }
                byte_end = Some(args[i + 1].parse()?);
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
    let byte_start = byte_start.ok_or_else(|| anyhow::anyhow!("--start is required"))?;
    let byte_end = byte_end.ok_or_else(|| anyhow::anyhow!("--end is required"))?;

    Ok(Command::ChunkBySpan {
        db_path,
        file_path,
        byte_start,
        byte_end,
        output_format,
    })
}

/// Parse the `chunk-by-symbol` command arguments
pub fn parse_chunk_by_symbol_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut symbol_name: Option<String> = None;
    let mut file_filter: Option<String> = None;
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
                symbol_name = Some(args[i + 1].clone());
                i += 2;
            }
            "--file" | "--path" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--file requires an argument"));
                }
                file_filter = Some(args[i + 1].clone());
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
    let symbol_name = symbol_name.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

    Ok(Command::ChunkBySymbol {
        db_path,
        symbol_name,
        file_filter,
        output_format,
    })
}

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

/// Convenience wrapper around `parse_args_impl` that uses the version module
pub fn parse_args() -> Result<Command> {
    parse_args_impl(|| {
        println!("{}", crate::version::version());
    })
}

/// Parse the `cypher` command arguments
pub fn parse_cypher_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut query: Option<String> = None;
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
            "--query" | "-q" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--query requires an argument"));
                }
                query = Some(args[i + 1].clone());
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
            _ => {
                // Positional: first unknown is the query string
                query = Some(args[i].clone());
                i += 1;
            }
        }
    }

    let db_path = resolve_db_path(db_path)?;
    let query = query.ok_or_else(|| anyhow::anyhow!("Query string is required"))?;

    Ok(Command::Cypher {
        db_path,
        query,
        output_format,
    })
}

/// Parse the `ask` command arguments
pub fn parse_ask_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;
    let mut name: Option<String> = None;
    let mut all = false;
    let mut project: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--output" | "-o" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            "--name" | "-n" => {
                name = Some(parse_required_arg(args, &mut i, "--name")?);
            }
            "--all" => {
                all = true;
                i += 1;
            }
            "--project" => {
                project = Some(parse_required_arg(args, &mut i, "--project")?);
            }
            _ => {
                if !args[i].starts_with("--") && name.is_none() {
                    name = Some(args[i].clone());
                }
                i += 1;
            }
        }
    }
    let Some(question) = name else {
        return Err(anyhow::anyhow!(
            "ask requires a question. Example: magellan ask \"who calls run_find\""
        ));
    };
    if let Some(ref proj_name) = project {
        let registry =
            Registry::load().context("Failed to load project registry for --project resolution")?;
        let entry = registry
            .find(proj_name)
            .ok_or_else(|| anyhow::anyhow!("project '{}' not found in registry", proj_name))?;
        db_path = Some(entry.db.clone());
    }
    let db_path = resolve_db_path(db_path)?;
    Ok(Command::Ask {
        question,
        db_path,
        output_format,
        all,
    })
}

/// Parse the `navigate` command arguments
pub fn parse_navigate_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut task: Option<String> = None;
    let mut depth = 2usize;
    let mut budget = 4000usize;
    let mut limit = 5usize;
    let mut concise = false;
    let mut with_llmgrep = false;
    let mut with_mirage = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--depth" => {
                let v = parse_required_arg(args, &mut i, "--depth")?;
                depth = v
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--depth must be a positive integer"))?;
            }
            "--budget" => {
                let v = parse_required_arg(args, &mut i, "--budget")?;
                budget = v
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--budget must be a positive integer"))?;
            }
            "--limit" => {
                let v = parse_required_arg(args, &mut i, "--limit")?;
                limit = v
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--limit must be a positive integer"))?;
            }
            "--concise" => {
                concise = true;
                i += 1;
            }
            "--with-llmgrep" => {
                with_llmgrep = true;
                i += 1;
            }
            "--with-mirage" => {
                with_mirage = true;
                i += 1;
            }
            _ => {
                if !args[i].starts_with("--") && task.is_none() {
                    task = Some(args[i].clone());
                }
                i += 1;
            }
        }
    }
    let task = task.ok_or_else(|| {
        anyhow::anyhow!(
            "navigate requires a task description. Example: magellan navigate \"who calls index_file\""
        )
    })?;
    let db_path = resolve_db_path(db_path)?;
    Ok(Command::Navigate {
        task,
        db_path,
        depth,
        budget,
        limit,
        concise,
        with_llmgrep,
        with_mirage,
    })
}

/// Parse the `hnsw-create` command arguments
pub fn parse_hnsw_create_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut dim = 128usize;
    let mut m = 16usize;
    let mut ef_construction = 200usize;
    let mut ef_search = 50usize;
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
            "--name" | "-n" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--name requires an argument"));
                }
                name = Some(args[i + 1].clone());
                i += 2;
            }
            "--dim" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--dim requires an argument"));
                }
                dim = args[i + 1]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--dim must be a number"))?;
                i += 2;
            }
            "--m" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--m requires an argument"));
                }
                m = args[i + 1]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--m must be a number"))?;
                i += 2;
            }
            "--ef-construction" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--ef-construction requires an argument"));
                }
                ef_construction = args[i + 1]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--ef-construction must be a number"))?;
                i += 2;
            }
            "--ef-search" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--ef-search requires an argument"));
                }
                ef_search = args[i + 1]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--ef-search must be a number"))?;
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
            _ => i += 1,
        }
    }

    let db_path = resolve_db_path(db_path)?;
    let name = name.ok_or_else(|| anyhow::anyhow!("--name is required"))?;

    Ok(Command::HnswCreate {
        db_path,
        name,
        dim,
        m,
        ef_construction,
        ef_search,
        output_format,
    })
}

/// Parse the `hnsw-query` command arguments
pub fn parse_hnsw_query_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut vector: Option<String> = None;
    let mut k = 10usize;
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
            "--name" | "-n" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--name requires an argument"));
                }
                name = Some(args[i + 1].clone());
                i += 2;
            }
            "--vector" | "-v" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--vector requires an argument"));
                }
                vector = Some(args[i + 1].clone());
                i += 2;
            }
            "--k" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--k requires an argument"));
                }
                k = args[i + 1]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--k must be a number"))?;
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
            _ => i += 1,
        }
    }

    let db_path = resolve_db_path(db_path)?;
    let name = name.ok_or_else(|| anyhow::anyhow!("--name is required"))?;
    let vector =
        vector.ok_or_else(|| anyhow::anyhow!("--vector is required (JSON array of f32)"))?;

    Ok(Command::HnswQuery {
        db_path,
        name,
        vector,
        k,
        output_format,
    })
}
