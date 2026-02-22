//! CLI argument parsing for Magellan
//!
//! Defines the Command enum and parse_args() function for all CLI commands.

use anyhow::Result;
use magellan::graph::export::ExportFilters;
use magellan::graph::query::CollisionField;
use magellan::{ExportFormat, OutputFormat, WatcherConfig};
use std::path::PathBuf;

pub fn print_usage() {
    eprintln!("Magellan - Multi-language codebase mapping tool");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  magellan <command> [arguments]");
    eprintln!("  magellan --help");
    eprintln!();
    eprintln!("  magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>] [--watch-only] [--validate] [--validate-only]");
    eprintln!(
        "  magellan export --db <FILE> [--format json|jsonl|csv|scip|dot] [--output <PATH>] [--minify] [--cluster]"
    );
    eprintln!("  magellan status --db <FILE>");
    eprintln!("  magellan query --db <FILE> --file <PATH> [--kind <KIND>]");
    eprintln!("  magellan find --db <FILE> (--name <NAME> | --symbol-id <ID> | --ambiguous <NAME>) [--path <PATH>] [--first]");
    eprintln!("  magellan refs --db <FILE> (--name <NAME> [--path <PATH>] | --symbol-id <ID> --path <PATH>) [--direction <in|out>] [--output <FORMAT>]");
    eprintln!("  magellan get --db <FILE> --file <PATH> --symbol <NAME>");
    eprintln!("  magellan get-file --db <FILE> --file <PATH>");
    eprintln!("  magellan chunks --db <FILE> [--limit N] [--file PATTERN] [--kind KIND] [--output FORMAT]");
    eprintln!("  magellan chunk-by-span --db <FILE> --file <PATH> --start <N> --end <N> [--output FORMAT]");
    eprintln!("  magellan chunk-by-symbol --db <FILE> --symbol <NAME> [--file PATTERN] [--output FORMAT]");
    eprintln!("  magellan files --db <FILE> [--symbols] [--output <FORMAT>]");
    eprintln!("  magellan label --db <FILE> [--label <LABEL>]... [--list] [--count] [--show-code]");
    eprintln!("  magellan collisions --db <FILE> [--field <fqn|display_fqn|canonical_fqn>] [--limit <N>] [--output <FORMAT>]");
    eprintln!("  magellan migrate --db <FILE> [--dry-run] [--no-backup] [--output <FORMAT>]");
    eprintln!("  magellan migrate-backend --input <DB> --output <DB> [--export-dir <DIR>] [--dry-run] [--output <FORMAT>]");
    eprintln!("  magellan verify --root <DIR> --db <FILE>");
    eprintln!("  magellan ast --db <FILE> --file <PATH> [--position <OFFSET>] [--output <FORMAT>]");
    eprintln!("  magellan find-ast --db <FILE> --kind <KIND> [--output <FORMAT>]");
    eprintln!("  magellan reachable --db <FILE> --symbol <SYMBOL_ID> [--reverse] [--output <FORMAT>]");
    eprintln!("  magellan dead-code --db <FILE> --entry <SYMBOL_ID> [--output <FORMAT>]");
    eprintln!("  magellan cycles --db <FILE> [--symbol <SYMBOL_ID>] [--output <FORMAT>]");
    eprintln!("  magellan condense --db <FILE> [--members] [--output <FORMAT>]");
    eprintln!("  magellan paths --db <FILE> --start <SYMBOL_ID> [--end <SYMBOL_ID>] [--max-depth <N>] [--max-paths <N>] [--output <FORMAT>]");
    eprintln!("  magellan slice --db <FILE> --target <SYMBOL_ID> [--direction <backward|forward>] [--verbose] [--output <FORMAT>]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  watch           Watch directory and index changes");
    eprintln!("  export          Export graph data to JSON/JSONL/CSV/SCIP");
    eprintln!("  status          Show database statistics");
    eprintln!("  query           List symbols in a file");
    eprintln!("  find            Find a symbol by name");
    eprintln!("  refs            Show calls for a symbol");
    eprintln!("  get             Get source code for a specific symbol");
    eprintln!("  get-file        Get all source code chunks for a file");
    eprintln!("  chunks          List all code chunks in database");
    eprintln!("  chunk-by-span   Get chunk by file path and byte range");
    eprintln!("  chunk-by-symbol Get all chunks for a symbol name");
    eprintln!("  files           List all indexed files");
    eprintln!("  label           Query symbols by label (language, kind, etc.)");
    eprintln!("  collisions      List ambiguous symbol groups for a chosen field");
    eprintln!("  migrate         Upgrade database to current schema version");
    eprintln!("  migrate-backend Migrate database between SQLite backends");
    eprintln!("  verify          Verify database vs filesystem");
    eprintln!("  ast             Query AST nodes for a file");
    eprintln!("  find-ast        Find AST nodes by kind");
    eprintln!("  reachable       Show symbols reachable from a given symbol (SQLite backend only)");
    eprintln!("  dead-code       Find dead code unreachable from an entry point (SQLite backend only)");
    eprintln!("  cycles          Detect strongly connected components (cycles) in the call graph (SQLite backend only)");
    eprintln!("  condense        Show call graph condensation (SCCs collapsed into supernodes) (SQLite backend only)");
    eprintln!("  paths           Enumerate execution paths between symbols (SQLite backend only)");
    eprintln!("  slice           Program slicing (backward/forward) from a target symbol (SQLite backend only)");
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
    eprintln!("  --gitignore-aware   Enable .gitignore filtering (default: true)");
    eprintln!("  --no-gitignore      Disable .gitignore filtering (index all files)");
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
    eprintln!("Chunks arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --limit N           Limit number of chunks returned");
    eprintln!("  --file PATTERN      Filter by file path pattern (substring match)");
    eprintln!("  --kind KIND         Filter by symbol kind (fn, struct, method, class, etc.)");
    eprintln!();
    eprintln!("Chunk-by-span arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --file <PATH>       File path containing the chunk (required)");
    eprintln!("  --start N           Byte offset where chunk starts (required)");
    eprintln!("  --end N             Byte offset where chunk ends (required)");
    eprintln!();
    eprintln!("Chunk-by-symbol arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --symbol <NAME>     Symbol name to find (required)");
    eprintln!("  --file PATTERN      Filter by file path pattern (optional)");
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
    eprintln!("  --output <FORMAT>   Output format: human (default), json (compact), or pretty (formatted)");
    eprintln!();
    eprintln!("Backend migration arguments:");
    eprintln!("  --input <DB>        Path to input database (SQLite)");
    eprintln!("  --output <DB>       Path to output database (SQLite)");
    eprintln!("  --export-dir <DIR>  Directory for snapshot files (default: temp dir)");
    eprintln!("  --dry-run           Show what would be migrated without doing it");
    eprintln!();
    eprintln!("Verify arguments:");
    eprintln!("  --root <DIR>        Directory to verify against");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!();
    eprintln!("Slice arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --target <ID>       Target symbol ID to slice from");
    eprintln!("  --direction <DIR>   Slice direction: backward (default) or forward");
    eprintln!("  --verbose           Show detailed statistics");
}

