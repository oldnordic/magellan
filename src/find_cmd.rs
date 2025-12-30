//! Find command implementation
//!
//! Finds a symbol by name, optionally limited to a specific file.

use anyhow::Result;
use magellan::{CodeGraph, SymbolKind};
use std::path::PathBuf;

/// Represents a found symbol with its file and node ID
struct FoundSymbol {
    kind: SymbolKind,
    file: String,
    line: usize,
    col: usize,
    node_id: i64,
}

/// Format a SymbolKind for display
fn format_symbol_kind(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "Function",
        SymbolKind::Method => "Method",
        SymbolKind::Class => "Class",
        SymbolKind::Interface => "Interface",
        SymbolKind::Enum => "Enum",
        SymbolKind::Module => "Module",
        SymbolKind::Union => "Union",
        SymbolKind::Namespace => "Namespace",
        SymbolKind::TypeAlias => "TypeAlias",
        SymbolKind::Unknown => "Unknown",
    }
}

/// Resolve a file path against an optional root directory
fn resolve_path(file_path: &PathBuf, root: &Option<PathBuf>) -> String {
    if file_path.is_absolute() {
        return file_path.to_string_lossy().to_string();
    }

    if let Some(ref root) = root {
        root.join(file_path).to_string_lossy().to_string()
    } else {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| cwd.join(file_path).canonicalize().ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string_lossy().to_string())
    }
}

/// Find a symbol in a specific file by name
///
/// Returns the first matching symbol with its node ID
fn find_in_file(graph: &mut CodeGraph, file_path: &str, name: &str) -> Result<Option<FoundSymbol>> {
    let node_id = match graph.symbol_id_by_name(file_path, name)? {
        Some(id) => id,
        None => return Ok(None),
    };

    // Get all symbols in the file to find the matching one
    let symbols = graph.symbols_in_file(file_path)?;

    for symbol in symbols {
        if let Some(symbol_name) = &symbol.name {
            if symbol_name == name {
                return Ok(Some(FoundSymbol {
                    kind: symbol.kind,
                    file: symbol.file_path.to_string_lossy().to_string(),
                    line: symbol.start_line,
                    col: symbol.start_col,
                    node_id,
                }));
            }
        }
    }

    Ok(None)
}

/// Find a symbol across all files by name
///
/// Returns all matching symbols
fn find_all_files(graph: &mut CodeGraph, name: &str) -> Result<Vec<FoundSymbol>> {
    let mut results = Vec::new();

    // Get all indexed files
    let file_nodes = graph.all_file_nodes()?;

    // Search each file for the symbol
    for file_path in file_nodes.keys() {
        if let Some(node_id) = graph.symbol_id_by_name(file_path, name)? {
            let symbols = graph.symbols_in_file(file_path)?;
            for symbol in symbols {
                if let Some(symbol_name) = &symbol.name {
                    if symbol_name == name {
                        results.push(FoundSymbol {
                            kind: symbol.kind.clone(),
                            file: symbol.file_path.to_string_lossy().to_string(),
                            line: symbol.start_line,
                            col: symbol.start_col,
                            node_id,
                        });
                        break; // Found in this file, move to next
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Run the find command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `name` - Symbol name to find
/// * `root` - Optional root directory for resolving relative paths
/// * `path` - Optional file path to limit search
///
/// # Displays
/// Human-readable symbol details
pub fn run_find(
    db_path: PathBuf,
    name: String,
    root: Option<PathBuf>,
    path: Option<PathBuf>,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    let results = match path {
        Some(file_path) => {
            let path_str = resolve_path(&file_path, &root);
            match find_in_file(&mut graph, &path_str, &name)? {
                Some(symbol) => vec![symbol],
                None => vec![],
            }
        }
        None => find_all_files(&mut graph, &name)?,
    };

    if results.is_empty() {
        println!("Symbol '{}' not found", name);
    } else if results.len() == 1 {
        let symbol = &results[0];
        println!("Found \"{}\":", name);
        println!("  File:     {}", symbol.file);
        println!("  Kind:     {}", format_symbol_kind(&symbol.kind));
        println!("  Location: Line {}, Column {}", symbol.line, symbol.col);
        println!("  Node ID:  {}", symbol.node_id);
    } else {
        println!("Found {} symbols named \"{}\":", results.len(), name);
        for (i, symbol) in results.iter().enumerate() {
            println!();
            println!("  [{}]", i + 1);
            println!("    File:     {}", symbol.file);
            println!("    Kind:     {}", format_symbol_kind(&symbol.kind));
            println!("    Location: Line {}, Column {}", symbol.line, symbol.col);
        }
    }

    Ok(())
}
