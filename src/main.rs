//! Magellan CLI - Dumb, deterministic codebase mapping tool
//!
//! Usage: magellan <command> [arguments]

mod collisions_cmd;
mod export_cmd;
mod migrate_cmd;
mod files_cmd;
mod find_cmd;
mod get_cmd;
mod query_cmd;
mod refs_cmd;
mod verify_cmd;
mod watch_cmd;

use anyhow::Result;
use magellan::graph::export::ExportFilters;
use magellan::graph::query::CollisionField;
use magellan::output::{generate_execution_id, output_json, JsonResponse, StatusResponse};
use magellan::{CodeGraph, ExportFormat, OutputFormat, WatcherConfig};
use std::path::PathBuf;
use std::process::ExitCode;

fn version() {
    let version = env!("CARGO_PKG_VERSION");
    let commit = option_env!("MAGELLAN_COMMIT_SHA").unwrap_or("unknown");
    let date = option_env!("MAGELLAN_BUILD_DATE").unwrap_or("unknown");
    let rustc_version = option_env!("MAGELLAN_RUSTC_VERSION").unwrap_or("unknown");

    println!(
        "magellan {} ({} {}) rustc {}",
        version, commit, date, rustc_version
    );
}

fn print_usage() {
    eprintln!("Magellan - Multi-language codebase mapping tool");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  magellan <command> [arguments]");
    eprintln!("  magellan --help");
    eprintln!();
    eprintln!("  magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>] [--watch-only] [--validate] [--validate-only]");
    eprintln!(
        "  magellan export --db <FILE> [--format json|jsonl|csv|scip] [--output <PATH>] [--minify]"
    );
    eprintln!("  magellan status --db <FILE>");
    eprintln!("  magellan query --db <FILE> --file <PATH> [--kind <KIND>]");
    eprintln!("  magellan find --db <FILE> (--name <NAME> | --symbol-id <ID> | --ambiguous <NAME>) [--path <PATH>] [--first]");
    eprintln!("  magellan refs --db <FILE> (--name <NAME> [--path <PATH>] | --symbol-id <ID> --path <PATH>) [--direction <in|out>] [--output <FORMAT>]");
    eprintln!("  magellan get --db <FILE> --file <PATH> --symbol <NAME>");
    eprintln!("  magellan get-file --db <FILE> --file <PATH>");
    eprintln!("  magellan files --db <FILE> [--symbols] [--output <FORMAT>]");
    eprintln!("  magellan label --db <FILE> [--label <LABEL>]... [--list] [--count] [--show-code]");
    eprintln!("  magellan collisions --db <FILE> [--field <fqn|display_fqn|canonical_fqn>] [--limit <N>] [--output <FORMAT>]");
    eprintln!("  magellan migrate --db <FILE> [--dry-run] [--no-backup]");
    eprintln!("  magellan verify --root <DIR> --db <FILE>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  watch     Watch directory and index changes");
    eprintln!("  export    Export graph data to JSON/JSONL/CSV/SCIP");
    eprintln!("  status    Show database statistics");
    eprintln!("  query     List symbols in a file");
    eprintln!("  find      Find a symbol by name");
    eprintln!("  refs      Show calls for a symbol");
    eprintln!("  get       Get source code for a specific symbol");
    eprintln!("  get-file  Get all source code chunks for a file");
    eprintln!("  files     List all indexed files");
    eprintln!("  label     Query symbols by label (language, kind, etc.)");
    eprintln!("  collisions List ambiguous symbol groups for a chosen field");
    eprintln!("  migrate   Upgrade database to current schema version");
    eprintln!("  verify    Verify database vs filesystem");
    eprintln!();
    eprintln!("Global arguments:");
    eprintln!("  --output <FORMAT>   Output format: human (default), json (compact), or pretty (formatted)");
    eprintln!();
    eprintln!("Watch arguments:");
    eprintln!("  --root <DIR>        Directory to watch recursively");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --debounce-ms <N>   Debounce delay in milliseconds (default: 500)");
    eprintln!("  --watch-only        Watch for changes only; skip initial directory scan baseline");
    eprintln!("  --scan-initial      Scan directory for source files on startup (default: true; disabled by --watch-only)");
    eprintln!("  --validate          Enable pre-run and post-run validation checks");
    eprintln!(
        "  --validate-only     Run validation without indexing (pre + post validation, no watch)"
    );
    eprintln!();
    eprintln!("Export arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --format <FORMAT>   Export format: json (default), jsonl, csv, or scip");
    eprintln!("  --output <PATH>     Write to file instead of stdout");
    eprintln!("  --minify            Use compact JSON (no pretty-printing)");
    eprintln!("  --no-symbols        Exclude symbols from export");
    eprintln!("  --no-references     Exclude references from export");
    eprintln!("  --no-calls          Exclude calls from export");
    eprintln!("  --include-collisions Include collision groups (JSON only)");
    eprintln!("  --collisions-field <FIELD>  Collision field: fqn, display_fqn, canonical_fqn (default: fqn)");
    eprintln!();
    eprintln!("Status arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!();
    eprintln!("Query arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --file <PATH>       File path to query");
    eprintln!("  --kind <KIND>       Filter by symbol kind (optional)");
    eprintln!("  --with-context      Include source code context lines");
    eprintln!("  --with-callers      Include caller references");
    eprintln!("  --with-callees      Include callee references");
    eprintln!("  --with-semantics    Include symbol kind and language");
    eprintln!("  --with-checksums    Include content checksums");
    eprintln!("  --context-lines <N> Number of context lines (default: 3, max: 100)");
    eprintln!();
    eprintln!("Find arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --name <NAME>       Symbol name to find");
    eprintln!("  --symbol-id <ID>    Stable SymbolId for precise lookup");
    eprintln!("  --ambiguous <NAME>  Show all candidates for ambiguous display name");
    eprintln!("  --first             Use first match when ambiguous (deprecated)");
    eprintln!("  --path <PATH>       Limit search to specific file (optional)");
    eprintln!();
    eprintln!("Refs arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --name <NAME>       Symbol name to query");
    eprintln!("  --symbol-id <ID>    Use SymbolId instead of name for precise lookup");
    eprintln!("  --path <PATH>       File path containing the symbol");
    eprintln!("  --direction <in|out> Show incoming (in) or outgoing (out) calls (default: in)");
    eprintln!("  --with-context      Include source code context lines");
    eprintln!("  --with-semantics    Include symbol kind and language");
    eprintln!("  --with-checksums    Include content checksums");
    eprintln!("  --context-lines <N> Number of context lines (default: 3, max: 100)");
    eprintln!();
    eprintln!("Get arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --file <PATH>       File path containing the symbol");
    eprintln!("  --symbol <NAME>     Symbol name to retrieve");
    eprintln!("  --with-context      Include source code context lines");
    eprintln!("  --with-semantics    Include symbol kind and language");
    eprintln!("  --with-checksums    Include content checksums");
    eprintln!("  --context-lines <N> Number of context lines (default: 3, max: 100)");
    eprintln!();
    eprintln!("Get-file arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --file <PATH>       File path to retrieve code for");
    eprintln!();
    eprintln!("Files arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --symbols           Show symbol count per file");
    eprintln!();
    eprintln!("Label arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --label <LABEL>     Label to query (can specify multiple for AND semantics)");
    eprintln!("  --list             List all available labels with counts");
    eprintln!("  --count            Count entities with specified label(s)");
    eprintln!("  --show-code        Show source code for each matching symbol");
    eprintln!();
    eprintln!("Migrate arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --dry-run           Check version without migrating");
    eprintln!("  --no-backup         Skip backup creation");
    eprintln!();
    eprintln!("Verify arguments:");
    eprintln!("  --root <DIR>        Directory to verify against");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
}

enum Command {
    Watch {
        root_path: PathBuf,
        db_path: PathBuf,
        config: WatcherConfig,
        scan_initial: bool,
        validate: bool,
        validate_only: bool,
        output_format: OutputFormat,
    },
    Export {
        db_path: PathBuf,
        format: ExportFormat,
        output: Option<PathBuf>,
        include_symbols: bool,
        include_references: bool,
        include_calls: bool,
        minify: bool,
        include_collisions: bool,
        collisions_field: CollisionField,
        filters: ExportFilters,
    },
    Status {
        output_format: OutputFormat,
        db_path: PathBuf,
    },
    Query {
        db_path: PathBuf,
        file_path: Option<PathBuf>,
        root: Option<PathBuf>,
        kind: Option<String>,
        explain: bool,
        symbol: Option<String>,
        show_extent: bool,
        output_format: OutputFormat,
        with_context: bool,
        with_callers: bool,
        with_callees: bool,
        with_semantics: bool,
        with_checksums: bool,
        context_lines: usize,
    },
    Find {
        db_path: PathBuf,
        name: Option<String>,
        root: Option<PathBuf>,
        path: Option<PathBuf>,
        glob_pattern: Option<String>,
        symbol_id: Option<String>,
        ambiguous_name: Option<String>,
        first: bool,
        output_format: OutputFormat,
        with_context: bool,
        with_callers: bool,
        with_callees: bool,
        with_semantics: bool,
        with_checksums: bool,
        context_lines: usize,
    },
    Refs {
        db_path: PathBuf,
        name: String,
        root: Option<PathBuf>,
        path: PathBuf,
        symbol_id: Option<String>,
        direction: String,
        output_format: OutputFormat,
        with_context: bool,
        with_semantics: bool,
        with_checksums: bool,
        context_lines: usize,
    },
    Get {
        db_path: PathBuf,
        file_path: String,
        symbol_name: String,
        output_format: OutputFormat,
        with_context: bool,
        with_semantics: bool,
        with_checksums: bool,
        context_lines: usize,
    },
    GetFile {
        db_path: PathBuf,
        file_path: String,
    },
    Files {
        db_path: PathBuf,
        output_format: OutputFormat,
        with_symbols: bool,
    },
    Verify {
        root_path: PathBuf,
        db_path: PathBuf,
    },
    /// Query symbols by label (Phase 2: Label integration)
    Label {
        db_path: PathBuf,
        label: Vec<String>,
        list: bool,
        count: bool,
        show_code: bool,
    },
    Collisions {
        db_path: PathBuf,
        field: CollisionField,
        limit: usize,
        output_format: OutputFormat,
    },
    Migrate {
        db_path: PathBuf,
        dry_run: bool,
        no_backup: bool,
    },
}

fn parse_args() -> Result<Command> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        return Err(anyhow::anyhow!("Missing command"));
    }

    let command = &args[1];

    // Handle --version and -V flags
    if command == "--version" || command == "-V" {
        version();
        std::process::exit(0);
    }

    // Handle --help and -h flags
    if command == "--help" || command == "-h" {
        print_usage();
        std::process::exit(0);
    }

    match command.as_str() {
        "watch" => {
            let mut root_path: Option<PathBuf> = None;
            let mut db_path: Option<PathBuf> = None;
            let mut debounce_ms: u64 = 500;
            let mut watch_only = false;
            let mut scan_initial = true; // Default: true (scan on startup)
            let mut validate = false;
            let mut validate_only = false;
            let mut output_format = OutputFormat::Human;

            let mut i = 2;
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
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let root_path = root_path.ok_or_else(|| anyhow::anyhow!("--root is required"))?;
            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let config = WatcherConfig {
                root_path: root_path.clone(),
                debounce_ms,
            };

            // Precedence: --watch-only forces scan_initial to false
            let scan_initial = if watch_only { false } else { scan_initial };
            // Precedence: --validate-only implies validate=true
            let validate = if validate_only { true } else { validate };

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
        "export" => {
            let mut db_path: Option<PathBuf> = None;
            let mut format = ExportFormat::Json;
            let mut output: Option<PathBuf> = None;
            let mut include_symbols = true;
            let mut include_references = true;
            let mut include_calls = true;
            let mut minify = false;
            let mut include_collisions = false;
            let mut collisions_field = CollisionField::Fqn;
            let mut filter_file: Option<String> = None;
            let mut filter_symbol: Option<String> = None;
            let mut filter_kind: Option<String> = None;
            let mut filter_max_depth: Option<usize> = None;
            let mut filter_cluster = false;

            let mut i = 2;
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
                        format = ExportFormat::from_str(&args[i + 1])
                            .ok_or_else(|| anyhow::anyhow!("Invalid format: {}", args[i + 1]))?;
                        i += 2;
                    }
                    "--output" | "-o" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output = Some(PathBuf::from(&args[i + 1]));
                        i += 2;
                    }
                    "--minify" => {
                        minify = true;
                        i += 1;
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
                    "--include-collisions" => {
                        include_collisions = true;
                        i += 1;
                    }
                    "--collisions-field" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--collisions-field requires an argument"));
                        }
                        collisions_field =
                            CollisionField::from_str(&args[i + 1]).ok_or_else(|| {
                                anyhow::anyhow!("Invalid collisions field: {}", args[i + 1])
                            })?;
                        i += 2;
                    }
                    "--file" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--file requires an argument"));
                        }
                        filter_file = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--symbol" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--symbol requires an argument"));
                        }
                        filter_symbol = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--kind" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--kind requires an argument"));
                        }
                        filter_kind = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--max-depth" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--max-depth requires an argument"));
                        }
                        filter_max_depth = Some(
                            args[i + 1]
                                .parse()
                                .map_err(|_| anyhow::anyhow!("--max-depth must be a number"))?,
                        );
                        i += 2;
                    }
                    "--cluster" => {
                        filter_cluster = true;
                        i += 1;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let filters = ExportFilters {
                file: filter_file,
                symbol: filter_symbol,
                kind: filter_kind,
                max_depth: filter_max_depth,
                cluster: filter_cluster,
            };

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
        "status" => {
            let mut db_path: Option<PathBuf> = None;
            let mut output_format = OutputFormat::Human;

            let mut i = 2;
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
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Status {
                output_format,
                db_path,
            })
        }
        "query" => {
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
            let mut context_lines = 3usize;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--db" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--db requires an argument"));
                        }
                        db_path = Some(PathBuf::from(&args[i + 1]));
                        i += 2;
                    }
                    "--file" => {
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
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
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
                        context_lines = args[i + 1].parse().unwrap_or(3).min(100);
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            if !explain && file_path.is_none() {
                return Err(anyhow::anyhow!(
                    "--file is required unless --explain is set"
                ));
            }

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
        "find" => {
            let mut db_path: Option<PathBuf> = None;
            let mut name: Option<String> = None;
            let mut root: Option<PathBuf> = None;
            let mut path: Option<PathBuf> = None;
            let mut glob_pattern: Option<String> = None;
            let mut symbol_id: Option<String> = None;
            let mut ambiguous_name: Option<String> = None;
            let mut first = false;
            let mut output_format = OutputFormat::Human;
            let mut with_context = false;
            let mut with_callers = false;
            let mut with_callees = false;
            let mut with_semantics = false;
            let mut with_checksums = false;
            let mut context_lines = 3usize;

            let mut i = 2;
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
                    "--list-glob" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--list-glob requires an argument"));
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
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
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
                        context_lines = args[i + 1].parse().unwrap_or(3).min(100);
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            if glob_pattern.is_some() && name.is_some() {
                return Err(anyhow::anyhow!(
                    "Use either --name or --list-glob, not both"
                ));
            }

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
            })
        }
        "refs" => {
            let mut db_path: Option<PathBuf> = None;
            let mut name: Option<String> = None;
            let mut root: Option<PathBuf> = None;
            let mut path: Option<PathBuf> = None;
            let mut symbol_id: Option<String> = None;
            let mut direction = String::from("in"); // default
            let mut output_format = OutputFormat::Human;
            let mut with_context = false;
            let mut with_semantics = false;
            let mut with_checksums = false;
            let mut context_lines = 3usize;

            let mut i = 2;
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
                        i += 2;
                    }
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
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
                        context_lines = args[i + 1].parse().unwrap_or(3).min(100);
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let name = name.ok_or_else(|| anyhow::anyhow!("--name is required"))?;
            let path = path.ok_or_else(|| anyhow::anyhow!("--path is required"))?;

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
            })
        }
        "files" => {
            let mut db_path: Option<PathBuf> = None;
            let mut output_format = OutputFormat::Human;
            let mut with_symbols = false;

            let mut i = 2;
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
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
                        i += 2;
                    }
                    "--symbols" => {
                        with_symbols = true;
                        i += 1;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Files {
                db_path,
                output_format,
                with_symbols,
            })
        }
        "collisions" => {
            let mut db_path: Option<PathBuf> = None;
            let mut field = CollisionField::Fqn;
            let mut limit: usize = 50;
            let mut output_format = OutputFormat::Human;

            let mut i = 2;
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
                        field = CollisionField::from_str(&args[i + 1])
                            .ok_or_else(|| anyhow::anyhow!("Invalid field: {}", args[i + 1]))?;
                        i += 2;
                    }
                    "--limit" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--limit requires an argument"));
                        }
                        limit = args[i + 1]
                            .parse()
                            .map_err(|_| anyhow::anyhow!("--limit must be a number"))?;
                        i += 2;
                    }
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Collisions {
                db_path,
                field,
                limit,
                output_format,
            })
        }
        "migrate" => {
            let mut db_path: Option<PathBuf> = None;
            let mut dry_run = false;
            let mut no_backup = false;

            let mut i = 2;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Migrate {
                db_path,
                dry_run,
                no_backup,
            })
        }
        "verify" => {
            let mut root_path: Option<PathBuf> = None;
            let mut db_path: Option<PathBuf> = None;

            let mut i = 2;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let root_path = root_path.ok_or_else(|| anyhow::anyhow!("--root is required"))?;
            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Verify { root_path, db_path })
        }
        "get" => {
            let mut db_path: Option<PathBuf> = None;
            let mut file_path: Option<String> = None;
            let mut symbol_name: Option<String> = None;
            let mut output_format = OutputFormat::Human;
            let mut with_context = false;
            let mut with_semantics = false;
            let mut with_checksums = false;
            let mut context_lines = 3usize;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--db" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--db requires an argument"));
                        }
                        db_path = Some(PathBuf::from(&args[i + 1]));
                        i += 2;
                    }
                    "--file" => {
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
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
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
                        context_lines = args[i + 1].parse().unwrap_or(3).min(100);
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
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
        "get-file" => {
            let mut db_path: Option<PathBuf> = None;
            let mut file_path: Option<String> = None;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--db" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--db requires an argument"));
                        }
                        db_path = Some(PathBuf::from(&args[i + 1]));
                        i += 2;
                    }
                    "--file" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--file requires an argument"));
                        }
                        file_path = Some(args[i + 1].clone());
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let file_path = file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;

            Ok(Command::GetFile { db_path, file_path })
        }
        "chunks" => {
            let mut db_path: Option<PathBuf> = None;
            let mut output_format = OutputFormat::Human;
            let mut limit: Option<usize> = None;
            let mut file_filter: Option<String> = None;
            let mut kind_filter: Option<String> = None;

            let mut i = 2;
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
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
                        i += 2;
                    }
                    "--limit" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--limit requires an argument"));
                        }
                        limit = Some(args[i + 1].parse().map_err(|_| {
                            anyhow::anyhow!("--limit must be a number")
                        })?);
                        i += 2;
                    }
                    "--file" => {
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Chunks {
                db_path,
                output_format,
                limit,
                file_filter,
                kind_filter,
            })
        }
        "chunk-by-span" => {
            let mut db_path: Option<PathBuf> = None;
            let mut file_path: Option<String> = None;
            let mut byte_start: Option<usize> = None;
            let mut byte_end: Option<usize> = None;
            let mut output_format = OutputFormat::Human;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--db" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--db requires an argument"));
                        }
                        db_path = Some(PathBuf::from(&args[i + 1]));
                        i += 2;
                    }
                    "--file" => {
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
                        byte_start = Some(args[i + 1].parse().map_err(|_| {
                            anyhow::anyhow!("--start must be a number")
                        })?);
                        i += 2;
                    }
                    "--end" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--end requires an argument"));
                        }
                        byte_end = Some(args[i + 1].parse().map_err(|_| {
                            anyhow::anyhow!("--end must be a number")
                        })?);
                        i += 2;
                    }
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
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
        "chunk-by-symbol" => {
            let mut db_path: Option<PathBuf> = None;
            let mut symbol_name: Option<String> = None;
            let mut output_format = OutputFormat::Human;
            let mut file_filter: Option<String> = None;

            let mut i = 2;
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
                    "--output" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--output requires an argument"));
                        }
                        output_format = OutputFormat::from_str(&args[i + 1]).ok_or_else(|| {
                            anyhow::anyhow!("Invalid output format: {}", args[i + 1])
                        })?;
                        i += 2;
                    }
                    "--file" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--file requires an argument"));
                        }
                        file_filter = Some(args[i + 1].clone());
                        i += 2;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let symbol_name = symbol_name.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

            Ok(Command::ChunkBySymbol {
                db_path,
                symbol_name,
                output_format,
                file_filter,
            })
        }
        "label" => {
            let mut db_path: Option<PathBuf> = None;
            let mut labels: Vec<String> = Vec::new();
            let mut list = false;
            let mut count = false;
            let mut show_code = false;

            let mut i = 2;
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
                        labels.push(args[i + 1].clone());
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Label {
                db_path,
                label: labels,
                list,
                count,
                show_code,
            })
        }
        _ => Err(anyhow::anyhow!("Unknown command: {}", command)),
    }
}

