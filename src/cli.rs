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
        "watch" => {
            let mut root_path: Option<PathBuf> = None;
            let mut db_path: Option<PathBuf> = None;
            let mut debounce_ms: u64 = 500;
            let mut watch_only = false;
            let mut scan_initial = true; // Default: true (scan on startup)
            let mut gitignore_aware = true; // Default: true (respect .gitignore)
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

            // Watch-only mode: don't scan initial directory (legacy behavior)
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
            let mut filters = ExportFilters::default();

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
                        format = match args[i + 1].as_str() {
                            "json" => ExportFormat::Json,
                            "jsonl" => ExportFormat::JsonL,
                            "csv" => ExportFormat::Csv,
                            "scip" => ExportFormat::Scip,
                            "dot" => ExportFormat::Dot,
                            _ => {
                                return Err(anyhow::anyhow!(
                                    "Invalid format: {}. Must be json, jsonl, csv, scip, or dot",
                                    args[i + 1]
                                ))
                            }
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
                        collisions_field = match args[i + 1].as_str() {
                            "fqn" => CollisionField::Fqn,
                            "display_fqn" => CollisionField::DisplayFqn,
                            "canonical_fqn" => CollisionField::CanonicalFqn,
                            _ => {
                                return Err(anyhow::anyhow!(
                                    "Invalid collisions field: {}. Must be fqn, display_fqn, or canonical_fqn",
                                    args[i + 1]
                                ))
                            }
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
            let mut context_lines = 3;

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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
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
            let mut context_lines = 3;

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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
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
        "refs" => {
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
        "get" => {
            let mut db_path: Option<PathBuf> = None;
            let mut file_path: Option<String> = None;
            let mut symbol_name: Option<String> = None;
            let mut output_format = OutputFormat::Human;
            let mut with_context = false;
            let mut with_semantics = false;
            let mut with_checksums = false;
            let mut context_lines = 3;

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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let file_path = file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;
            let symbol_name =
                symbol_name.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

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

            Ok(Command::GetFile {
                db_path,
                file_path,
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

            let root_path =
                root_path.ok_or_else(|| anyhow::anyhow!("--root is required"))?;
            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Verify {
                root_path,
                db_path,
            })
        }
        "label" => {
            let mut db_path: Option<PathBuf> = None;
            let mut label = Vec::new();
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
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
        "collisions" => {
            let mut db_path: Option<PathBuf> = None;
            let mut field = CollisionField::Fqn;
            let mut limit = 100;
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
                output_format,
            })
        }
        "migrate-backend" => {
            let mut input_db: Option<PathBuf> = None;
            let mut output_db: Option<PathBuf> = None;
            let mut export_dir: Option<PathBuf> = None;
            let mut dry_run = false;
            let mut output_format = OutputFormat::Human;

            let mut i = 2;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let input_db = input_db
                .ok_or_else(|| anyhow::anyhow!("--input is required"))?;
            let output_db = output_db
                .ok_or_else(|| anyhow::anyhow!("--output is required"))?;

            Ok(Command::MigrateBackend {
                input_db,
                output_db,
                export_dir,
                dry_run,
                output_format,
            })
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let file_path =
                file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;
            let byte_start =
                byte_start.ok_or_else(|| anyhow::anyhow!("--start is required"))?;
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
            let mut file_filter: Option<String> = None;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let symbol_name =
                symbol_name.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

            Ok(Command::ChunkBySymbol {
                db_path,
                symbol_name,
                file_filter,
                output_format,
            })
        }
        "ast" => {
            let mut db_path: Option<PathBuf> = None;
            let mut file_path: Option<String> = None;
            let mut position: Option<usize> = None;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let file_path =
                file_path.ok_or_else(|| anyhow::anyhow!("--file is required"))?;

            Ok(Command::Ast {
                db_path,
                file_path,
                position,
                output_format,
            })
        }
        "find-ast" => {
            let mut db_path: Option<PathBuf> = None;
            let mut kind: Option<String> = None;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
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
        "reachable" => {
            let mut db_path: Option<PathBuf> = None;
            let mut symbol_id: Option<String> = None;
            let mut reverse = false;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let symbol_id =
                symbol_id.ok_or_else(|| anyhow::anyhow!("--symbol is required"))?;

            Ok(Command::Reachable {
                db_path,
                symbol_id,
                reverse,
                output_format,
            })
        }
        "dead-code" => {
            let mut db_path: Option<PathBuf> = None;
            let mut entry_symbol_id: Option<String> = None;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let entry_symbol_id =
                entry_symbol_id.ok_or_else(|| anyhow::anyhow!("--entry is required"))?;

            Ok(Command::DeadCode {
                db_path,
                entry_symbol_id,
                output_format,
            })
        }
        "cycles" => {
            let mut db_path: Option<PathBuf> = None;
            let mut symbol_id: Option<String> = None;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Cycles {
                db_path,
                symbol_id,
                output_format,
            })
        }
        "condense" => {
            let mut db_path: Option<PathBuf> = None;
            let mut show_members = false;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Condense {
                db_path,
                show_members,
                output_format,
            })
        }
        "paths" => {
            let mut db_path: Option<PathBuf> = None;
            let mut start_symbol_id: Option<String> = None;
            let mut end_symbol_id: Option<String> = None;
            let mut max_depth = 100;
            let mut max_paths = 1000;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let start_symbol_id =
                start_symbol_id.ok_or_else(|| anyhow::anyhow!("--start is required"))?;

            Ok(Command::Paths {
                db_path,
                start_symbol_id,
                end_symbol_id,
                max_depth,
                max_paths,
                output_format,
            })
        }
        "slice" => {
            let mut db_path: Option<PathBuf> = None;
            let mut target: Option<String> = None;
            let mut direction = "backward".to_string();
            let mut verbose = false;
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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
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
        _ => Err(anyhow::anyhow!("Unknown command: {}", command)),
    }
}

/// Convenience wrapper around parse_args_impl that uses the version module
pub fn parse_args() -> Result<Command> {
    parse_args_impl(|| {
        println!("{}", crate::version::version());
    })
}
