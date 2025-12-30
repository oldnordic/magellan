//! Query command implementation
//!
//! Lists symbols in a file, optionally filtered by kind.

use anyhow::Result;
use magellan::{CodeGraph, SymbolKind};
use std::path::PathBuf;

/// Parse a string into a SymbolKind (case-insensitive)
///
/// # Arguments
/// * `s` - String to parse
///
/// # Returns
/// Some(SymbolKind) if recognized, None otherwise
///
/// # Supported values (case-insensitive):
/// - "function", "fn" → Function
/// - "method" → Method
/// - "class", "struct" → Class
/// - "interface", "trait" → Interface
/// - "enum" → Enum
/// - "module", "mod" → Module
/// - "union" → Union
/// - "namespace", "ns" → Namespace
/// - "type", "typealias", "typealias" → TypeAlias
fn parse_symbol_kind(s: &str) -> Option<SymbolKind> {
    match s.to_lowercase().as_str() {
        "function" | "fn" => Some(SymbolKind::Function),
        "method" => Some(SymbolKind::Method),
        "class" | "struct" => Some(SymbolKind::Class),
        "interface" | "trait" => Some(SymbolKind::Interface),
        "enum" => Some(SymbolKind::Enum),
        "module" | "mod" => Some(SymbolKind::Module),
        "union" => Some(SymbolKind::Union),
        "namespace" | "ns" => Some(SymbolKind::Namespace),
        "type" | "typealias" | "type alias" => Some(SymbolKind::TypeAlias),
        _ => None,
    }
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
///
/// # Arguments
/// * `file_path` - The file path (may be relative or absolute)
/// * `root` - Optional root directory for resolving relative paths
///
/// # Returns
/// Absolute path string
///
/// # Behavior
/// - If `file_path` is absolute, return it as-is
/// - If `root` is provided, resolve `file_path` relative to `root`
/// - If `root` is None and `file_path` is relative, canonicalize from current directory
fn resolve_path(file_path: &PathBuf, root: &Option<PathBuf>) -> String {
    if file_path.is_absolute() {
        return file_path.to_string_lossy().to_string();
    }

    // file_path is relative - need to resolve it
    if let Some(ref root) = root {
        // Resolve relative to explicit root (NO GUESSING)
        root.join(file_path).to_string_lossy().to_string()
    } else {
        // No explicit root - try canonicalizing from current directory
        // This may fail if the file doesn't exist from current directory
        std::env::current_dir()
            .ok()
            .and_then(|cwd| cwd.join(file_path).canonicalize().ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string_lossy().to_string())
    }
}

/// Run the query command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `file_path` - Path to the file to query (relative or absolute)
/// * `root` - Optional root directory for resolving relative paths
/// * `kind_str` - Optional symbol kind filter string
///
/// # Displays
/// Human-readable list of symbols in the file
pub fn run_query(
    db_path: PathBuf,
    file_path: PathBuf,
    root: Option<PathBuf>,
    kind_str: Option<String>,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    // Parse kind filter if provided
    let kind_filter = match kind_str {
        Some(ref s) => {
            match parse_symbol_kind(s) {
                Some(k) => Some(k),
                None => {
                    anyhow::bail!("Unknown symbol kind: '{}'. Valid kinds: function, method, class, interface, enum, module, union, namespace, typealias", s);
                }
            }
        }
        None => None,
    };

    // Resolve file path (with explicit root if provided)
    let path_str = resolve_path(&file_path, &root);

    let symbols = graph.symbols_in_file_with_kind(&path_str, kind_filter)?;

    // Print results
    println!("{}:", path_str);

    if symbols.is_empty() {
        println!("  (no symbols found)");
    } else {
        for symbol in &symbols {
            let kind_str = format_symbol_kind(&symbol.kind);
            let name = symbol.name.as_deref().unwrap_or("(unnamed)");
            println!("  Line {:4}: {:12} {}", symbol.start_line, kind_str, name);
        }
    }

    Ok(())
}
