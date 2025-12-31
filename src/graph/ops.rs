//! Core graph operations for CodeGraph
//!
//! Provides file indexing and deletion operations.

use anyhow::Result;
use std::path::PathBuf;

use super::CodeGraph;

/// Index a file into the graph (idempotent)
///
/// # Behavior
/// 1. Compute SHA-256 hash of file contents
/// 2. Upsert File node with path and hash
/// 3. DELETE all existing Symbol nodes and DEFINES edges for this file
/// 4. Detect language and parse symbols from source code
/// 5. Insert new Symbol nodes
/// 6. Create DEFINES edges from File to each Symbol
/// 7. Index calls (CALLS edges)
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path
/// * `source` - File contents as bytes
///
/// # Returns
/// Number of symbols indexed
pub fn index_file(graph: &mut CodeGraph, path: &str, source: &[u8]) -> Result<usize> {
    use crate::ingest::c::CParser;
    use crate::ingest::cpp::CppParser;
    use crate::ingest::java::JavaParser;
    use crate::ingest::javascript::JavaScriptParser;
    use crate::ingest::python::PythonParser;
    use crate::ingest::typescript::TypeScriptParser;
    use crate::ingest::{detect::Language, detect_language, Parser};

    let hash = graph.files.compute_hash(source);

    // Step 1: Find or create file node
    let file_id = graph.files.find_or_create_file_node(path, &hash)?;

    // Step 2: Delete all existing symbols for this file
    graph.symbols.delete_file_symbols(file_id)?;

    // Step 3: Detect language and parse symbols from source
    let path_buf = PathBuf::from(path);
    let language = detect_language(&path_buf);

    let symbol_facts = match language {
        Some(Language::Python) => {
            // Use Python parser
            let mut parser = PythonParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::Rust) => {
            // Use Rust parser
            let mut parser = Parser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::C) => {
            // Use C parser
            let mut parser = CParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::Cpp) => {
            // Use C++ parser
            let mut parser = CppParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::Java) => {
            // Use Java parser
            let mut parser = JavaParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::JavaScript) => {
            // Use JavaScript parser
            let mut parser = JavaScriptParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        Some(Language::TypeScript) => {
            // Use TypeScript parser
            let mut parser = TypeScriptParser::new()?;
            parser.extract_symbols(path_buf.clone(), source)
        }
        // Unknown language — return empty
        _ => Vec::new(),
    };

    // Step 4: Insert new symbol nodes and DEFINES edges
    for fact in &symbol_facts {
        let symbol_id = graph.symbols.insert_symbol_node(fact)?;
        graph.symbols.insert_defines_edge(file_id, symbol_id)?;
    }

    // Step 5: Index calls (all supported languages)
    if language.is_some() {
        let _ = super::calls::index_calls(graph, path, source);
    }

    Ok(symbol_facts.len())
}

/// Delete a file and all derived data from the graph
///
/// # Behavior
/// 1. Find File node by path
/// 2. Delete all DEFINES edges from File
/// 3. Delete all Symbol nodes that were defined by this File
/// 4. Delete the File node itself
/// 5. Remove from in-memory index
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path to delete
pub fn delete_file(graph: &mut CodeGraph, path: &str) -> Result<()> {
    let file_id = match graph.files.find_file_node(path)? {
        Some(id) => id,
        None => return Ok(()), // File doesn't exist, nothing to delete
    };

    // Delete all symbols for this file
    graph.symbols.delete_file_symbols(file_id)?;

    // Delete the file node using underlying SqliteGraph
    graph
        .files
        .backend
        .graph()
        .delete_entity(file_id.as_i64())?;

    // Remove from in-memory index
    graph.files.file_index.remove(path);

    Ok(())
}