#[derive(Debug)]
pub enum Command {
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
        output_format: OutputFormat,
    },
    /// Backend migration (Phase 47)
    MigrateBackend {
        input_db: PathBuf,
        output_db: PathBuf,
        export_dir: Option<PathBuf>,
        dry_run: bool,
        output_format: OutputFormat,
    },
    Chunks {
        db_path: PathBuf,
        output_format: OutputFormat,
        limit: Option<usize>,
        file_filter: Option<String>,
        kind_filter: Option<String>,
    },
    ChunkBySpan {
        db_path: PathBuf,
        file_path: String,
        byte_start: usize,
        byte_end: usize,
        output_format: OutputFormat,
    },
    ChunkBySymbol {
        db_path: PathBuf,
        symbol_name: String,
        file_filter: Option<String>,
        output_format: OutputFormat,
    },
    Ast {
        db_path: PathBuf,
        file_path: String,
        position: Option<usize>,
        output_format: OutputFormat,
    },
    FindAst {
        db_path: PathBuf,
        kind: String,
        output_format: OutputFormat,
    },
    /// Reachability analysis (Phase 40)
    Reachable {
        db_path: PathBuf,
        symbol_id: String,
        reverse: bool,
        output_format: OutputFormat,
    },
    /// Dead code detection (Phase 40)
    /// Cycles detection (Phase 40)
    Cycles {
        db_path: PathBuf,
        symbol_id: Option<String>,
        output_format: OutputFormat,
    },
    /// Condensation graph (Phase 40)
    Condense {
        db_path: PathBuf,
        show_members: bool,
        output_format: OutputFormat,
    },
    DeadCode {
        db_path: PathBuf,
        entry_symbol_id: String,
        output_format: OutputFormat,
    },
    /// Path enumeration (Phase 40)
    Paths {
        db_path: PathBuf,
        start_symbol_id: String,
        end_symbol_id: Option<String>,
        max_depth: usize,
        max_paths: usize,
        output_format: OutputFormat,
    },
    /// Program slicing (Phase 40)
    Slice {
        db_path: PathBuf,
        target: String,
        direction: String,
        verbose: bool,
        output_format: OutputFormat,
    },
}

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

/// Helper to parse a required string argument
/// 
/// Returns the next argument value and increments index by 2,
/// or returns an error if no value is provided.
fn parse_required_arg(args: &[String], i: &mut usize, flag: &str) -> Result<String> {
    if *i + 1 >= args.len() {
        return Err(anyhow::anyhow!("{} requires an argument", flag));
    }
    let value = args[*i + 1].clone();
    *i += 2;
    Ok(value)
}

/// Helper to parse an optional string argument
/// 
/// Returns Some(value) and increments index by 2 if next arg exists,
/// otherwise returns None and increments by 1.
fn parse_optional_arg(args: &[String], i: &mut usize) -> Option<String> {
    if *i + 1 < args.len() {
        let value = args[*i + 1].clone();
        *i += 2;
        Some(value)
    } else {
        *i += 1;
        None
    }
}

