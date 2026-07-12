use crate::cli::Command;
use anyhow::{Context, Result};
use magellan::OutputFormat;
use std::path::PathBuf;

use crate::cli::parsers::*;
use crate::db_resolver::resolve_db_path;
use crate::service::registry::Registry;

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

// ============================================================================
// Semantic Parsers
// ============================================================================

/// Parse the `search` command arguments
pub fn parse_search_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut pattern: Option<String> = None;
    let mut limit = 20usize;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--limit" => {
                let value = parse_required_arg(args, &mut i, "--limit")?;
                limit = value.parse().context("--limit must be a number")?;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            s if s.starts_with("--") => {
                return Err(anyhow::anyhow!("Unknown flag: {s}"));
            }
            s if !s.is_empty() => {
                if pattern.is_some() {
                    return Err(anyhow::anyhow!("search takes one pattern argument"));
                }
                pattern = Some(s.to_string());
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    let pattern = pattern.ok_or_else(|| anyhow::anyhow!("search requires a pattern argument"))?;
    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Search {
        db_path,
        pattern,
        limit,
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
            "--kind" => {
                let value = parse_required_arg(args, &mut i, "--kind")?;
                kind = Some(value);
            }
            "--explain" => {
                explain = true;
                i += 1;
            }
            "--symbol" => {
                let value = parse_required_arg(args, &mut i, "--symbol")?;
                symbol = Some(value);
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
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
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
                let value = parse_required_arg(args, &mut i, "--context-lines")?;
                context_lines = value.parse().map_err(|_| {
                    anyhow::anyhow!("Invalid context lines: {}. Must be a number", value)
                })?;
                // Cap context lines at 100 maximum
                if context_lines > 100 {
                    context_lines = 100;
                }
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            "--limit" => {
                let value = parse_required_arg(args, &mut i, "--limit")?;
                limit = Some(value.parse()?);
            }
            "--file" | "--path" => {
                let flag = args[i].as_str();
                let value = parse_required_arg(args, &mut i, flag)?;
                file_filter = Some(value);
            }
            "--kind" => {
                let value = parse_required_arg(args, &mut i, "--kind")?;
                kind_filter = Some(value);
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--file" | "--path" => {
                let flag = args[i].as_str();
                let value = parse_required_arg(args, &mut i, flag)?;
                file_path = Some(value);
            }
            "--start" => {
                let value = parse_required_arg(args, &mut i, "--start")?;
                byte_start = Some(value.parse()?);
            }
            "--end" => {
                let value = parse_required_arg(args, &mut i, "--end")?;
                byte_end = Some(value.parse()?);
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--symbol" => {
                let value = parse_required_arg(args, &mut i, "--symbol")?;
                symbol_name = Some(value);
            }
            "--file" | "--path" => {
                let flag = args[i].as_str();
                let value = parse_required_arg(args, &mut i, flag)?;
                file_filter = Some(value);
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
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

/// Parse the `cypher` command arguments
pub fn parse_cypher_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut query: Option<String> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--query" | "-q" => {
                let value = parse_required_arg(args, &mut i, "--query")?;
                query = Some(value);
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
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
    let mut output_format = OutputFormat::Human;
    let mut depth = 2usize;
    let mut budget = 4000usize;
    let mut limit = 5usize;
    let mut concise = false;
    let mut with_llmgrep = false;
    let mut with_mirage = false;
    let mut tokens: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
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
            "--tokens" => {
                let v = parse_required_arg(args, &mut i, "--tokens")?;
                tokens = Some(
                    v.parse()
                        .map_err(|_| anyhow::anyhow!("--tokens must be a positive integer"))?,
                );
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
        output_format,
        depth,
        budget,
        limit,
        concise,
        with_llmgrep,
        with_mirage,
        tokens,
    })
}

/// Parse the `explore` command arguments
pub fn parse_explore_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut symbol: Option<String> = None;
    let mut id: Option<i64> = None;
    let mut edges = false;
    let mut callers = false;
    let mut callees = false;
    let mut chain: Option<String> = None;
    let mut depth = 2u32;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--symbol" | "-s" => {
                let value = parse_required_arg(args, &mut i, "--symbol")?;
                symbol = Some(value);
            }
            "--id" => {
                let value = parse_required_arg(args, &mut i, "--id")?;
                id = Some(
                    value
                        .parse()
                        .map_err(|_| anyhow::anyhow!("--id must be integer"))?,
                );
            }
            "--edges" | "-e" => {
                edges = true;
                i += 1;
            }
            "--callers" => {
                callers = true;
                i += 1;
            }
            "--callees" => {
                callees = true;
                i += 1;
            }
            "--chain" => {
                let value = parse_required_arg(args, &mut i, "--chain")?;
                chain = Some(value);
            }
            "--depth" => {
                let v = parse_required_arg(args, &mut i, "--depth")?;
                depth = v
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--depth must be integer"))?;
            }
            "--json" | "-j" => {
                json = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    let db_path = resolve_db_path(db_path)?;
    Ok(Command::Explore {
        db_path,
        symbol,
        id,
        edges,
        callers,
        callees,
        chain,
        depth,
        json,
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--name" | "-n" => {
                let value = parse_required_arg(args, &mut i, "--name")?;
                name = Some(value);
            }
            "--dim" => {
                let value = parse_required_arg(args, &mut i, "--dim")?;
                dim = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--dim must be a number"))?;
            }
            "--m" => {
                let value = parse_required_arg(args, &mut i, "--m")?;
                m = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--m must be a number"))?;
            }
            "--ef-construction" => {
                let value = parse_required_arg(args, &mut i, "--ef-construction")?;
                ef_construction = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--ef-construction must be a number"))?;
            }
            "--ef-search" => {
                let value = parse_required_arg(args, &mut i, "--ef-search")?;
                ef_search = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--ef-search must be a number"))?;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
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
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--name" | "-n" => {
                let value = parse_required_arg(args, &mut i, "--name")?;
                name = Some(value);
            }
            "--vector" | "-v" => {
                let value = parse_required_arg(args, &mut i, "--vector")?;
                vector = Some(value);
            }
            "--k" => {
                let value = parse_required_arg(args, &mut i, "--k")?;
                k = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--k must be a number"))?;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
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

/// Parse the `telemetry` command arguments
pub fn parse_telemetry_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut recent = false;
    let mut phases: Option<String> = None;
    let mut limit = 20usize;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--recent" => {
                recent = true;
                i += 1;
            }
            "--phases" => {
                let value = parse_required_arg(args, &mut i, "--phases")?;
                phases = Some(value);
            }
            "--limit" => {
                let value = parse_required_arg(args, &mut i, "--limit")?;
                limit = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--limit must be a number"))?;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => i += 1,
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Telemetry {
        db_path,
        recent,
        phases,
        limit,
        output_format,
    })
}

pub fn parse_hopgraph_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut query: Option<String> = None;
    let mut k = 10usize;
    let mut hops = 0u32;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--k" => {
                let value = parse_required_arg(args, &mut i, "--k")?;
                k = value.parse().unwrap_or(10);
            }
            "--hops" => {
                let value = parse_required_arg(args, &mut i, "--hops")?;
                hops = value.parse().unwrap_or(0);
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            other => {
                if !other.starts_with('-') && query.is_none() {
                    query = Some(other.to_string());
                }
                i += 1;
            }
        }
    }

    let db_path = resolve_db_path(db_path)?;
    let query = query.ok_or_else(|| anyhow::anyhow!("hopgraph requires a query argument"))?;

    Ok(Command::Hopgraph {
        db_path,
        query,
        k,
        hops,
        output_format,
    })
}

pub fn parse_embed_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut force = false;
    let mut batch_size: Option<usize> = None;
    let mut num_parallel: Option<usize> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db_path = Some(PathBuf::from(value));
            }
            "--force" => {
                force = true;
                i += 1;
            }
            "--batch-size" => {
                let value = parse_required_arg(args, &mut i, "--batch-size")?;
                batch_size = Some(
                    value
                        .parse()
                        .map_err(|_| anyhow::anyhow!("Invalid batch-size: {}", value))?,
                );
            }
            "--num-parallel" => {
                let value = parse_required_arg(args, &mut i, "--num-parallel")?;
                num_parallel = Some(
                    value
                        .parse()
                        .map_err(|_| anyhow::anyhow!("Invalid num-parallel: {}", value))?,
                );
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            other => {
                return Err(anyhow::anyhow!(
                    "Unknown argument: '{}'. Usage: magellan embed [--db PATH] [--force] [--batch-size N] [--num-parallel N]",
                    other
                ));
            }
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Embed {
        db_path,
        force,
        batch_size,
        num_parallel,
        output_format,
    })
}
