//! Magellan CLI - Dumb, deterministic codebase mapping tool
//!
//! Usage: magellan <command> [arguments]

mod find_cmd;
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
    eprintln!("  magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>] [--scan-initial]");
    eprintln!("  magellan export --db <FILE>");
    eprintln!("  magellan status --db <FILE>");
    eprintln!("  magellan query --db <FILE> --file <PATH> [--kind <KIND>]");
    eprintln!("  magellan find --db <FILE> --name <NAME> [--path <PATH>]");
    eprintln!("  magellan refs --db <FILE> --name <NAME> --path <PATH> [--direction <in|out>]");
    eprintln!("  magellan files --db <FILE>");
    eprintln!("  magellan verify --root <DIR> --db <FILE>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  watch    Watch directory and index changes");
    eprintln!("  export   Export graph data to JSON");
    eprintln!("  status   Show database statistics");
    eprintln!("  query    List symbols in a file");
    eprintln!("  find     Find a symbol by name");
    eprintln!("  refs     Show calls for a symbol");
    eprintln!("  files    List all indexed files");
    eprintln!("  verify   Verify database vs filesystem");
    eprintln!();
    eprintln!("Watch arguments:");
    eprintln!("  --root <DIR>        Directory to watch recursively");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --debounce-ms <N>   Debounce delay in milliseconds (default: 500)");
    eprintln!("  --scan-initial      Scan directory for source files on startup");
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
    eprintln!("Files arguments:");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
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
    Files {
        db_path: PathBuf,
    },
    Verify {
        root_path: PathBuf,
        db_path: PathBuf,
    },
}

fn parse_args() -> Result<Command> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        return Err(anyhow::anyhow!("Missing command"));
    }

    let command = &args[1];

    match command.as_str() {
        "watch" => {
            let mut root_path: Option<PathBuf> = None;
            let mut db_path: Option<PathBuf> = None;
            let mut debounce_ms: u64 = 500;
            let mut scan_initial = false;

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
        _ => Err(anyhow::anyhow!("Unknown command: {}", command)),
    }
}

fn run_status(db_path: PathBuf) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;

    let file_count = graph.count_files()?;
    let symbol_count = graph.count_symbols()?;
    let reference_count = graph.count_references()?;

    println!("files: {}", file_count);
    println!("symbols: {}", symbol_count);
    println!("references: {}", reference_count);

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