/// Helper to parse output format from string
/// 
/// Accepts: "human", "json", "pretty"
fn parse_output_format(value: &str) -> Result<OutputFormat> {
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

/// Helper to parse a PathBuf argument
fn parse_path_arg(args: &[String], i: &mut usize, flag: &str) -> Result<PathBuf> {
    let value = parse_required_arg(args, i, flag)?;
    Ok(PathBuf::from(value))
}

/// Helper to parse an integer argument
fn parse_int_arg<T: std::str::FromStr>(args: &[String], i: &mut usize, flag: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    let value = parse_required_arg(args, i, flag)?;
    value.parse::<T>().map_err(|e| {
        anyhow::anyhow!("Invalid value for {}: {}", flag, e)
    })
}

/// Parse the `watch` command arguments
/// 
/// # Arguments
/// * `args` - The command line arguments (starting from index 2, after "watch")
///
/// # Returns
/// The parsed Watch command or an error
fn parse_watch_args(args: &[String]) -> Result<Command> {
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

    let root_path = root_path.ok_or_else(|| anyhow::anyhow!("--root is required"))?;
    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

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
fn parse_export_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

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

/// Parse the `status` command arguments
fn parse_status_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => db_path = Some(parse_path_arg(args, &mut i, "--db")?),
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    Ok(Command::Status {
        output_format,
        db_path,
    })
}

/// Parse the `find` command arguments
fn parse_find_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

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
        print_usage();
        std::process::exit(0);
    }

    match command.as_str() {
        "watch" => parse_watch_args(&args[2..]),
        "export" => parse_export_args(&args[2..]),
        "status" => parse_status_args(&args[2..]),
        "query" => parse_query_args(&args[2..]),
        "find" => parse_find_args(&args[2..]),
        "refs" => parse_refs_args(&args[2..]),
        "get" => parse_get_args(&args[2..]),
        "get-file" => parse_get_file_args(&args[2..]),
        "files" => parse_files_args(&args[2..]),
        "verify" => parse_verify_args(&args[2..]),
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
        "condense" => parse_condense_args(&args[2..]),
        "paths" => parse_paths_args(&args[2..]),
        "slice" => parse_slice_args(&args[2..]),
        _ => Err(anyhow::anyhow!("Unknown command: {}", command)),
    }
}

/// Parse the `files` command arguments
fn parse_files_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    Ok(Command::Files {
        db_path,
        output_format,
        with_symbols,
    })
}

/// Parse the `get` command arguments
fn parse_get_args(args: &[String]) -> Result<Command> {
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

/// Parse the `get-file` command arguments
fn parse_get_file_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut file_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => db_path = Some(parse_path_arg(args, &mut i, "--db")?),
            "--file" => file_path = Some(parse_required_arg(args, &mut i, "--file")?),
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
    let file_path = file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;

    Ok(Command::GetFile {
        db_path,
        file_path,
    })
}

/// Parse the `refs` command arguments
fn parse_refs_args(args: &[String]) -> Result<Command> {
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

/// Parse the `verify` command arguments
fn parse_verify_args(args: &[String]) -> Result<Command> {
    let mut root_path: Option<PathBuf> = None;
    let mut db_path: Option<PathBuf> = None;

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
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let root_path = root_path.ok_or_else(|| anyhow::anyhow!("--root is required"))?;
    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    Ok(Command::Verify {
        root_path,
        db_path,
    })
}

/// Parse the `label` command arguments
fn parse_label_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut label = Vec::new();
    let mut list = false;
    let mut count = false;
    let mut show_code = false;

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
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    Ok(Command::Label {
        db_path,
        label,
        list,
        count,
        show_code,
    })
}

/// Parse the `collisions` command arguments
fn parse_collisions_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    Ok(Command::Collisions {
        db_path,
        field,
        limit,
        output_format,
    })
}

/// Parse the `migrate` command arguments
fn parse_migrate_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    Ok(Command::Migrate {
        db_path,
        dry_run,
        no_backup,
        output_format,
    })
}

/// Parse the `migrate-backend` command arguments
fn parse_migrate_backend_args(args: &[String]) -> Result<Command> {
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
fn parse_query_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

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
fn parse_chunks_args(args: &[String]) -> Result<Command> {
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
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
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

/// Parse the `chunk-by-span` command arguments
fn parse_chunk_by_span_args(args: &[String]) -> Result<Command> {
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

/// Parse the `chunk-by-symbol` command arguments
fn parse_chunk_by_symbol_args(args: &[String]) -> Result<Command> {
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
            "--file" => {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
    let symbol_name = symbol_name.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

    Ok(Command::ChunkBySymbol {
        db_path,
        symbol_name,
        file_filter,
        output_format,
    })
}

/// Parse the `ast` command arguments
fn parse_ast_args(args: &[String]) -> Result<Command> {
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
            "--file" => {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
    let file_path = file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;

    Ok(Command::Ast {
        db_path,
        file_path,
        position,
        output_format,
    })
}

/// Parse the `find-ast` command arguments
fn parse_find_ast_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
    let kind = kind.ok_or_else(|| anyhow::anyhow!("--kind is required"))?;

    Ok(Command::FindAst {
        db_path,
        kind,
        output_format,
    })
}

/// Parse the `reachable` command arguments
fn parse_reachable_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
    let symbol_id = symbol_id.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

    Ok(Command::Reachable {
        db_path,
        symbol_id,
        reverse,
        output_format,
    })
}

/// Parse the `dead-code` command arguments
fn parse_dead_code_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
    let entry_symbol_id = entry_symbol_id.ok_or_else(|| anyhow::anyhow!("--entry is required"))?;

    Ok(Command::DeadCode {
        db_path,
        entry_symbol_id,
        output_format,
    })
}

/// Parse the `cycles` command arguments
fn parse_cycles_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    Ok(Command::Cycles {
        db_path,
        symbol_id,
        output_format,
    })
}

/// Parse the `condense` command arguments
fn parse_condense_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    Ok(Command::Condense {
        db_path,
        show_members,
        output_format,
    })
}

