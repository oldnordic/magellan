//! Get command - Retrieve source code for symbols
//!
//! Usage: magellan get --db <FILE> --file <PATH> --symbol <NAME>

use anyhow::Result;
use std::path::PathBuf;

use crate::{CodeGraph, generate_execution_id};

pub fn run_get(
    db_path: PathBuf,
    file_path: String,
    symbol_name: String,
) -> Result<()> {
    // Build args for execution tracking
    let args = vec![
        "get".to_string(),
        "--db".to_string(),
        db_path.to_string_lossy().to_string(),
        "--file".to_string(),
        file_path.clone(),
        "--symbol".to_string(),
        symbol_name.clone(),
    ];

    let graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    let chunks = graph.get_code_chunks_for_symbol(&file_path, &symbol_name)?;

    if chunks.is_empty() {
        eprintln!("No code chunks found for symbol '{}' in file '{}'", symbol_name, file_path);
        graph.execution_log().finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    for chunk in chunks {
        println!("// Symbol: {} in {}", chunk.symbol_name.as_ref().unwrap_or(&symbol_name), chunk.file_path);
        println!("// Kind: {}", chunk.symbol_kind.as_ref().unwrap_or(&"?".to_string()));
        println!("// Bytes: {}-{}", chunk.byte_start, chunk.byte_end);
        println!("{}", chunk.content);
        println!();
    }

    graph.execution_log().finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

pub fn run_get_file(db_path: PathBuf, file_path: String) -> Result<()> {
    // Build args for execution tracking
    let args = vec![
        "get-file".to_string(),
        "--db".to_string(),
        db_path.to_string_lossy().to_string(),
        "--file".to_string(),
        file_path.clone(),
    ];

    let graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    let chunks = graph.get_code_chunks(&file_path)?;

    if chunks.is_empty() {
        eprintln!("No code chunks found for file '{}'", file_path);
        graph.execution_log().finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return Ok(());
    }

    println!("// {} code chunks in {}", chunks.len(), file_path);
    println!();

    for chunk in chunks {
        let symbol = chunk.symbol_name.as_deref().unwrap_or("<unnamed>");
        let kind = chunk.symbol_kind.as_deref().unwrap_or("?");

        println!("// {} ({}) [{}-{}]", symbol, kind, chunk.byte_start, chunk.byte_end);
        println!("{}", chunk.content);
        println!();
    }

    graph.execution_log().finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}
