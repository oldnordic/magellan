//! Magellan CLI - Dumb, deterministic codebase mapping tool
//!
//! Usage: magellan <command> [arguments]

mod find_cmd;
mod get_cmd;
mod query_cmd;
mod refs_cmd;
mod verify_cmd;
mod watch_cmd;

use anyhow::Result;
use magellan::{CodeGraph, WatcherConfig};
use std::path::PathBuf;
use std::process::ExitCode;

fn print_usage() {
    eprintln!("Magellan - Multi-language codebase mapping tool");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  magellan <command> [arguments]");
    eprintln!("  magellan --help");
    eprintln!();
    eprintln!("  magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>] [--watch-only]");
    eprintln!("  magellan export --db <FILE>");
    eprintln!("  magellan status --db <FILE>");
    eprintln!("  magellan query --db <FILE> --file <PATH> [--kind <KIND>]");
    eprintln!("  magellan find --db <FILE> --name <NAME> [--path <PATH>]");
    eprintln!("  magellan refs --db <FILE> --name <NAME> --path <PATH> [--direction <in|out>]");
    eprintln!("  magellan get --db <FILE> --file <PATH> --symbol <NAME>");
    eprintln!("  magellan get-file --db <FILE> --file <PATH>");
    eprintln!("  magellan files --db <FILE>");
    eprintln!("  magellan label --db <FILE> [--label <LABEL>]... [--list] [--count] [--show-code]");
    eprintln!("  magellan verify --root <DIR> --db <FILE>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  watch     Watch directory and index changes");
    eprintln!("  export    Export graph data to JSON");
    eprintln!("  status    Show database statistics");
    eprintln!("  query     List symbols in a file");
    eprintln!("  find      Find a symbol by name");
    eprintln!("  refs      Show calls for a symbol");
    eprintln!("  get       Get source code for a specific symbol");
    eprintln!("  get-file  Get all source code chunks for a file");
    eprintln!("  files     List all indexed files");
    eprintln!("  label     Query symbols by label (language, kind, etc.)");
    eprintln!("  verify    Verify database vs filesystem");
    eprintln!();
    eprintln!("Watch arguments:");
    eprintln!("  --root <DIR>        Directory to watch recursively");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --debounce-ms <N>   Debounce delay in milliseconds (default: 500)");
    eprintln!("  --watch-only        Watch for changes only; skip initial directory scan baseline");
    eprintln!("  --scan-initial      Scan directory for source files on startup (default: true; disabled by --watch-only)");
    eprintln!();
    eprintln!("Export arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!();
    eprintln!("Status arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!();
    eprintln!("Query arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --file <PATH>       File path to query");
    eprintln!("  --kind <KIND>       Filter by symbol kind (optional)");
    eprintln!();
    eprintln!("Find arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --name <NAME>       Symbol name to find");
    eprintln!("  --path <PATH>       Limit search to specific file (optional)");
    eprintln!();
    eprintln!("Refs arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --name <NAME>       Symbol name to query");
    eprintln!("  --path <PATH>       File path containing the symbol");
    eprintln!("  --direction <in|out> Show incoming (in) or outgoing (out) calls (default: in)");
    eprintln!();
    eprintln!("Get arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --file <PATH>       File path containing the symbol");
    eprintln!("  --symbol <NAME>     Symbol name to retrieve");
    eprintln!();
    eprintln!("Get-file arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --file <PATH>       File path to retrieve code for");
    eprintln!();
    eprintln!("Files arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!();
    eprintln!("Label arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --label <LABEL>     Label to query (can specify multiple for AND semantics)");
    eprintln!("  --list             List all available labels with counts");
    eprintln!("  --count            Count entities with specified label(s)");
    eprintln!("  --show-code        Show source code for each matching symbol");
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
    },
    Export {
        db_path: PathBuf,
    },
    Status {
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
    },
    Find {
        db_path: PathBuf,
        name: Option<String>,
        root: Option<PathBuf>,
        path: Option<PathBuf>,
        glob_pattern: Option<String>,
    },
    Refs {
        db_path: PathBuf,
        name: String,
        root: Option<PathBuf>,
        path: PathBuf,
        direction: String,
    },
    Get {
        db_path: PathBuf,
        file_path: String,
        symbol_name: String,
    },
    GetFile {
        db_path: PathBuf,
        file_path: String,
    },
    Files {
        db_path: PathBuf,
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
}

fn parse_args() -> Result<Command> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        return Err(anyhow::anyhow!("Missing command"));
    }

    let command = &args[1];

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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let root_path = root_path.ok_or_else(|| anyhow::anyhow!("--root is required"))?;
            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;
            let config = WatcherConfig { debounce_ms };

            // Precedence: --watch-only forces scan_initial to false
            let scan_initial = if watch_only { false } else { scan_initial };

            Ok(Command::Watch {
                root_path,
                db_path,
                config,
                scan_initial,
            })
        }
        "export" => {
            let mut db_path: Option<PathBuf> = None;

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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Export { db_path })
        }
        "status" => {
            let mut db_path: Option<PathBuf> = None;

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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Status { db_path })
        }
        "query" => {
            let mut db_path: Option<PathBuf> = None;
            let mut file_path: Option<PathBuf> = None;
            let mut root: Option<PathBuf> = None;
            let mut kind: Option<String> = None;
            let mut explain = false;
            let mut symbol: Option<String> = None;
            let mut show_extent = false;

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
            })
        }
        "find" => {
            let mut db_path: Option<PathBuf> = None;
            let mut name: Option<String> = None;
            let mut root: Option<PathBuf> = None;
            let mut path: Option<PathBuf> = None;
            let mut glob_pattern: Option<String> = None;

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
            })
        }
        "refs" => {
            let mut db_path: Option<PathBuf> = None;
            let mut name: Option<String> = None;
            let mut root: Option<PathBuf> = None;
            let mut path: Option<PathBuf> = None;
            let mut direction = String::from("in"); // default

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
                    "--direction" => {
                        if i + 1 >= args.len() {
                            return Err(anyhow::anyhow!("--direction requires an argument"));
                        }
                        direction = args[i + 1].clone();
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
                direction,
            })
        }
        "files" => {
            let mut db_path: Option<PathBuf> = None;

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
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

            Ok(Command::Files { db_path })
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

fn run_status(db_path: PathBuf) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;

    let file_count = graph.count_files()?;
    let symbol_count = graph.count_symbols()?;
    let reference_count = graph.count_references()?;
    let chunk_count = graph.count_chunks()?;

    println!("files: {}", file_count);
    println!("symbols: {}", symbol_count);
    println!("references: {}", reference_count);
    println!("code_chunks: {}", chunk_count);

    Ok(())
}

