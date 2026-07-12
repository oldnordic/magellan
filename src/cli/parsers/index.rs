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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--fqn" => {
                let value = parse_required_arg(args, &mut i, "--fqn")?;
                fqn = Some(value);
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--file" | "--path" => {
                let flag = args[i].as_str();
                let value = parse_required_arg(args, &mut i, flag)?;
                file_path = Some(PathBuf::from(value));
            }
            "--root" => {
                let value = parse_required_arg(args, &mut i, "--root")?;
                root = Some(PathBuf::from(value));
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
    let mut compile_commands: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                let value = parse_required_arg(args, &mut i, "--root")?;
                root_path = Some(PathBuf::from(value));
            }
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--debounce-ms" => {
                let value = parse_required_arg(args, &mut i, "--debounce-ms")?;
                debounce_ms = value.parse()?;
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

            "--compile-commands" => {
                let value = parse_required_arg(args, &mut i, "--compile-commands")?;
                compile_commands = Some(PathBuf::from(value));
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
        compile_commands,
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--format" => {
                let value = parse_required_arg(args, &mut i, "--format")?;
                format = match value.as_str() {
                    "json" => ExportFormat::Json,
                    "jsonl" => ExportFormat::JsonL,
                    "csv" => ExportFormat::Csv,
                    "scip" => ExportFormat::Scip,
                    "dot" => ExportFormat::Dot,
                    "lsif" => ExportFormat::Lsif,
                    "impact" => ExportFormat::Impact,
                    _ => return Err(anyhow::anyhow!("Invalid format: {}", value)),
                };
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output = Some(PathBuf::from(value));
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
                let value = parse_required_arg(args, &mut i, "--collisions-field")?;
                collisions_field = match value.as_str() {
                    "fqn" => CollisionField::Fqn,
                    "display_fqn" => CollisionField::DisplayFqn,
                    "canonical_fqn" => CollisionField::CanonicalFqn,
                    _ => return Err(anyhow::anyhow!("Invalid collisions field: {}", value)),
                };
            }
            "--filter-file" | "--file" => {
                let flag = args[i].as_str();
                let value = parse_required_arg(args, &mut i, flag)?;
                filters.file = Some(value);
            }
            "--filter-kind" => {
                let value = parse_required_arg(args, &mut i, "--filter-kind")?;
                filters.kind = Some(value);
            }
            "--cluster" => {
                filters.cluster = true;
                i += 1;
            }
            "--symbol" => {
                let value = parse_required_arg(args, &mut i, "--symbol")?;
                impact_symbol = Some(value);
            }
            "--impact-file" => {
                let value = parse_required_arg(args, &mut i, "--impact-file")?;
                impact_file = Some(value);
            }
            "--depth" => {
                let value = parse_required_arg(args, &mut i, "--depth")?;
                impact_depth = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--depth must be a number"))?;
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--input" | "--file" => {
                let flag = args[i].as_str();
                let value = parse_required_arg(args, &mut i, flag)?;
                lsif_paths.push(PathBuf::from(value));
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--lcov" => {
                let value = parse_required_arg(args, &mut i, "--lcov")?;
                lcov_path = Some(PathBuf::from(value));
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--symbol" | "--name" => {
                let flag = args[i].as_str();
                let value = parse_required_arg(args, &mut i, flag)?;
                symbol = Some(value);
            }
            "--file" => {
                let value = parse_required_arg(args, &mut i, "--file")?;
                file = Some(value);
            }
            "--depth" => {
                let value = parse_required_arg(args, &mut i, "--depth")?;
                depth = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--depth must be a positive integer"))?;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--file" | "--path" => {
                let flag = args[i].as_str();
                let value = parse_required_arg(args, &mut i, flag)?;
                let file = PathBuf::from(value);
                files.get_or_insert_with(Vec::new).push(file);
            }
            "--timeout" => {
                let value = parse_required_arg(args, &mut i, "--timeout")?;
                timeout_secs = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid timeout: {}. Must be a number", value))?;
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_paths.extend(parse_db_paths(&value)?);
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                let _ = parse_output_format(&value)?;
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
                        let value = parse_required_arg(args, &mut i, "--db")?;
                        db_paths.extend(parse_db_paths(&value)?);
                    }
                    "--detail" => {
                        let value = parse_required_arg(args, &mut i, "--detail")?;
                        detail = Some(value);
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
                        let value = parse_required_arg(args, &mut i, "--db")?;
                        db_paths.extend(parse_db_paths(&value)?);
                    }
                    "--kind" => {
                        let value = parse_required_arg(args, &mut i, "--kind")?;
                        kind = Some(value);
                    }
                    "--page" => {
                        let value = parse_required_arg(args, &mut i, "--page")?;
                        page = Some(
                            value
                                .parse()
                                .map_err(|_| anyhow::anyhow!("Invalid page number"))?,
                        );
                    }
                    "--page-size" => {
                        let value = parse_required_arg(args, &mut i, "--page-size")?;
                        page_size = Some(
                            value
                                .parse()
                                .map_err(|_| anyhow::anyhow!("Invalid page size"))?,
                        );
                    }
                    "--cursor" => {
                        let value = parse_required_arg(args, &mut i, "--cursor")?;
                        cursor = Some(value);
                    }
                    "--project" => {
                        let value = parse_required_arg(args, &mut i, "--project")?;
                        project = Some(value);
                    }
                    "--output" => {
                        let value = parse_required_arg(args, &mut i, "--output")?;
                        output_format = parse_output_format(&value)?;
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
                        let value = parse_required_arg(args, &mut i, "--db")?;
                        db_paths.extend(parse_db_paths(&value)?);
                    }
                    "--name" => {
                        let value = parse_required_arg(args, &mut i, "--name")?;
                        name = Some(value);
                    }
                    "--file" | "--path" => {
                        let flag = args[i].as_str();
                        let value = parse_required_arg(args, &mut i, flag)?;
                        file = Some(value);
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
                        let value = parse_required_arg(args, &mut i, "--output")?;
                        output_format = parse_output_format(&value)?;
                    }
                    "--with-source" => {
                        with_source = true;
                        i += 1;
                    }
                    "--depth" => {
                        let value = parse_required_arg(args, &mut i, "--depth")?;
                        let d: usize = value
                            .parse()
                            .map_err(|_| anyhow::anyhow!("--depth must be a positive integer"))?;
                        depth = Some(d);
                    }
                    "--project" => {
                        let value = parse_required_arg(args, &mut i, "--project")?;
                        project = Some(value);
                    }
                    "--detail" => {
                        let value = parse_required_arg(args, &mut i, "--detail")?;
                        detail = Some(value);
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
                        let value = parse_required_arg(args, &mut i, "--tokens")?;
                        tokens =
                            Some(value.parse().map_err(|_| {
                                anyhow::anyhow!("--tokens must be a positive integer")
                            })?);
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