/// Parse the `paths` command arguments
fn parse_paths_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
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
fn parse_slice_args(args: &[String]) -> Result<Command> {
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

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
    let target = target.ok_or_else(|| anyhow::anyhow!("--target is required"))?;

    Ok(Command::Slice {
        db_path,
        target,
        direction,
        verbose,
        output_format,
    })
}

/// Convenience wrapper around parse_args_impl that uses the version module
pub fn parse_args() -> Result<Command> {
    parse_args_impl(|| {
        println!("{}", crate::version::version());
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that watch command parsing works correctly
    /// This test ensures the refactoring doesn't break existing functionality
    #[test]
    fn test_parse_watch_command() {
        // Note: We can't easily test parse_args_impl directly since it uses std::env::args()
        // Instead, we verify the Command enum structure is correct
        let cmd = Command::Watch {
            root_path: PathBuf::from("."),
            db_path: PathBuf::from("test.db"),
            config: WatcherConfig {
                root_path: PathBuf::from("."),
                debounce_ms: 500,
                gitignore_aware: true,
            },
            scan_initial: true,
            validate: false,
            validate_only: false,
            output_format: OutputFormat::Human,
        };
        
        // Verify we can construct the command
        match cmd {
            Command::Watch { root_path, db_path, .. } => {
                assert_eq!(root_path, PathBuf::from("."));
                assert_eq!(db_path, PathBuf::from("test.db"));
            }
            _ => panic!("Expected Watch command"),
        }
    }

    /// Test that find command parsing structure is correct
    #[test]
    fn test_parse_find_command_structure() {
        let cmd = Command::Find {
            db_path: PathBuf::from("test.db"),
            name: Some("test_function".to_string()),
            root: None,
            path: None,
            glob_pattern: None,
            symbol_id: None,
            ambiguous_name: None,
            first: false,
            output_format: OutputFormat::Json,
            with_context: false,
            with_callers: false,
            with_callees: false,
            with_semantics: false,
            with_checksums: false,
            context_lines: 3,
        };
        
        match cmd {
            Command::Find { name, output_format, .. } => {
                assert_eq!(name, Some("test_function".to_string()));
                assert!(matches!(output_format, OutputFormat::Json));
            }
            _ => panic!("Expected Find command"),
        }
    }

    // Tests for extracted parser functions
    
    #[test]
    fn test_parse_watch_args() {
        let args = vec![
            "--root".to_string(), "/home/test".to_string(),
            "--db".to_string(), "test.db".to_string(),
            "--debounce-ms".to_string(), "1000".to_string(),
            "--watch-only".to_string(),
        ];
        
        let result = parse_watch_args(&args).unwrap();
        match result {
            Command::Watch { root_path, db_path, config, scan_initial, .. } => {
                assert_eq!(root_path, PathBuf::from("/home/test"));
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(config.debounce_ms, 1000);
                assert!(!scan_initial); // watch-only implies no initial scan
            }
            _ => panic!("Expected Watch command"),
        }
    }

    #[test]
    fn test_parse_watch_args_missing_required() {
        let args = vec!["--root".to_string(), "/home/test".to_string()];
        
        let result = parse_watch_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--db is required"));
    }

    #[test]
    fn test_parse_export_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--format".to_string(), "json".to_string(),
            "--output".to_string(), "output.json".to_string(),
        ];
        
        let result = parse_export_args(&args).unwrap();
        match result {
            Command::Export { db_path, format, output, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(format, ExportFormat::Json);
                assert_eq!(output, Some(PathBuf::from("output.json")));
            }
            _ => panic!("Expected Export command"),
        }
    }

    #[test]
    fn test_parse_status_args() {
        let args = vec!["--db".to_string(), "test.db".to_string()];
        
        let result = parse_status_args(&args).unwrap();
        match result {
            Command::Status { db_path, output_format } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert!(matches!(output_format, OutputFormat::Human));
            }
            _ => panic!("Expected Status command"),
        }
    }

    #[test]
    fn test_parse_find_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "my_function".to_string(),
            "--first".to_string(),
            "--output".to_string(), "json".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { db_path, name, first, output_format, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(name, Some("my_function".to_string()));
                assert!(first);
                assert!(matches!(output_format, OutputFormat::Json));
            }
            _ => panic!("Expected Find command"),
        }
    }

    #[test]
    fn test_parse_find_args_by_symbol_id() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--symbol-id".to_string(), "abc123".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { db_path, symbol_id, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(symbol_id, Some("abc123".to_string()));
            }
            _ => panic!("Expected Find command"),
        }
    }

    #[test]
    fn test_parse_find_args_without_name_or_symbol() {
        // Find can work without --name or --symbol-id (lists all symbols)
        let args = vec!["--db".to_string(), "test.db".to_string()];
        
        let result = parse_find_args(&args);
        assert!(result.is_ok());
        
        match result.unwrap() {
            Command::Find { db_path, name, symbol_id, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(name, None);
                assert_eq!(symbol_id, None);
            }
            _ => panic!("Expected Find command"),
        }
    }

    #[test]
    fn test_parse_refs_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "my_function".to_string(),
            "--path".to_string(), "src/main.rs".to_string(),
            "--direction".to_string(), "out".to_string(),
        ];
        
        let result = parse_refs_args(&args).unwrap();
        match result {
            Command::Refs { db_path, name, direction, path, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(name, "my_function".to_string());
                assert_eq!(path, PathBuf::from("src/main.rs"));
                assert_eq!(direction, "out");
            }
            _ => panic!("Expected Refs command"),
        }
    }

    #[test]
    fn test_parse_get_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--file".to_string(), "src/main.rs".to_string(),
            "--symbol".to_string(), "main".to_string(),
            "--with-context".to_string(),
        ];
        
        let result = parse_get_args(&args).unwrap();
        match result {
            Command::Get { db_path, file_path, symbol_name, with_context, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(file_path, "src/main.rs".to_string());
                assert_eq!(symbol_name, "main".to_string());
                assert!(with_context);
            }
            _ => panic!("Expected Get command"),
        }
    }

    #[test]
    fn test_parse_get_file_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--file".to_string(), "src/main.rs".to_string(),
        ];
        
        let result = parse_get_file_args(&args).unwrap();
        match result {
            Command::GetFile { db_path, file_path } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(file_path, "src/main.rs".to_string());
            }
            _ => panic!("Expected GetFile command"),
        }
    }

    #[test]
    fn test_parse_files_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--symbols".to_string(),
            "--output".to_string(), "json".to_string(),
        ];
        
        let result = parse_files_args(&args).unwrap();
        match result {
            Command::Files { db_path, with_symbols, output_format } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert!(with_symbols);
                assert!(matches!(output_format, OutputFormat::Json));
            }
            _ => panic!("Expected Files command"),
        }
    }

    #[test]
    fn test_parse_verify_args() {
        let args = vec![
            "--root".to_string(), "/home/test".to_string(),
            "--db".to_string(), "test.db".to_string(),
        ];
        
        let result = parse_verify_args(&args).unwrap();
        match result {
            Command::Verify { root_path, db_path } => {
                assert_eq!(root_path, PathBuf::from("/home/test"));
                assert_eq!(db_path, PathBuf::from("test.db"));
            }
            _ => panic!("Expected Verify command"),
        }
    }

    #[test]
    fn test_parse_label_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--label".to_string(), "important".to_string(),
            "--label".to_string(), "refactored".to_string(),
            "--list".to_string(),
        ];
        
        let result = parse_label_args(&args).unwrap();
        match result {
            Command::Label { db_path, label, list, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(label, vec!["important", "refactored"]);
                assert!(list);
            }
            _ => panic!("Expected Label command"),
        }
    }

    #[test]
    fn test_parse_collisions_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--field".to_string(), "display_fqn".to_string(),
            "--limit".to_string(), "50".to_string(),
        ];
        
        let result = parse_collisions_args(&args).unwrap();
        match result {
            Command::Collisions { db_path, field, limit, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert!(matches!(field, CollisionField::DisplayFqn));
                assert_eq!(limit, 50);
            }
            _ => panic!("Expected Collisions command"),
        }
    }

    #[test]
    fn test_parse_migrate_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--dry-run".to_string(),
            "--no-backup".to_string(),
        ];
        
        let result = parse_migrate_args(&args).unwrap();
        match result {
            Command::Migrate { db_path, dry_run, no_backup, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert!(dry_run);
                assert!(no_backup);
            }
            _ => panic!("Expected Migrate command"),
        }
    }

    #[test]
    fn test_parse_migrate_backend_args() {
        let args = vec![
            "--input".to_string(), "old.db".to_string(),
            "--output".to_string(), "new.db".to_string(),
            "--dry-run".to_string(),
        ];
        
        let result = parse_migrate_backend_args(&args).unwrap();
        match result {
            Command::MigrateBackend { input_db, output_db, dry_run, .. } => {
                assert_eq!(input_db, PathBuf::from("old.db"));
                assert_eq!(output_db, PathBuf::from("new.db"));
                assert!(dry_run);
            }
            _ => panic!("Expected MigrateBackend command"),
        }
    }

    #[test]
    fn test_parse_query_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--file".to_string(), "src/main.rs".to_string(),
            "--kind".to_string(), "function".to_string(),
            "--explain".to_string(),
        ];
        
        let result = parse_query_args(&args).unwrap();
        match result {
            Command::Query { db_path, file_path, kind, explain, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(file_path, Some(PathBuf::from("src/main.rs")));
                assert_eq!(kind, Some("function".to_string()));
                assert!(explain);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_parse_chunks_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--limit".to_string(), "100".to_string(),
            "--file".to_string(), "*.rs".to_string(),
        ];
        
        let result = parse_chunks_args(&args).unwrap();
        match result {
            Command::Chunks { db_path, limit, file_filter, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(limit, Some(100));
                assert_eq!(file_filter, Some("*.rs".to_string()));
            }
            _ => panic!("Expected Chunks command"),
        }
    }

    #[test]
    fn test_parse_chunk_by_span_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--file".to_string(), "src/main.rs".to_string(),
            "--start".to_string(), "100".to_string(),
            "--end".to_string(), "200".to_string(),
        ];
        
        let result = parse_chunk_by_span_args(&args).unwrap();
        match result {
            Command::ChunkBySpan { db_path, file_path, byte_start, byte_end, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(file_path, "src/main.rs".to_string());
                assert_eq!(byte_start, 100);
                assert_eq!(byte_end, 200);
            }
            _ => panic!("Expected ChunkBySpan command"),
        }
    }

    #[test]
    fn test_parse_chunk_by_symbol_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--symbol".to_string(), "my_function".to_string(),
        ];
        
        let result = parse_chunk_by_symbol_args(&args).unwrap();
        match result {
            Command::ChunkBySymbol { db_path, symbol_name, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(symbol_name, "my_function".to_string());
            }
            _ => panic!("Expected ChunkBySymbol command"),
        }
    }

    #[test]
    fn test_parse_ast_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--file".to_string(), "src/main.rs".to_string(),
            "--position".to_string(), "150".to_string(),
        ];
        
        let result = parse_ast_args(&args).unwrap();
        match result {
            Command::Ast { db_path, file_path, position, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(file_path, "src/main.rs".to_string());
                assert_eq!(position, Some(150));
            }
            _ => panic!("Expected Ast command"),
        }
    }

    #[test]
    fn test_parse_find_ast_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--kind".to_string(), "function".to_string(),
        ];
        
        let result = parse_find_ast_args(&args).unwrap();
        match result {
            Command::FindAst { db_path, kind, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(kind, "function".to_string());
            }
            _ => panic!("Expected FindAst command"),
        }
    }

    #[test]
    fn test_parse_reachable_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--symbol".to_string(), "main::test".to_string(),
            "--reverse".to_string(),
        ];
        
        let result = parse_reachable_args(&args).unwrap();
        match result {
            Command::Reachable { db_path, symbol_id, reverse, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(symbol_id, "main::test".to_string());
                assert!(reverse);
            }
            _ => panic!("Expected Reachable command"),
        }
    }

    #[test]
    fn test_parse_dead_code_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--entry".to_string(), "main".to_string(),
        ];
        
        let result = parse_dead_code_args(&args).unwrap();
        match result {
            Command::DeadCode { db_path, entry_symbol_id, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(entry_symbol_id, "main".to_string());
            }
            _ => panic!("Expected DeadCode command"),
        }
    }

    #[test]
    fn test_parse_cycles_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--symbol".to_string(), "main".to_string(),
        ];
        
        let result = parse_cycles_args(&args).unwrap();
        match result {
            Command::Cycles { db_path, symbol_id, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(symbol_id, Some("main".to_string()));
            }
            _ => panic!("Expected Cycles command"),
        }
    }

    #[test]
    fn test_parse_condense_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--members".to_string(),
        ];
        
        let result = parse_condense_args(&args).unwrap();
        match result {
            Command::Condense { db_path, show_members, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert!(show_members);
            }
            _ => panic!("Expected Condense command"),
        }
    }

    #[test]
    fn test_parse_paths_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--start".to_string(), "main".to_string(),
            "--end".to_string(), "helper".to_string(),
            "--max-depth".to_string(), "50".to_string(),
        ];
        
        let result = parse_paths_args(&args).unwrap();
        match result {
            Command::Paths { db_path, start_symbol_id, end_symbol_id, max_depth, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(start_symbol_id, "main".to_string());
                assert_eq!(end_symbol_id, Some("helper".to_string()));
                assert_eq!(max_depth, 50);
            }
            _ => panic!("Expected Paths command"),
        }
    }

    #[test]
    fn test_parse_slice_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--target".to_string(), "main".to_string(),
            "--direction".to_string(), "forward".to_string(),
            "--verbose".to_string(),
        ];
        
        let result = parse_slice_args(&args).unwrap();
        match result {
            Command::Slice { db_path, target, direction, verbose, .. } => {
                assert_eq!(db_path, PathBuf::from("test.db"));
                assert_eq!(target, "main".to_string());
                assert_eq!(direction, "forward".to_string());
                assert!(verbose);
            }
            _ => panic!("Expected Slice command"),
        }
    }

    #[test]
    fn test_parse_output_format_validation() {
        // Test invalid output format is rejected
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--output".to_string(), "invalid_format".to_string(),
        ];
        
        let result = parse_status_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid output format"));
    }

    #[test]
    fn test_parse_unknown_argument() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--unknown-flag".to_string(),
        ];
        
        let result = parse_status_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown argument"));
    }

    #[test]
    fn test_parse_missing_argument_value() {
        let args = vec!["--db".to_string()]; // Missing value for --db
        
        let result = parse_status_args(&args);
        assert!(result.is_err());
    }

    // ============================================================================
    // Edge Case Explosion Tests
    // ============================================================================

    /// Test empty arguments
    #[test]
    fn test_edge_empty_args() {
        let args: Vec<String> = vec![];
        
        // All parsers should fail with missing --db
        assert!(parse_status_args(&args).is_err());
        assert!(parse_files_args(&args).is_err());
        assert!(parse_watch_args(&args).is_err());
    }

    /// Test arguments in different orders
    #[test]
    fn test_edge_arg_order_independence() {
        // Order 1: --db first
        let args1 = vec![
            "--db".to_string(), "test.db".to_string(),
            "--output".to_string(), "json".to_string(),
        ];
        
        // Order 2: --output first
        let args2 = vec![
            "--output".to_string(), "json".to_string(),
            "--db".to_string(), "test.db".to_string(),
        ];
        
        let result1 = parse_status_args(&args1).unwrap();
        let result2 = parse_status_args(&args2).unwrap();
        
        match (result1, result2) {
            (Command::Status { db_path: db1, output_format: fmt1 },
             Command::Status { db_path: db2, output_format: fmt2 }) => {
                assert_eq!(db1, db2);
                assert!(matches!(fmt1, OutputFormat::Json));
                assert!(matches!(fmt2, OutputFormat::Json));
            }
            _ => panic!("Expected Status commands"),
        }
    }

    /// Test duplicate arguments (last one wins)
    #[test]
    fn test_edge_duplicate_args() {
        let args = vec![
            "--db".to_string(), "first.db".to_string(),
            "--db".to_string(), "second.db".to_string(),
        ];
        
        let result = parse_status_args(&args).unwrap();
        match result {
            Command::Status { db_path, .. } => {
                // Last --db should win
                assert_eq!(db_path, PathBuf::from("second.db"));
            }
            _ => panic!("Expected Status command"),
        }
    }

    /// Test special characters in path arguments
    #[test]
    fn test_edge_special_chars_in_paths() {
        let args = vec![
            "--db".to_string(), "/path/with spaces/file.db".to_string(),
        ];
        
        let result = parse_status_args(&args).unwrap();
        match result {
            Command::Status { db_path, .. } => {
                assert_eq!(db_path, PathBuf::from("/path/with spaces/file.db"));
            }
            _ => panic!("Expected Status command"),
        }
    }

    /// Test unicode in string arguments
    #[test]
    fn test_edge_unicode_args() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "函数_🎉".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { name, .. } => {
                assert_eq!(name, Some("函数_🎉".to_string()));
            }
            _ => panic!("Expected Find command"),
        }
    }

    /// Test very long argument values
    #[test]
    fn test_edge_long_argument_values() {
        let long_name = "a".repeat(1000);
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), long_name.clone(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { name, .. } => {
                assert_eq!(name, Some(long_name));
            }
            _ => panic!("Expected Find command"),
        }
    }

    /// Test boundary: context_lines at max (100)
    #[test]
    fn test_edge_context_lines_max() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "test".to_string(),
            "--context-lines".to_string(), "100".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { context_lines, .. } => {
                assert_eq!(context_lines, 100);
            }
            _ => panic!("Expected Find command"),
        }
    }

    /// Test boundary: context_lines above max (should be capped)
    #[test]
    fn test_edge_context_lines_above_max() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "test".to_string(),
            "--context-lines".to_string(), "200".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { context_lines, .. } => {
                // Should be capped at 100
                assert_eq!(context_lines, 100);
            }
            _ => panic!("Expected Find command"),
        }
    }

    /// Test boundary: context_lines at zero
    #[test]
    fn test_edge_context_lines_zero() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "test".to_string(),
            "--context-lines".to_string(), "0".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { context_lines, .. } => {
                assert_eq!(context_lines, 0);
            }
            _ => panic!("Expected Find command"),
        }
    }

    /// Test invalid integer values
    #[test]
    fn test_edge_invalid_integer() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--limit".to_string(), "not_a_number".to_string(),
        ];
        
        let result = parse_chunks_args(&args);
        assert!(result.is_err());
    }

    /// Test negative integer values
    #[test]
    fn test_edge_negative_integer() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--limit".to_string(), "-5".to_string(),
        ];
        
        // This should parse but may cause issues later - just verify it doesn't panic
        let result = parse_chunks_args(&args);
        // Note: The result depends on whether the type accepts negative values
        // For usize, this should fail
        assert!(result.is_err());
    }

    /// Test empty string values
    #[test]
    fn test_edge_empty_string_values() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { name, .. } => {
                assert_eq!(name, Some("".to_string()));
            }
            _ => panic!("Expected Find command"),
        }
    }

    /// Test all boolean flags can be combined
    #[test]
    fn test_edge_combined_boolean_flags() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "test".to_string(),
            "--first".to_string(),
            "--with-context".to_string(),
            "--with-callers".to_string(),
            "--with-callees".to_string(),
            "--with-semantics".to_string(),
            "--with-checksums".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { 
                first, 
                with_context, 
                with_callers, 
                with_callees, 
                with_semantics, 
                with_checksums, 
                .. 
            } => {
                assert!(first);
                assert!(with_context);
                assert!(with_callers);
                assert!(with_callees);
                assert!(with_semantics);
                assert!(with_checksums);
            }
            _ => panic!("Expected Find command"),
        }
    }

    /// Test collision field variants
    #[test]
    fn test_edge_collision_field_variants() {
        // Test all valid field values
        for (field_str, expected) in [
            ("fqn", CollisionField::Fqn),
            ("display_fqn", CollisionField::DisplayFqn),
            ("canonical_fqn", CollisionField::CanonicalFqn),
        ] {
            let args = vec![
                "--db".to_string(), "test.db".to_string(),
                "--field".to_string(), field_str.to_string(),
            ];
            
            let result = parse_collisions_args(&args).unwrap();
            match result {
                Command::Collisions { field, .. } => {
                    assert_eq!(field, expected, "Field {} should map correctly", field_str);
                }
                _ => panic!("Expected Collisions command"),
            }
        }
    }

    /// Test invalid collision field
    #[test]
    fn test_edge_invalid_collision_field() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--field".to_string(), "invalid_field".to_string(),
        ];
        
        let result = parse_collisions_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid field"));
    }

    /// Test refs direction variants
    #[test]
    fn test_edge_refs_direction_variants() {
        // Valid directions: "in" and "out"
        for direction in ["in", "out"] {
            let args = vec![
                "--db".to_string(), "test.db".to_string(),
                "--name".to_string(), "test".to_string(),
                "--path".to_string(), "src/main.rs".to_string(),
                "--direction".to_string(), direction.to_string(),
            ];
            
            let result = parse_refs_args(&args).unwrap();
            match result {
                Command::Refs { direction: dir, .. } => {
                    assert_eq!(dir, direction);
                }
                _ => panic!("Expected Refs command"),
            }
        }
    }

    /// Test invalid refs direction
    #[test]
    fn test_edge_invalid_refs_direction() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "test".to_string(),
            "--path".to_string(), "src/main.rs".to_string(),
            "--direction".to_string(), "invalid".to_string(),
        ];
        
        let result = parse_refs_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid direction"));
    }

    /// Test slice direction validation
    #[test]
    fn test_edge_slice_direction_validation() {
        // Valid: backward
        let args1 = vec![
            "--db".to_string(), "test.db".to_string(),
            "--target".to_string(), "main".to_string(),
            "--direction".to_string(), "backward".to_string(),
        ];
        assert!(parse_slice_args(&args1).is_ok());

        // Valid: forward
        let args2 = vec![
            "--db".to_string(), "test.db".to_string(),
            "--target".to_string(), "main".to_string(),
            "--direction".to_string(), "forward".to_string(),
        ];
        assert!(parse_slice_args(&args2).is_ok());

        // Invalid direction
        let args3 = vec![
            "--db".to_string(), "test.db".to_string(),
            "--target".to_string(), "main".to_string(),
            "--direction".to_string(), "sideways".to_string(),
        ];
        let result = parse_slice_args(&args3);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid direction"));
    }

    /// Test export format variants
    #[test]
    fn test_edge_export_format_variants() {
        let formats = vec!["json", "jsonl", "csv", "scip", "dot"];
        
        for format in formats {
            let args = vec![
                "--db".to_string(), "test.db".to_string(),
                "--format".to_string(), format.to_string(),
            ];
            
            let result = parse_export_args(&args);
            assert!(result.is_ok(), "Format {} should be valid", format);
        }
    }

    /// Test multiple labels
    #[test]
    fn test_edge_multiple_labels() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--label".to_string(), "label1".to_string(),
            "--label".to_string(), "label2".to_string(),
            "--label".to_string(), "label3".to_string(),
        ];
        
        let result = parse_label_args(&args).unwrap();
        match result {
            Command::Label { label, .. } => {
                assert_eq!(label, vec!["label1", "label2", "label3"]);
            }
            _ => panic!("Expected Label command"),
        }
    }

    /// Test watch mode flags interaction
    #[test]
    fn test_edge_watch_mode_flags() {
        // watch-only should disable scan_initial
        let args = vec![
            "--root".to_string(), "/test".to_string(),
            "--db".to_string(), "test.db".to_string(),
            "--watch-only".to_string(),
        ];
        
        let result = parse_watch_args(&args).unwrap();
        match result {
            Command::Watch { scan_initial, .. } => {
                assert!(!scan_initial, "watch-only should disable initial scan");
            }
            _ => panic!("Expected Watch command"),
        }
    }

    /// Test paths with special characters
    #[test]
    fn test_edge_special_path_characters() {
        let special_paths = vec![
            "/path/with-dash/file.rs",
            "/path/with_underscore/file.rs",
            "/path/with.dot/file.rs",
            "/path/with@symbol/file.rs",
            "/path/with#hash/file.rs",
        ];
        
        for path in special_paths {
            let args = vec![
                "--db".to_string(), "test.db".to_string(),
                "--file".to_string(), path.to_string(),
            ];
            
            let result = parse_get_file_args(&args);
            assert!(result.is_ok(), "Path {} should be valid", path);
        }
    }

    /// Test absolute vs relative paths
    #[test]
    fn test_edge_absolute_vs_relative_paths() {
        // Absolute path
        let args1 = vec![
            "--db".to_string(), "/absolute/path/to/test.db".to_string(),
        ];
        let result1 = parse_status_args(&args1).unwrap();
        match result1 {
            Command::Status { db_path, .. } => {
                assert_eq!(db_path, PathBuf::from("/absolute/path/to/test.db"));
            }
            _ => panic!("Expected Status command"),
        }

        // Relative path
        let args2 = vec![
            "--db".to_string(), "./relative/path/to/test.db".to_string(),
        ];
        let result2 = parse_status_args(&args2).unwrap();
        match result2 {
            Command::Status { db_path, .. } => {
                assert_eq!(db_path, PathBuf::from("./relative/path/to/test.db"));
            }
            _ => panic!("Expected Status command"),
        }
    }

    /// Test argument with equals sign (if supported by shell)
    #[test]
    fn test_edge_arguments_with_equals() {
        // This tests that we handle values that might contain equals signs
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "foo=bar".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { name, .. } => {
                assert_eq!(name, Some("foo=bar".to_string()));
            }
            _ => panic!("Expected Find command"),
        }
    }

    /// Test newlines in arguments (edge case from shell)
    #[test]
    fn test_edge_newline_in_arguments() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "line1\nline2".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { name, .. } => {
                assert_eq!(name, Some("line1\nline2".to_string()));
            }
            _ => panic!("Expected Find command"),
        }
    }

    /// Test tab characters in arguments
    #[test]
    fn test_edge_tab_in_arguments() {
        let args = vec![
            "--db".to_string(), "test.db".to_string(),
            "--name".to_string(), "col1\tcol2".to_string(),
        ];
        
        let result = parse_find_args(&args).unwrap();
        match result {
            Command::Find { name, .. } => {
                assert_eq!(name, Some("col1\tcol2".to_string()));
            }
            _ => panic!("Expected Find command"),
        }
    }
}
