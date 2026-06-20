//! Language-agnostic call and reference extraction helpers.
//!
//! These helpers centralise the FQN-aware tree walk so each language parser only
//! has to supply language-specific node kinds and name-extraction closures.

use crate::ingest::{build_fqn_map, resolve_qualified_symbol, SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type ExtractNameFn<'a> = &'a dyn Fn(&tree_sitter::Node, &[u8]) -> Option<(String, &'static str)>;

/// Bundled callbacks for call extraction.
struct CallExtraction<'a> {
    is_function_node: &'a dyn Fn(&tree_sitter::Node) -> bool,
    extract_function_name: &'a dyn Fn(&tree_sitter::Node, &[u8]) -> Option<String>,
    call_node_kind: &'a str,
    extract_callee: ExtractNameFn<'a>,
}

/// Bundled callbacks for reference extraction.
struct ReferenceExtraction<'a> {
    is_reference_node: &'a dyn Fn(&tree_sitter::Node) -> bool,
    extract_reference: ExtractNameFn<'a>,
}

/// Extract function-call facts from a pre-parsed tree.
///
/// # Arguments
/// * `tree` - pre-parsed tree-sitter tree
/// * `file_path` - path of the file being analysed (for context only)
/// * `source` - file contents as bytes
/// * `symbols` - all known symbols, including symbols from other files
/// * `is_function_node` - returns true for nodes that introduce a new caller scope
/// * `extract_function_name` - closure that extracts the function name from a function node
/// * `call_node_kind` - tree-sitter node kind that represents a call
/// * `extract_callee` - closure that returns `(callee_text, node_kind)` for the callee child
#[allow(
    clippy::too_many_arguments,
    reason = "language extraction adapters pass parser callbacks and source context together"
)]
pub fn extract_calls_from_tree(
    tree: &tree_sitter::Tree,
    file_path: PathBuf,
    source: &[u8],
    symbols: &[SymbolFact],
    is_function_node: impl Fn(&tree_sitter::Node) -> bool,
    extract_function_name: impl Fn(&tree_sitter::Node, &[u8]) -> Option<String>,
    call_node_kind: &str,
    extract_callee: impl Fn(&tree_sitter::Node, &[u8]) -> Option<(String, &'static str)>,
) -> Vec<CallFact> {
    let fqn_map = build_fqn_map(symbols);
    let symbol_map: HashMap<String, &SymbolFact> = symbols
        .iter()
        .filter_map(|s| s.name.as_ref().map(|name| (name.clone(), s)))
        .collect();

    let extraction = CallExtraction {
        is_function_node: &is_function_node,
        extract_function_name: &extract_function_name,
        call_node_kind,
        extract_callee: &extract_callee,
    };

    let mut calls = Vec::new();
    walk_tree_for_calls(
        &tree.root_node(),
        source,
        &file_path,
        &symbol_map,
        &fqn_map,
        None,
        &mut calls,
        &extraction,
    );
    calls
}

#[allow(
    clippy::too_many_arguments,
    reason = "recursive walker threads parse state and symbol maps through each visit"
)]
fn walk_tree_for_calls(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbol_map: &HashMap<String, &SymbolFact>,
    fqn_map: &HashMap<String, &SymbolFact>,
    current_caller: Option<&SymbolFact>,
    calls: &mut Vec<CallFact>,
    extraction: &CallExtraction<'_>,
) {
    let kind = node.kind();

    let caller: Option<&SymbolFact> = if (extraction.is_function_node)(node) {
        (extraction.extract_function_name)(node, source)
            .and_then(|name| symbol_map.get(&name).copied())
    } else {
        current_caller
    };

    if kind == extraction.call_node_kind {
        if let Some(caller_fact) = caller {
            extract_call_in_node(
                node,
                source,
                file_path,
                caller_fact,
                symbol_map,
                fqn_map,
                calls,
                extraction.extract_callee,
            );
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_calls(
            &child, source, file_path, symbol_map, fqn_map, caller, calls, extraction,
        );
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "call resolution needs both caller context and symbol lookup tables"
)]
fn extract_call_in_node(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    caller: &SymbolFact,
    symbol_map: &HashMap<String, &SymbolFact>,
    fqn_map: &HashMap<String, &SymbolFact>,
    calls: &mut Vec<CallFact>,
    extract_callee: ExtractNameFn<'_>,
) {
    if let Some((callee_text, callee_node_kind)) = extract_callee(node, source) {
        let all_symbols: Vec<&SymbolFact> = symbol_map.values().copied().collect();
        let resolved = if is_qualified_kind(callee_node_kind) {
            resolve_qualified_symbol(
                &callee_text,
                callee_node_kind,
                file_path,
                fqn_map,
                &all_symbols,
            )
        } else {
            symbol_map.get(&callee_text).copied()
        };

        if let Some(callee_fact) = resolved {
            if matches!(callee_fact.kind, SymbolKind::Function | SymbolKind::Method) {
                let callee_name = callee_fact
                    .name
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| callee_text.clone());
                calls.push(CallFact {
                    file_path: file_path.to_path_buf(),
                    caller: caller.name.clone().unwrap_or_default(),
                    callee: callee_name,
                    caller_symbol_id: None,
                    callee_symbol_id: None,
                    byte_start: node.start_byte(),
                    byte_end: node.end_byte(),
                    start_line: node.start_position().row + 1,
                    start_col: node.start_position().column,
                    end_line: node.end_position().row + 1,
                    end_col: node.end_position().column,
                });
            }
        }
    }
}