/// Execution tracking wrapper
///
/// Records execution start/finish in execution_log.
/// Handles both success and error outcomes.
struct ExecutionTracker {
    exec_id: String,
    tool_version: String,
    args: Vec<String>,
    root: Option<String>,
    db_path: String,
    outcome: String,
    error_message: Option<String>,
    files_indexed: usize,
    symbols_indexed: usize,
    references_indexed: usize,
}

impl ExecutionTracker {
    fn new(args: Vec<String>, root: Option<String>, db_path: String) -> Self {
        Self {
            exec_id: generate_execution_id(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            args,
            root,
            db_path,
            outcome: "success".to_string(),
            error_message: None,
            files_indexed: 0,
            symbols_indexed: 0,
            references_indexed: 0,
        }
    }

    fn start(&self, graph: &CodeGraph) -> Result<()> {
        graph.execution_log().start_execution(
            &self.exec_id,
            &self.tool_version,
            &self.args,
            self.root.as_deref(),
            &self.db_path,
        )?;
        Ok(())
    }

    fn finish(&self, graph: &CodeGraph) -> Result<()> {
        graph.execution_log().finish_execution(
            &self.exec_id,
            &self.outcome,
            self.error_message.as_deref(),
            self.files_indexed,
            self.symbols_indexed,
            self.references_indexed,
        )
    }

    /// Set execution outcome to error with message
    ///
    /// Currently unused but provided for API completeness and future error handling.
    #[expect(dead_code)] // API completeness for future error handling
    fn set_error(&mut self, msg: String) {
        self.outcome = "error".to_string();
        self.error_message = Some(msg);
    }

    /// Set indexing counts for execution tracking
    ///
    /// Currently unused but provided for API completeness and future tracking.
    #[expect(dead_code)] // API completeness for future tracking
    fn set_counts(&mut self, files: usize, symbols: usize, references: usize) {
        self.files_indexed = files;
        self.symbols_indexed = symbols;
        self.references_indexed = references;
    }

    fn exec_id(&self) -> &str {
        &self.exec_id
    }
}

fn run_status(db_path: PathBuf, output_format: OutputFormat) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let tracker = ExecutionTracker::new(
        vec!["status".to_string()],
        None,
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;

    let file_count = graph.count_files()?;
    let symbol_count = graph.count_symbols()?;
    let reference_count = graph.count_references()?;
    let call_count = graph.count_calls()?;
    let chunk_count = graph.count_chunks()?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = StatusResponse {
                files: file_count,
                symbols: symbol_count,
                references: reference_count,
                calls: call_count,
                code_chunks: chunk_count,
            };
            let exec_id = tracker.exec_id().to_string();
            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
            println!("files: {}", file_count);
            println!("symbols: {}", symbol_count);
            println!("references: {}", reference_count);
            println!("calls: {}", call_count);
            println!("code_chunks: {}", chunk_count);
        }
    }

    tracker.finish(&graph)?;
    Ok(())
}

