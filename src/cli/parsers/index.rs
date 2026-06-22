use crate::cli::{Command, ContextSubcommand};
use anyhow::{Context, Result};
use magellan::graph::export::ExportFilters;
use magellan::graph::query::CollisionField;
use magellan::{detect_project_root, ExportFormat, OutputFormat, WatcherConfig};
use std::path::PathBuf;

use crate::cli::parsers::*;
use crate::db_resolver::resolve_db_path;
use crate::service::registry::Registry;

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

// ============================================================================
// Index Parsers
// ============================================================================

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
    let mut impact_symbol = None;
    let mut impact_file = None;
    let mut impact_depth = 10;

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
                    "impact" => ExportFormat::Impact,
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
            "--symbol" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--symbol requires an argument"));
                }
                impact_symbol = Some(args[i + 1].clone());
                i += 2;
            }
            "--impact-file" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--impact-file requires an argument"));
                }
                impact_file = Some(args[i + 1].clone());
                i += 2;
            }
            "--depth" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--depth requires an argument"));
                }
                impact_depth = args[i + 1]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--depth must be a number"))?;
                i += 2;
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
        impact_symbol,
        impact_file,
        impact_depth,
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

/// Parse the `blast-score` command arguments
pub fn parse_blast_score_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut symbol: Option<String> = None;
    let mut file: Option<String> = None;
    let mut depth: usize = 3;
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
            "--symbol" | "--name" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--symbol requires an argument"));
                }
                symbol = Some(args[i + 1].clone());
                i += 2;
            }
            "--file" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--file requires an argument"));
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
    let symbol = symbol.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

    Ok(Command::BlastScore {
        db_path,
        symbol,
        file,
        depth,
        output_format,
    })
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
        "summary" => {
            let mut detail: Option<String> = None;
            let mut concise = false;

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
                    "--detail" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--detail requires an argument"));
                        }
                        detail = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--concise" => {
                        concise = true;
                        i += 1;
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

            ContextSubcommand::Summary { detail, concise }
        }
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
            let mut detail: Option<String> = None;
            let mut concise = false;
            let mut tokens: Option<usize> = None;

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
                    "--detail" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--detail requires an argument"));
                        }
                        detail = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--concise" => {
                        concise = true;
                        i += 1;
                    }
                    "--all" => {
                        all = true;
                        i += 1;
                    }
                    "--tokens" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--tokens requires an argument"));
                        }
                        tokens =
                            Some(args[i + 1].parse().map_err(|_| {
                                anyhow::anyhow!("--tokens must be a positive integer")
                            })?);
                        i += 2;
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
                detail,
                concise,
                tokens,
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
            let mut detail: Option<String> = None;
            let mut concise = false;
            let mut tokens: Option<usize> = None;

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
                    "--detail" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--detail requires an argument"));
                        }
                        detail = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--concise" => {
                        concise = true;
                        i += 1;
                    }
                    "--all" => {
                        all = true;
                        i += 1;
                    }
                    "--tokens" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--tokens requires an argument"));
                        }
                        tokens =
                            Some(args[i + 1].parse().map_err(|_| {
                                anyhow::anyhow!("--tokens must be a positive integer")
                            })?);
                        i += 2;
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
                detail,
                concise,
                tokens,
            }
        }
        "affected" => {
            let mut symbol: Option<String> = None;
            let mut file: Option<String> = None;
            let mut depth: usize = 3;
            let mut project: Option<String> = None;
            let mut output_format = OutputFormat::Human;
            let mut detail: Option<String> = None;
            let mut concise = false;
            let mut tokens: Option<usize> = None;

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
                    "--detail" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--detail requires an argument"));
                        }
                        detail = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--concise" => {
                        concise = true;
                        i += 1;
                    }
                    "--all" => {
                        all = true;
                        i += 1;
                    }
                    "--tokens" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--tokens requires an argument"));
                        }
                        tokens =
                            Some(args[i + 1].parse().map_err(|_| {
                                anyhow::anyhow!("--tokens must be a positive integer")
                            })?);
                        i += 2;
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
                detail,
                concise,
                tokens,
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
                "No enabled projects in registry. Use `magellan catalog` to list registered projects, then `magellan watch` to index one."
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
