//! Get command - Retrieve source code for symbols
//!
//! Usage: magellan get --db <FILE> --file <PATH> --symbol <NAME>

use anyhow::Result;
use std::path::PathBuf;

use crate::CodeGraph;

pub fn run_get(
    db_path: PathBuf,
    file_path: String,
    symbol_name: String,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;

    let chunks = graph.get_code_chunks_for_symbol(&file_path, &symbol_name)?;

    if chunks.is_empty() {
        eprintln!("No code chunks found for symbol '{}' in file '{}'", symbol_name, file_path);
        return Ok(());
    }

    for chunk in chunks {
        println!("// Symbol: {} in {}", chunk.symbol_name.as_ref().unwrap_or(&symbol_name), chunk.file_path);
        println!("// Kind: {}", chunk.symbol_kind.as_ref().unwrap_or(&"?".to_string()));
        println!("// Bytes: {}-{}", chunk.byte_start, chunk.byte_end);
        println!("{}", chunk.content);
        println!();
    }

    Ok(())
}

pub fn run_get_file(db_path: PathBuf, file_path: String) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;

    let chunks = graph.get_code_chunks(&file_path)?;

    if chunks.is_empty() {
        eprintln!("No code chunks found for file '{}'", file_path);
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

    Ok(())
}