/// Extract reference facts from a pre-parsed tree.
///
/// # Arguments
/// * `tree` - pre-parsed tree-sitter tree
/// * `file_path` - path of the file being analysed
/// * `source` - file contents as bytes
/// * `symbols` - all known symbols, including symbols from other files
/// * `is_reference_node` - returns `true` for nodes that may reference a symbol
/// * `extract_reference` - closure that returns `(reference_text, node_kind)`
pub fn extract_references_from_tree(
    tree: &tree_sitter::Tree,
    file_path: PathBuf,
    source: &[u8],
    symbols: &[SymbolFact],
    is_reference_node: impl Fn(&tree_sitter::Node) -> bool,
    extract_reference: impl Fn(&tree_sitter::Node, &[u8]) -> Option<(String, &'static str)>,
) -> Vec<ReferenceFact> {
    let fqn_map = build_fqn_map(symbols);
    let extraction = ReferenceExtraction {
        is_reference_node: &is_reference_node,
        extract_reference: &extract_reference,
    };
    let mut references = Vec::new();
    walk_tree_for_references(
        &tree.root_node(),
        source,
        &file_path,
        symbols,
        &fqn_map,
        &mut references,
        &extraction,
    );
    references
}

fn walk_tree_for_references(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[SymbolFact],
    fqn_map: &HashMap<String, &SymbolFact>,
    references: &mut Vec<ReferenceFact>,
    extraction: &ReferenceExtraction<'_>,
) {
    if (extraction.is_reference_node)(node) {
        if let Some((text, node_kind)) = (extraction.extract_reference)(node, source) {
            let all_symbols: Vec<&SymbolFact> = symbols.iter().collect();
            let resolved = if is_qualified_kind(node_kind) {
                resolve_qualified_symbol(text.as_str(), node_kind, file_path, fqn_map, &all_symbols)
            } else {
                symbols
                    .iter()
                    .find(|s| {
                        s.name
                            .as_ref()
                            .map(|n| n.as_str() == text.as_str())
                            .unwrap_or(false)
                    })
                    .map(|s| s as &SymbolFact)
            };

            if let Some(symbol) = resolved {
                let symbol_name = symbol.name.as_deref().unwrap_or(text.as_str());
                let ref_start = node.start_byte();
                if symbol.file_path != *file_path || ref_start >= symbol.byte_end {
                    references.push(ReferenceFact {
                        file_path: file_path.to_path_buf(),
                        referenced_symbol: symbol_name.to_string(),
                        byte_start: ref_start,
                        byte_end: node.end_byte(),
                        start_line: node.start_position().row + 1,
                        start_col: node.start_position().column,
                        end_line: node.end_position().row + 1,
                        end_col: node.end_position().column,
                    });
                }
            }
        }
        if is_qualified_node(node.kind()) {
            return;
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_references(
            &child, source, file_path, symbols, fqn_map, references, extraction,
        );
    }
}

fn is_qualified_kind(kind: &str) -> bool {
    matches!(
        kind,
        "scoped_identifier"
            | "qualified_identifier"
            | "namespace_qualified_name"
            | "qualified_name"
            | "field_expression"
            | "method_expression"
            | "field_access"
            | "selector_expression"
            | "attribute"
            | "member_expression"
    )
}

fn is_qualified_node(kind: &str) -> bool {
    is_qualified_kind(kind)
}