fn run_export(db_path: PathBuf) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let json = graph.export_json()?;
    println!("{}", json);
    Ok(())
}

fn run_files(db_path: PathBuf) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let file_nodes = graph.all_file_nodes()?;

    if file_nodes.is_empty() {
        println!("0 indexed files");
    } else {
        println!("{} indexed files:", file_nodes.len());
        let mut paths: Vec<_> = file_nodes.keys().collect();
        paths.sort();
        for path in paths {
            println!("  {}", path);
        }
    }

    Ok(())
}

/// Run label query command
/// Usage: magellan label --db <FILE> --label <LABEL> [--list] [--count] [--show-code]
fn run_label(db_path: PathBuf, labels: Vec<String>, list: bool, count: bool, show_code: bool) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;

    // List all labels mode
    if list {
        let all_labels = graph.get_all_labels()?;
        println!("{} labels in use:", all_labels.len());
        for label in all_labels {
            let count = graph.count_entities_by_label(&label)?;
            println!("  {} ({})", label, count);
        }
        return Ok(());
    }

    // Count mode
    if count {
        if labels.is_empty() {
            return Err(anyhow::anyhow!("--count requires --label"));
        }
        for label in &labels {
            let entity_count = graph.count_entities_by_label(label)?;
            println!("{}: {} entities", label, entity_count);
        }
        return Ok(());
    }

    // Query mode - get symbols by label(s)
    if labels.is_empty() {
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
            println!("{} symbols with labels [{}]:", results.len(), labels.join(", "));
        }

        for result in results {
            println!();
            println!("  {} ({}) in {} [{}-{}]",
                result.name, result.kind, result.file_path, result.byte_start, result.byte_end
            );

            // Show code chunk if requested
            if show_code {
                // Get code chunk by exact byte span instead of by name
                // This avoids getting chunks for other symbols with the same name
                if let Ok(Some(chunk)) = graph.get_code_chunk_by_span(&result.file_path, result.byte_start, result.byte_end) {
                    for line in chunk.content.lines() {
                        println!("    {}", line);
                    }
                }
            }
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return ExitCode::from(1);
    }

    match parse_args() {
        Ok(Command::Status { db_path }) => {
            if let Err(e) = run_status(db_path) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Export { db_path }) => {
            if let Err(e) = run_export(db_path) {
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
        }) => {
            if let Err(e) =
                query_cmd::run_query(db_path, file_path, root, kind, explain, symbol, show_extent)
            {
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
        }) => {
            if let Err(e) = find_cmd::run_find(db_path, name, root, path, glob_pattern) {
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
            direction,
        }) => {
            if let Err(e) = refs_cmd::run_refs(db_path, name, root, path, direction) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Files { db_path }) => {
            if let Err(e) = run_files(db_path) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Get {
            db_path,
            file_path,
            symbol_name,
        }) => {
            if let Err(e) = get_cmd::run_get(db_path, file_path, symbol_name) {
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
        }) => {
            if let Err(e) = watch_cmd::run_watch(root_path, db_path, config, scan_initial) {
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
