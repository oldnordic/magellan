//! CLI commands for AST node queries
//!
//! Provides commands for querying and displaying AST (Abstract Syntax Tree) nodes
//! from the code graph database. Supports file-based queries, position-based queries,
//! and kind-based filtering.
//!
//! # Commands
//!
//! ## `magellan ast`
//!
//! Query AST nodes for a file.
//!
//! ```bash
//! magellan ast --db <FILE> --file <PATH> [--position <OFFSET>] [--output <FORMAT>]
//! ```
//!
//! ### Arguments
//!
//! - `--db <FILE>` - Path to the Magellan database (required)
//! - `--file <PATH>` - File path to query (required)
//! - `--position <OFFSET>` - Byte offset in the file to find node at (optional)
//! - `--output <FORMAT>` - Output format: human, json, or pretty (default: human)
//!
//! ### Examples
//!
//! Show all AST nodes for a file:
//! ```bash
//! magellan ast --db .codemcp/magellan.db --file src/main.rs
//! ```
//!
//! Find node at byte position 100:
//! ```bash
//! magellan ast --db .codemcp/magellan.db --file src/main.rs --position 100
//! ```
//!
//! Output as JSON:
//! ```bash
//! magellan ast --db .codemcp/magellan.db --file src/main.rs --output json
//! ```
//!
//! ## `magellan find-ast`
//!
//! Find AST nodes by kind across all files.
//!
//! ```bash
//! magellan find-ast --db <FILE> --kind <KIND> [--output <FORMAT>]
//! ```
//!
//! ### Arguments
//!
//! - `--db <FILE>` - Path to the Magellan database (required)
//! - `--kind <KIND>` - Node kind to find (e.g., function_item, if_expression, block) (required)
//! - `--output <FORMAT>` - Output format: human, json, or pretty (default: human)
//!
//! ### Examples
//!
//! Find all if expressions:
//! ```bash
//! magellan find-ast --db .codemcp/magellan.db --kind if_expression
//! ```
//!
//! Find all function definitions as JSON:
//! ```bash
//! magellan find-ast --db .codemcp/magellan.db --kind function_item --output json
//! ```
//!
//! ### Common Node Kinds
//!
//! - `function_item` - Function definitions
//! - `struct_item` - Struct definitions
//! - `enum_item` - Enum definitions
//! - `impl_item` - Implementation blocks
//! - `if_expression` - If statements/expressions
//! - `while_expression` - While loops
//! - `for_expression` - For loops
//! - `match_expression` - Match expressions
//! - `block` - Code blocks
//! - `call_expression` - Function calls

use anyhow::Result;
use std::path::PathBuf;

use magellan::CodeGraph;
use magellan::output::{generate_execution_id, output_json, JsonResponse, OutputFormat};
use magellan::graph::AstNode;

/// Run the 'ast' command
///
/// Displays AST nodes for a file, optionally filtering by position.
/// Supports both human-readable and JSON output formats.
pub fn run_ast_command(
    db_path: PathBuf,
    file_path: String,
    position: Option<usize>,
    output_format: OutputFormat,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();

    if let Some(pos) = position {
        // Show AST at specific position
        match graph.get_ast_node_at_position(&file_path, pos)? {
            Some(node) => {
                match output_format {
                    OutputFormat::Json | OutputFormat::Pretty => {
                        let response = JsonResponse::new(
                            serde_json::json!({
                                "file_path": file_path,
                                "position": pos,
                                "node": node,
                            }),
                            &exec_id,
                        );
                        output_json(&response, output_format)?;
                    }
                    OutputFormat::Human => {
                        println!("AST node at position {} in {}:", pos, file_path);
                        print_node_tree(&graph, &node, 0)?;
                    }
                }
            }
            None => {
                eprintln!("No AST node found at position {} in {}", pos, file_path);
                std::process::exit(1);
            }
        }
    } else {
        // Show all AST nodes for the file
        let nodes = graph.get_ast_nodes_by_file(&file_path)?;

        if nodes.is_empty() {
            eprintln!("No AST nodes found for file: {}", file_path);
            std::process::exit(1);
        }

        match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let response = JsonResponse::new(
                    serde_json::json!({
                        "file_path": file_path,
                        "count": nodes.len(),
                        "nodes": nodes,
                    }),
                    &exec_id,
                );
                output_json(&response, output_format)?;
            }
            OutputFormat::Human => {
                println!("AST nodes for {} ({} nodes):", file_path, nodes.len());
                for node_with_text in nodes {
                    print_node_tree(&graph, &node_with_text.node, 0)?;
                }
            }
        }
    }

    Ok(())
}

/// Run the 'find-ast' command
///
/// Finds all AST nodes of a specific kind across all files.
/// Supports both human-readable and JSON output formats.
pub fn run_find_ast_command(
    db_path: PathBuf,
    kind: String,
    output_format: OutputFormat,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();

    let nodes = graph.get_ast_nodes_by_kind(&kind)?;

    if nodes.is_empty() {
        eprintln!("No AST nodes found with kind '{}'", kind);
        std::process::exit(1);
    }

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = JsonResponse::new(
                serde_json::json!({
                    "kind": kind,
                    "count": nodes.len(),
                    "nodes": nodes,
                }),
                &exec_id,
            );
            output_json(&response, output_format)?;
        }
        OutputFormat::Human => {
            println!("Found {} AST nodes with kind '{}':", nodes.len(), kind);
            for node in nodes {
                println!("  - {} @ {}:{}",
                    node.kind,
                    node.byte_start,
                    node.byte_end
                );
            }
        }
    }

    Ok(())
}

/// Print a node with indentation (human-readable)
///
/// Recursively prints the node and its children in a tree structure.
fn print_node_tree(graph: &CodeGraph, node: &AstNode, indent: usize) -> Result<()> {
    let prefix = "  ".repeat(indent);
    let connector = if indent == 0 { "" } else { "└── " };

    println!("{}{}{} ({}:{})",
        prefix,
        connector,
        node.kind,
        node.byte_start,
        node.byte_end
    );

    // Print children if this node has an ID
    if let Some(node_id) = node.id {
        let children = graph.get_ast_children(node_id)?;
        for child in children {
            print_node_tree(graph, &child, indent + 1)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_node_tree_basic() {
        // This is a basic compile-time test
        // Real testing would require a populated database
        let node = AstNode {
            id: None,
            parent_id: None,
            kind: "Function".to_string(),
            byte_start: 0,
            byte_end: 100,
        };

        // Verify we can construct the node
        assert_eq!(node.kind, "Function");
        assert_eq!(node.byte_start, 0);
        assert_eq!(node.byte_end, 100);
    }
}