/// Run label query command
/// Usage: magellan label --db <FILE> --label <LABEL> [--list] [--count] [--show-code]
fn run_label(
    db_path: PathBuf,
    labels: Vec<String>,
    list: bool,
    count: bool,
    show_code: bool,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let mut args = vec!["label".to_string()];
    for label in &labels {
        args.push("--label".to_string());
        args.push(label.clone());
    }
    if list {
        args.push("--list".to_string());
    }
    if count {
        args.push("--count".to_string());
    }
    if show_code {
        args.push("--show-code".to_string());
    }

    let tracker = ExecutionTracker::new(args, None, db_path.to_string_lossy().to_string());
    tracker.start(&graph)?;

    // List all labels mode
    if list {
        let all_labels = graph.get_all_labels()?;
        println!("{} labels in use:", all_labels.len());
        for label in all_labels {
            let count = graph.count_entities_by_label(&label)?;
            println!("  {} ({})", label, count);
        }
        tracker.finish(&graph)?;
        return Ok(());
    }

    // Count mode
    if count {
        if labels.is_empty() {
            tracker.finish(&graph)?;
            return Err(anyhow::anyhow!("--count requires --label"));
        }
        for label in &labels {
            let entity_count = graph.count_entities_by_label(label)?;
            println!("{}: {} entities", label, entity_count);
        }
        tracker.finish(&graph)?;
        return Ok(());
    }

    // Query mode - get symbols by label(s)
    if labels.is_empty() {
        tracker.finish(&graph)?;
        return Err(anyhow::anyhow!(
            "No labels specified. Use --label <LABEL> or --list to see all labels"
        ));
    }

    let labels_ref: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let results = if labels.len() == 1 {
        graph.get_symbols_by_label(&labels[0])?
    } else {
        graph.get_symbols_by_labels(&labels_ref)?
    };

    if results.is_empty() {
        if labels.len() == 1 {
            println!("No symbols found with label '{}'", labels[0]);
        } else {
            println!("No symbols found with labels: {}", labels.join(", "));
        }
    } else {
        if labels.len() == 1 {
            println!("{} symbols with label '{}':", results.len(), labels[0]);
        } else {
            println!(
                "{} symbols with labels [{}]:",
                results.len(),
                labels.join(", ")
            );
        }

        for result in results {
            println!();
            println!(
                "  {} ({}) in {} [{}-{}]",
                result.name, result.kind, result.file_path, result.byte_start, result.byte_end
            );

            // Show code chunk if requested
            if show_code {
                // Get code chunk by exact byte span instead of by name
                // This avoids getting chunks for other symbols with the same name
                if let Ok(Some(chunk)) = graph.get_code_chunk_by_span(
                    &result.file_path,
                    result.byte_start,
                    result.byte_end,
                ) {
                    for line in chunk.content.lines() {
                        println!("    {}", line);
                    }
                }
            }
        }
    }

    tracker.finish(&graph)?;
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return ExitCode::from(1);
    }

    match parse_args() {
        Ok(Command::Status {
            output_format,
            db_path,
        }) => {
            if let Err(e) = run_status(db_path, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
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
        }) => {
            if let Err(e) = export_cmd::run_export(
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
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
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
        }) => {
            if let Err(e) = query_cmd::run_query(
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
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
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
        }) => {
            if let Err(e) = find_cmd::run_find(
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
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
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
        }) => {
            if let Err(e) = refs_cmd::run_refs(
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
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Files {
            db_path,
            output_format,
            with_symbols,
        }) => {
            if let Err(e) = files_cmd::run_files(db_path, with_symbols, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Collisions {
            db_path,
            field,
            limit,
            output_format,
        }) => {
            if let Err(e) = collisions_cmd::run_collisions(db_path, field, limit, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Migrate {
            db_path,
            dry_run,
            no_backup,
        }) => {
            match migrate_cmd::run_migrate(db_path, dry_run, no_backup) {
                Ok(result) => {
                    if result.success {
                        println!("{}", result.message);
                        if result.old_version != result.new_version {
                            println!("Version: {} -> {}", result.old_version, result.new_version);
                        }
                        if let Some(ref backup) = result.backup_path {
                            println!("Backup: {}", backup.display());
                        }
                    } else {
                        eprintln!("Migration failed: {}", result.message);
                        return ExitCode::from(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return ExitCode::from(1);
                }
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Get {
            db_path,
            file_path,
            symbol_name,
            output_format,
            with_context,
            with_semantics,
            with_checksums,
            context_lines,
        }) => {
            if let Err(e) = get_cmd::run_get(
                db_path,
                file_path,
                symbol_name,
                output_format,
                with_context,
                with_semantics,
                with_checksums,
                context_lines,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::GetFile { db_path, file_path }) => {
            if let Err(e) = get_cmd::run_get_file(db_path, file_path) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Chunks {
            db_path,
            output_format,
            limit,
            file_filter,
            kind_filter,
        }) => {
            if let Err(e) = get_cmd::run_chunks(db_path, output_format, limit, file_filter, kind_filter) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ChunkBySpan {
            db_path,
            file_path,
            byte_start,
            byte_end,
            output_format,
        }) => {
            if let Err(e) = get_cmd::run_chunk_by_span(db_path, file_path, byte_start, byte_end, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ChunkBySymbol {
            db_path,
            symbol_name,
            output_format,
            file_filter,
        }) => {
            if let Err(e) = get_cmd::run_chunk_by_symbol(db_path, symbol_name, output_format, file_filter) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Label {
            db_path,
            label,
            list,
            count,
            show_code,
        }) => {
            if let Err(e) = run_label(db_path, label, list, count, show_code) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Verify { root_path, db_path }) => {
            match verify_cmd::run_verify(root_path, db_path) {
                Ok(exit_code) => ExitCode::from(exit_code),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return ExitCode::from(1);
                }
            }
        }
        Ok(Command::Watch {
            root_path,
            db_path,
            config,
            scan_initial,
            validate,
            validate_only,
            output_format,
        }) => {
            if let Err(e) = watch_cmd::run_watch(
                root_path,
                db_path,
                config,
                scan_initial,
                validate,
                validate_only,
                output_format,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            print_usage();
            ExitCode::from(1)
        }
    }
}
